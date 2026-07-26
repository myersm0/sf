# Development

## Architecture

`sf` is a single Rust binary that manages a flat directory hierarchy with semantic search. It follows the same design pattern as [cdm](https://github.com/myersm0/cdm): a compiled binary handles the logic and writes the selected path to stdout, while a thin shell wrapper captures that output and `cd`s into it. The picker UI goes to stderr so it doesn't interfere.

### Module layout

```
src/
├── main.rs              CLI definition (clap) and subcommand dispatch
├── commands/
│   ├── new.rs           Create a new directory with random hex key
│   ├── import.rs        Bring an existing directory under management
│   ├── clone.rs         git clone into the root, then adopt
│   ├── info.rs          Display metadata for a key
│   ├── search.rs        Semantic + metadata search with picker
│   ├── sync.rs          (Re)embed all directories via ollama
│   ├── coaccess.rs      NPMI co-access scoring over visit log
│   ├── edit.rs          Open .meta.json in $EDITOR, re-embed if changed
│   ├── audit.rs         Scan locations, report backup status
│   └── validate.rs      Report metadata findings, exit nonzero on errors
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
├── keys.rs              Key format predicate and key generation
├── meta/
│   ├── schema.rs        .meta.json serde struct, load/save
│   └── validate.rs      Schema checks beyond what serde enforces
├── picker/
│   └── menu.rs          Raw-mode terminal picker (ported from cdm)
└── prompt.rs            Line and yes/no prompts on stderr
```

### Data flow

**Creating a directory:** `sf new` generates a random 6-digit hex key, creates the directory at the specified root (or default `contents_path`), writes `.meta.json`, and inserts a row into the `directories` table with that root registered as the first location.

**Importing:** `sf import <target>` resolves its argument as a path first and then as a name under the root, so a key and a path are both accepted. What follows is a short-circuit and then two independent decisions.

The short-circuit: if the directory's name is a well-formed key that is already registered, the root is added to that key's locations and nothing else happens. Metadata is not even read, since there is nothing to decide.

Otherwise the key comes from the directory's name if that name is well-formed, and from `keys::generate_unique` with a rename if it isn't; and the metadata is either the file that's there or one composed at the prompt. The two do not interact — any combination is coherent — which is why this is one operation with branches rather than a family of commands. `sf adopt`, planned separately, collapsed into this once renaming was restricted to directories already inside a root.

Order within the command is chosen so that nothing is wasted or half-done. Validation errors are reported before anything is asked, since a broken metadata file should not cost the user a confirmation. The rename confirmation comes before the metadata prompts, so declining costs no typing. The rename itself happens after the prompts, because the default `purpose` is read from the directory's README and the path must still be valid. Metadata is written only if it was composed here or if a rename added `source_name`, so an existing file is left byte-for-byte alone unless there is something to record in it.

Renaming is confined to `contents_path`. Registering is not: a copy on a backup root is registered in place under whatever name it has, since renaming a backup copy would be wrong. Prompting requires a terminal, so an absent metadata file under redirected stdin is an error rather than a hang.

No embedding happens at import time; `sf sync` picks it up.

**Embedding:** `sf sync` iterates all registered directories. For each, it resolves the key to an accessible location and reconciles the database row against `.meta.json`, rewriting `created`, `purpose`, `author`, and `tags` whenever they differ. Reconciliation runs before and independently of the embedding decision, since `author` and `tags` are not embedding inputs and so never move the content hash. It then gathers text (purpose + README.md or index files), hashes it, and compares against the stored content hash and the recorded backend and model; if any of them changed, it sends the text to the configured backend and stores the resulting f32 vector as a blob in SQLite, stamped with the model that produced it. Directories without docs are skipped unless `--force` is used or `warn_no_docs` is set to false in config; if a directory's docs have vanished since the last sync, its `has_docs` flag is cleared so search marks its score as unreliable.

**Searching:** `sf search "query"` embeds the query via the configured backend, computes cosine similarity against stored embeddings, filters by `min_similarity`, caps at `max_search_results`, and presents the picker. Embeddings whose recorded backend or model differs from the configured one are skipped with a notice (never compared), as are rows with missing or corrupt embeddings. Metadata filters (author, tags, since, source) narrow the candidate set before scoring. Selecting a result records a timestamped visit in the `visits` table.

**Co-access:** `sf coaccess <key>` reads the visit log, computes NPMI over a sliding window, and surfaces directories frequently visited in the same session as the given key.

**Editing:** `sf edit <key>` resolves the key, opens `.meta.json` in `$EDITOR`, validates the JSON on save, updates the database row, and re-embeds if the content hash changed. Editing through sf and editing the file directly therefore converge on the same result; the second just waits for the next `sf sync`.

**Cloning:** `sf clone <url>` runs `git clone` into `<contents_path>/.sf-clone-<pid>` and then calls `import::adopt` with the repository name supplied explicitly. Cloning into the destination root rather than a temporary directory is deliberate: `/tmp` is frequently a different filesystem, and the rename that gives the directory its key would then be a cross-device move. Staging inside the root makes it a rename, and a hidden staging name means a failed clone leaves a hidden directory rather than a half-populated live key.

The name has to be passed in because the staging directory's name is synthetic — deriving it from the path, as `import` does, would record `.sf-clone-1234` as the provenance. This is the only reason `adopt` is a separate entry point from `import::run`: everything else about the two paths is identical, which is why `clone` needs no special-casing beyond it.

A clone that succeeds but fails to register is moved to `<contents_path>/<repository name>` rather than deleted or left hidden, so that the `sf import` suggested in the error message both finds it and derives the right `source_name` from it. If that name is taken, it stays at the staging path.

**Source names:** `source_name` records what a directory was called before `sf import` renamed it to a key. It is mirrored into the database so `search --source` can filter on it, matched case-insensitively and exactly. Nothing enforces uniqueness, and nothing resolves through it: two clones of one repository are two directories with the same source name, and a search reports both rather than picking one. Because reconciliation covers it like any other mirrored field, directories imported before the column existed need no migration — the next `sf sync` reads the name back out of `.meta.json`.

**Validating:** `sf validate` loads metadata and reports findings at three severities. `meta::validate::check` holds the checks that need only the struct; `check_in_directory` adds those that need the directory beside it. Keeping the split means the former unit-test without touching a filesystem, and it keeps `DirMeta::load` purely a serde operation — parsing and judging stay separate, so `import` can parse first and decide what to do about the findings second.

Severity is assigned by consequence, not by tidiness. Errors are reserved for metadata that fails silently rather than loudly: a non-ISO `created` never raises anything at write time, it just sorts wrong under `search --since` forever, because that column is compared as text. Unrecognized fields are only notes, since preserving them is a documented feature — they are reported at all so that a typo'd field name is visible next to the legitimate custom ones.

**Auditing:** `sf audit` scans `contents_path` and configured `backup_locations` for hex directories. Reports lost keys (no known locations), underprotected keys (fewer than 2 locations), and strays (on disk but not registered). Auto-registers newly discovered locations for known keys.

**Key resolution:** Commands that access directory contents (search, sync, edit, coaccess) resolve keys via `db.resolve_key()`, which checks each registered location until it finds one that's mounted. This supports directories that live on different drives.

### SQLite schema

Three tables:

- `directories` — key, created, purpose, author, tags (JSON), source_name, embedding (blob), embedding_backend, embedding_model, content_hash, has_docs
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

**Key format in one place.** `keys::is_valid` is the sole definition of what a well-formed key looks like, and `keys::generate` the sole producer. Previously the format was asserted by a regex inside `audit` and reproduced by a formatting string inside `new`. Centralizing it drops the `regex` dependency (its only use was a six-character match) and gives the planned structural membership test a single call site to replace.

**The row is shaped like the file.** `insert_directory` and `update_directory` take a `&DirMeta` rather than a list of columns, and `DirectoryRow::mirrors` is the single definition of whether a row still agrees with a file. Adding `source_name` to the schema meant a sixth positional string argument to both writers, four of which would have been interchangeable at the call site with no type error — a swap that compiles is exactly the bug the mirror should not have. With the struct passed whole, adding a mirrored field touches the insert, the update, the select, and `mirrors`, all within one file, and no call site at all.

The cost is that `db` now depends on `meta`. That direction is defensible — the table exists to mirror the file — but it is worth naming, since `db` also depends on `embed` for `EmbeddingIdentity` and the storage layer should not accumulate many more of these.

Note that `search_directories` was left with positional filters, now four of them. They are distinguishable at every call site because the argument names match the parameter names, but a fifth would be the point to introduce a filter struct.

**Metadata on disk is authoritative.** `.meta.json` travels with the directory; the database is a mirror kept for fast filtered queries, and can be rebuilt from disk. So where the two disagree, the file wins, and `sf sync` is where that reconciliation happens. Previously the mirror was written only by `new`, `import`, and `edit`, which meant any out-of-band edit left the database stale indefinitely — `search --author` would filter on a value no longer in any file. Note that this rule is settled only for the fields the database mirrors; when per-key `locations` move into `.meta.json`, which copy is authoritative becomes a genuinely open question, since every replica will carry its own list.

**SQLite over flat files.** The database stores metadata, embeddings, locations, and visits in a single file. This is simpler than managing multiple flat files and makes filtered queries trivial. The `rusqlite` crate with the `bundled` feature compiles SQLite into the binary, so there are no runtime dependencies.

**Embedding backends behind a trait.** Commands take a `&dyn EmbeddingClient`, constructed once in `main` from config. The ollama backend is the default; the openai backend exists for machines where running ollama isn't approved, and authenticates via OAuth2 client credentials (corporate-gateway style) with the id and secret taken from environment variables and tokens cached in memory. Requests are bounded by explicit connect and overall timeouts so a wedged server fails fast instead of hanging.

**Per-embedding backend and model tracking.** Every embedding records the backend and the model that produced it, as two columns rather than one qualified string. A single string was considered and rejected: ollama model names already use a colon for tags (`qwen3-embedding:8b`), so a `backend:model` stamp would need a split-on-first-colon rule, and rewriting existing values would mean guessing which backend produced rows written before the distinction existed. Separate columns make the migration an `ADD COLUMN`, leave every stored value untouched, and let mismatch messages name both halves.

Staleness is per row — content hash, backend, or model changed — so a switch of either re-embeds exactly what needs it on the next `sf sync`, and search refuses to compare vectors across identities. No global marker or bulk clear is needed. `EmbeddingIdentity` is the single place the pair is derived from config and compared against storage, so sync's notion of stale and search's notion of skippable cannot drift apart. Columns absent from older rows are grandfathered independently: a NULL backend matches any backend, a NULL model matches any model.

Two backends serving the same model name are treated as separate vector spaces. Qualifying further by gateway host would distinguish two OpenAI-compatible endpoints serving different vectors under one model name, but that stamp would churn whenever a URL changed, and the second column makes it cheap to revisit if it ever bites.

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
