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
│   └── coaccess.rs      NPMI co-access scoring over visit log
├── config/
│   └── settings.rs      TOML config with platform-appropriate defaults
├── db/
│   └── schema.rs        SQLite schema and all database operations
├── embed/
│   ├── ollama.rs        HTTP client for ollama's /api/embed endpoint
│   ├── content.rs       Text gathering and content hashing
│   └── similarity.rs    Cosine similarity (and unused elbow method)
├── meta/
│   └── schema.rs        .meta.json serde struct, load/save
└── picker/
    └── menu.rs          Raw-mode terminal picker (ported from cdm)
```

### Data flow

**Creating a directory:** `sf new` generates a random 6-digit hex key, creates `~/contents/<key>/`, writes `.meta.json`, and inserts a row into the `directories` table with the contents path registered as the first location.

**Importing:** `sf import <key>` reads an existing `.meta.json` and registers it in the database. No embedding happens at import time.

**Embedding:** `sf sync` iterates all registered directories. For each, it gathers text (purpose + README.md or index files), hashes it, compares against the stored hash, and if changed, sends the text to ollama and stores the resulting f32 vector as a blob in SQLite. Directories without docs are skipped unless `--force` is used.

**Searching:** `sf search "query"` embeds the query via ollama, computes cosine similarity against all stored embeddings, filters by `min_similarity`, caps at `max_search_results`, and presents the picker. Metadata filters (author, tags, since) narrow the candidate set before scoring. Selecting a result records a visit in the `visits` table.

**Co-access:** `sf coaccess <key>` reads the visit log, computes NPMI over a sliding window, and surfaces directories frequently visited in the same session as the given key.

### SQLite schema

Three tables:

- `directories` — key, created, purpose, author, tags (JSON), embedding (blob), content_hash, has_docs
- `locations` — (key, mount_path) pairs tracking where each directory exists
- `visits` — append-only log of keys, ordered by insertion

The embedding is stored as little-endian f32 bytes. At the scale of hundreds to low thousands of directories, brute-force cosine similarity over all embeddings is fast enough that no approximate nearest neighbor index is needed.

### Embedding strategy

The input text for each directory is `purpose` concatenated with a content document. By default the content document is `README.md`; if the `index` field is set in `.meta.json`, those files are used instead (as an override, not additive). The concatenated text is truncated to `max_embed_chars` before being sent to ollama.

The model used for embeddings is configurable. `qwen3-embedding` is the current default; it significantly outperforms `nomic-embed-text` on semantic depth, especially for domain-specific or proper-noun-heavy queries.

Content hashing (SHA-256) is used for change detection: `sf sync` skips directories whose content hasn't changed since the last embedding.

### Configuration

All settings have sensible defaults and are optionally overridden via a TOML file at the platform config directory (`~/.config/sf/config.toml` on Linux, `~/Library/Application Support/sf/config.toml` on macOS). The database lives at the platform data directory.

## Design decisions

**Flat hierarchy with random keys.** The core premise. Meaning lives in metadata and documents, not in paths. This eliminates the organizational rot that comes with hierarchical filesystems and makes every directory address permanent.

**SQLite over flat files.** The database stores metadata, embeddings, locations, and visits in a single file. This is simpler than managing multiple flat files and makes filtered queries trivial. The `rusqlite` crate with the `bundled` feature compiles SQLite into the binary, so there are no runtime dependencies.

**Ollama over built-in models.** Embedding models are large and change rapidly. Delegating to ollama keeps the binary small and lets users swap models without recompiling. The tradeoff is a runtime dependency on ollama, but anyone running local AI tools likely already has it.

**Stdout/stderr separation.** The binary writes the selected path to stdout and all UI (picker, warnings, progress) to stderr. This lets shell wrappers capture the path cleanly while the user sees the interactive menu.

**No TUI framework.** The picker uses raw terminal mode directly via libc, same as cdm. This avoids a dependency on ratatui or similar and keeps the interaction model simple: print a numbered list, read a digit, done.

**Visit tracking scoped to sf.** Unlike cdm which hooks into every `cd`, sf only records visits when a directory is selected through its own picker. This keeps the co-access graph focused on intentional navigation rather than incidental directory changes.

## Building

```
cargo build --release
```

### Running tests

```
cargo test
```

### Making a release

Tag and push:

```
git tag v0.1.0
git push origin v0.1.0
```

The GitHub Actions release workflow builds for Linux x86_64, macOS x86_64, and macOS aarch64, then publishes tarballs to a GitHub release.

## Future directions

**Backup audit.** `sf audit` would scan configured mount points, reconcile actual directory presence against the registry, and report: keys with fewer than two backup copies, strays on disk not in the registry, and keys in the registry missing from all known locations. The registry remembers locations even when drives aren't mounted; a content hash per location would detect stale copies.

**Metadata editing.** `sf edit <key>` would open `.meta.json` in `$EDITOR` and trigger re-embedding if the content changed.

**Topic modeling.** Clustering directories by embedding similarity to discover emergent groupings. This could surface implicit categories ("all my music projects", "all my NLP work") without requiring explicit tags. Possible approaches include k-means or HDBSCAN over the embedding vectors, or building a nearest-neighbor graph and running community detection. The challenge is choosing the right granularity — too few clusters and everything blurs together, too many and it's just a list of directories again.

**Hybrid search.** Combining semantic search with literal substring matching on the purpose field. This would catch cases where the embedding model doesn't recognize domain-specific terms (e.g. "Organteq") but the word appears verbatim in the metadata.

**Shell integration for visit tracking.** Currently visits are only recorded when selecting through sf's picker. A deeper integration — hooking into `cd` like cdm does, but filtering to managed directories — would enrich the co-access graph with organic navigation patterns.
