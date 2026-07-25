# Development

## Architecture

`sf` is a single Rust binary that manages a flat directory hierarchy with semantic search. It follows the same design pattern as [cdm](https://github.com/myersm0/cdm): a compiled binary handles the logic and writes the selected path to stdout, while a thin shell wrapper captures that output and `cd`s into it. The picker UI goes to stderr so it doesn't interfere.

### Module layout

```
src/
├── main.rs              CLI definition (clap) and subcommand dispatch
├── commands/
│   ├── new.rs           Create a new directory with random hex key
│   ├── import.rs        Register an existing directory
│   ├── info.rs          Display metadata for a key
│   ├── search.rs        Semantic + metadata search with picker
│   ├── sync.rs          (Re)embed all directories via ollama
│   ├── coaccess.rs      NPMI co-access scoring over visit log
│   ├── edit.rs          Open .meta.json in $EDITOR, re-embed if changed
│   └── audit.rs         Scan locations, report backup status
├── config/
│   └── settings.rs      TOML config with platform-appropriate defaults
├── db/
│   └── schema.rs        SQLite schema and all database operations
├── embed/
│   ├── mod.rs           EmbeddingClient trait, backend dispatch, byte codec, truncation
│   ├── ollama.rs        ollama backend (HTTP client with request timeouts)
│   ├── openai.rs        OpenAI-compatible backend (OAuth2 client credentials)
│   ├── content.rs       Text gathering and content hashing
│   └── similarity.rs    Cosine similarity (and unused elbow method)
├── meta/
│   └── schema.rs        .meta.json serde struct, load/save
└── picker/
    └── menu.rs          Raw-mode terminal picker (ported from cdm)
```

### Data flow

**Creating a directory:** `sf new` generates a random 6-digit hex key, creates the directory at the specified root (or default `contents_path`), writes `.meta.json`, and inserts a row into the `directories` table with that root registered as the first location.

**Importing:** `sf import <key>` reads an existing `.meta.json` and registers it in the database. Accepts `--path` for non-default locations. No embedding happens at import time.

**Embedding:** `sf sync` iterates all registered directories. For each, it resolves the key to an accessible location, gathers text (purpose + README.md or index files), hashes it, and compares against the stored content hash and the recorded embedding model; if either changed, it sends the text to the configured backend and stores the resulting f32 vector as a blob in SQLite, stamped with the model that produced it. Directories without docs are skipped unless `--force` is used or `warn_no_docs` is set to false in config; if a directory's docs have vanished since the last sync, its `has_docs` flag is cleared so search marks its score as unreliable.

**Searching:** `sf search "query"` embeds the query via the configured backend, computes cosine similarity against stored embeddings, filters by `min_similarity`, caps at `max_search_results`, and presents the picker. Embeddings whose recorded model differs from the configured one are skipped with a notice (never compared), as are rows with missing or corrupt embeddings. Metadata filters (author, tags, since) narrow the candidate set before scoring. Selecting a result records a timestamped visit in the `visits` table.

**Co-access:** `sf coaccess <key>` reads the visit log, computes NPMI over a sliding window, and surfaces directories frequently visited in the same session as the given key.

**Editing:** `sf edit <key>` resolves the key, opens `.meta.json` in `$EDITOR`, validates the JSON on save, updates the database row, and re-embeds if the content hash changed.

**Auditing:** `sf audit` scans `contents_path` and configured `backup_locations` for hex directories. Reports lost keys (no known locations), underprotected keys (fewer than 2 locations), and strays (on disk but not registered). Auto-registers newly discovered locations for known keys.

**Key resolution:** Commands that access directory contents (search, sync, edit, coaccess) resolve keys via `db.resolve_key()`, which checks each registered location until it finds one that's mounted. This supports directories that live on different drives.

### SQLite schema

Three tables:

- `directories` — key, created, purpose, author, tags (JSON), embedding (blob), embedding_model, content_hash, has_docs
- `locations` — (key, mount_path) pairs tracking where each directory exists; mount paths are stored tilde-expanded and canonicalized
- `visits` — append-only log of keys with UTC RFC 3339 timestamps (`visited_at`; rows predating the column are NULL)

The schema is versioned via `PRAGMA user_version`, with migrations applied on startup; a database stamped with a newer version than the binary understands is refused rather than touched. Foreign keys are enforced per connection. The embedding is stored as little-endian f32 bytes. At the scale of hundreds to low thousands of directories, brute-force cosine similarity over all embeddings is fast enough that no approximate nearest neighbor index is needed.

### Embedding strategy

The input text for each directory is `purpose` concatenated with a content document. By default the content document is `README.md`; if the `index` field is set in `.meta.json`, those files are used instead (as an override, not additive). The concatenated text is truncated to `max_embed_chars` before being sent to ollama.

The model used for embeddings is configurable. `qwen3-embedding` is the current default; it significantly outperforms `nomic-embed-text` on semantic depth, especially for domain-specific or proper-noun-heavy queries.

Content hashing (SHA-256) is used for change detection: `sf sync` skips directories whose content hasn't changed since the last embedding.

### Configuration

All settings have sensible defaults and are optionally overridden via a TOML file at the platform config directory (`~/.config/sf/config.toml` on Linux, `~/Library/Application Support/sf/config.toml` on macOS). The database lives at the platform data directory.

## Design decisions

**Flat hierarchy with random keys.** The core premise. Meaning lives in metadata and documents, not in paths. This eliminates the organizational rot that comes with hierarchical filesystems and makes every directory address permanent.

**SQLite over flat files.** The database stores metadata, embeddings, locations, and visits in a single file. This is simpler than managing multiple flat files and makes filtered queries trivial. The `rusqlite` crate with the `bundled` feature compiles SQLite into the binary, so there are no runtime dependencies.

**Embedding backends behind a trait.** Commands take a `&dyn EmbeddingClient`, constructed once in `main` from config. The ollama backend is the default; the openai backend exists for machines where running ollama isn't approved, and authenticates via OAuth2 client credentials (corporate-gateway style) with the id and secret taken from environment variables and tokens cached in memory. Requests are bounded by explicit connect and overall timeouts so a wedged server fails fast instead of hanging.

**Per-embedding model tracking.** Every embedding records the model that produced it. Staleness is per row — content hash or model changed — so a model switch re-embeds exactly what needs it on the next `sf sync`, and search refuses to compare vectors across models. No global marker or bulk clear is needed. Embeddings that predate the tracking column (NULL model) are grandfathered as current until next re-embedded.

**Ollama over built-in models.** Embedding models are large and change rapidly. Delegating to ollama keeps the binary small and lets users swap models without recompiling. The tradeoff is a runtime dependency on ollama, but anyone running local AI tools likely already has it.

**Stdout/stderr separation.** The binary writes the selected path to stdout and all UI (picker, warnings, progress) to stderr. This lets shell wrappers capture the path cleanly while the user sees the interactive menu.

**No TUI framework.** The picker uses raw terminal mode directly via libc, same as cdm. This avoids a dependency on ratatui or similar and keeps the interaction model simple: print a numbered list, read a digit, done.

**Visit tracking scoped to sf.** Unlike cdm which hooks into every `cd`, sf only records visits when a directory is selected through its own picker. This keeps the co-access graph focused on intentional navigation rather than incidental directory changes. However, integration with cdm's history is planned as an option (see future directions).

**Multi-location with no primary.** A directory can exist on multiple drives. All copies are peers — there's no primary designation. `resolve_key` checks each registered location and uses the first accessible one. `sf audit` verifies presence and can detect drift once content hashing is implemented.

## Building

```
cargo build --release
```

### Running tests

```
cargo test
```

Unit tests live alongside the modules they cover: NPMI co-access semantics, cosine similarity and the elbow cutoff, text truncation and the embedding byte codec, metadata roundtrips (including preservation of unknown fields), and database behavior (migrations, filters, foreign keys, model-currency rules) against in-memory SQLite.

### Making a release

Tag and push:

```
git tag v0.1.0
git push origin v0.1.0
```

The GitHub Actions release workflow builds for Linux x86_64, macOS x86_64, and macOS aarch64, then publishes tarballs to a GitHub release.

## Future directions

The broader roadmap, including v0.2.0 and v0.3.0 scope, lives in [PLAN.md](PLAN.md). Feature-level notes below.

**Audit content hashing.** Currently audit only checks presence. Planned: three-tier approach.
- `sf audit` — presence only, updates `last_seen` per location. Always fast.
- `sf audit --quick` — quick hash per location (file count + total size + newest mtime). Detects definite drift.
- `sf audit --deep` — full content hash (Merkle-style). Expensive, on demand.
Requires expanding the `locations` table with `last_seen`, `quick_hash`, `quick_hash_date`, `deep_hash`, `deep_hash_date` columns.

**Topic modeling.** Clustering directories by embedding similarity to discover emergent groupings. Two complementary signals: semantic clustering (HDBSCAN over embedding vectors) and behavioral clustering (community detection over the NPMI co-access graph). Could surface as `sf topics`, or as search boosting within clusters, or as tag suggestions.

**Multi-resolution embeddings for topic modeling.** The primary embedding (purpose + README, truncated) is optimized for search. Additional documents listed in the `index` field could feed separate embeddings used only for clustering — a directory becomes a cloud of points in embedding space. Two directories whose primary embeddings are distant might share document-level embeddings that link them in topic space.

**Co-access via cdm history.** Read `~/.cd_history` from cdm, filter to paths matching managed directories, normalize paths across drives to the same key, and feed into NPMI computation. Config option: `coaccess_source = "cdm" | "sf" | "both"`. Advantages: ~2 years of existing history; path normalization across mount points (sf knows that `~/contents/abc123` and `/Volumes/drive/contents/abc123` are the same key).

**Variable co-access windows.** The current NPMI uses a fixed small window (default 3) for workflow neighbors. A larger window (50-100) would capture temporal co-occurrence — directories developed during the same period. Could be exposed as `sf coaccess <key> --wide`.

**Hybrid search.** Combining semantic search with literal substring matching on the purpose field. Catches cases where the embedding model doesn't recognize domain-specific terms but the word appears verbatim in metadata.

**Maturity/health scoring.** Automatically distinguish active projects from abandoned ones. Signals: doc completeness, file modification recency, git commit activity, co-access recency. Could surface in `sf info` or as a search filter (`sf search --active`).

**Query caching.** Cache recent query embeddings in SQLite to avoid hitting ollama on repeated searches. Low priority — cold start is ~2 seconds (model reload), subsequent queries are fast.
