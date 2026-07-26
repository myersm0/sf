# sf
[![CI](https://github.com/myersm0/sf/actions/workflows/ci.yml/badge.svg)](https://github.com/myersm0/sf/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/myersm0/sf)](https://github.com/myersm0/sf/releases/latest)

A CLI tool for managing a flat hierarchy of directories, each identified by a 6-digit hexadecimal key. Directories are related to each other through metadata, semantic search, and co-access patterns rather than filesystem hierarchy.

Inspired by my days as a cyclist in San Francisco.

## Core principles
Every directory you make with this scheme is named with a stable, meaningless hex _key_ and lives at a single level. There's no hierarchy to organize or reorganize. Meaning lives in metadata, in the documentation you write, and in co-access patterns: how you move around the directories as you work. This creates much richer relationships than you could ever capture in names or in fixed hierarchical relationships, both of which are brittle and limited in expressiveness.

Directories can associate freely with any number of topics, projects, or contexts, rather than being forced into a single parent. Because keys are permanent, every reference to a directory — in scripts, notes, other projects — remains valid forever. Your projects may change unpredictably over time but your directory structures remain stable.

The system rewards good documentation habits: the richer your READMEs and metadata, the better your search results and the semantic relationships that will be discovered. 

## Installation
```bash
curl -fsSL https://raw.githubusercontent.com/myersm0/sf/main/install.sh | sh
```

This detects your platform, downloads the latest precompiled binary, and installs it to ~/.local/bin with shell functions in ~/.local/share/sf/. It then prints the lines to add to your shell profile.

Embeddings are computed by a configurable backend. The default is [ollama](https://ollama.com/) running locally; an OpenAI-compatible HTTP backend is also available for machines where ollama isn't an option (see Embedding backends below). For the default, install ollama and pull an embedding model such as `qwen3-embedding` (or another model of your choice):
```bash
ollama pull qwen3-embedding
```

Shell setup (required):
Add to your .bashrc, .zshrc, etc.:
```bash
export PATH="$HOME/.local/bin:$PATH"  # if not already there
source "$HOME/.local/share/sf/sf.sh"
```

The source line loads wrapper functions so that sf search and sf coaccess actually cd into the selected directory rather than just printing its path.

## How it works

All managed directories live at a single level under a root (default `~/contents/`):

```
~/contents/
├── a3f1c2/
│   ├── .meta.json
│   ├── README.md
│   └── ...
├── 55b3e2/
│   ├── .meta.json
│   └── ...
└── def456/
    ├── .meta.json
    └── ...
```

Each directory has a `.meta.json` file:

```json
{
  "created": "2026-03-06",
  "purpose": "My `sf` project for semantic management of directories",
  "author": "myersm0",
  "tags": ["cli-tools", "rust"],
  "index": ["docs/DEVELOPMENT.md"]
}
```

A directory brought under management by `sf import` also carries `source_name`, recording what it was called before it was renamed to a key. It is provenance, not an address: it is never used to locate anything, and it is not made unique.

Fields beyond this schema are allowed and preserved by every sf operation, so you can keep your own annotations (a `class` field, say) in metadata without sf discarding them.

A local SQLite database stores metadata, embedding vectors, backup locations, and a visit log. Semantic search is powered by [ollama](https://ollama.com) running locally.

`.meta.json` is the authoritative copy of a directory's metadata; the database mirrors it so that filtered queries stay fast. `sf sync` reconciles the two, so metadata edited outside sf takes effect on the next sync rather than being silently ignored.

## Commands

```
sf new                                  # interactive: prompts for purpose, tags
sf new --purpose="..." --tags="x,y"     # non-interactive
sf info a3f1c2                          # print metadata and backup locations
sf search "linear algebra"              # semantic search with interactive menu
sf search --tags="hcp,ceph"             # filter by tags
sf search --author="myersm0" --since="2025-01-01"
sf search --source="sf"                 # directories renamed from a directory called sf
sf sync                                 # (re)embed directories, reconcile metadata
sf sync --force                         # include directories without docs
sf coaccess a3f1c2                      # show co-access neighbors (NPMI)
sf audit                                # check backup invariants across locations
sf edit a3f1c2                          # edit .meta.json in $EDITOR, re-embed if changed
sf import a3f1c2                        # register a directory already named with a key
sf import ./myproject                   # rename to a fresh key, then register
sf clone https://github.com/u/repo      # clone into the contents root and register
sf validate                             # check metadata against the schema
sf validate a3f1c2                      # check one directory
```

## Search

Two complementary signals drive search results:

**Semantic search** uses cosine similarity over ollama embeddings. Each directory is embedded from its `purpose` field and a content document — by default `README.md`, or the files listed in the `index` field of `.meta.json` if specified (as an override, not additive). Results below a configurable similarity threshold are dropped. Scores are shown beside each result. Directories without docs are marked with `(!)` since their scores tend to be unreliable.

Every stored embedding records both the backend and the model that produced it, and search refuses to compare across either: if you change `embedding_backend` or `embedding_model` in the config, stale embeddings are skipped with a notice until `sf sync` re-embeds them. Results never silently mix vector spaces.

**Metadata filters** narrow candidates by author, tags, creation date, or source name. These can be combined with a semantic query or used alone.

`--source` answers "which directory holds the `sf` repository?" — it matches the name a directory had before sf renamed it, case-insensitively. Several clones of the same repository all match and are all listed, rather than one arbitrarily winning; if you keep a fork alongside an upstream checkout, that is what you want to see.

Results are presented as a numbered menu. Type a number to `cd` into the selected directory, or `q` to cancel.

```
 search
 1) 553fe2: MIT linear algebra lecture notes and julia code [0.834]
 2) a343b4: code to accompany Vectors, Matrices, and Least Squares [0.761]
 3) 01d42c: Screenshots from my personal Mac (!) [0.578]

 go to (q to cancel):
  (!) = no docs; score may be unreliable
```

## Co-access

`sf coaccess` uses normalized pointwise mutual information (NPMI) over the visit log to surface directories you tend to visit in the same session. If you frequently switch between `a3f1c2` and `def456`, running `sf coaccess a3f1c2` will suggest `def456`.

Visits are recorded whenever you select a directory through sf's picker.

## Backup tracking

Each directory can exist in multiple locations (e.g. `~/contents`, `/media/backup1`, `/media/backup2`). The registry remembers locations even when drives aren't mounted.

`sf audit` scans the contents path and configured backup locations and reports, per key: **lost** (registered but no known locations at all), **unreachable** (registered locations exist but none is reachable right now, distinguishing unmounted roots from copies missing at mounted ones), **missing copies** (present somewhere, but absent from a mounted root where it's registered — a copy vanished from a plugged-in drive), and **underprotected** (fewer than two locations). It also lists strays (directories on disk but not in the registry) and auto-registers newly discovered locations for known keys. An unplugged drive is reported as unknown, not as a failure.

## Importing existing directories

`sf import` brings a directory that already exists under management. It takes a path, or a bare name to be looked up under the root:

```
sf import a3f1c2           # already named with a key
sf import ./myproject      # not yet named with a key
sf import --path /media/backup1 a3f1c2
```

If the directory is already named with a registered key, that is all sf needs to know: the root is added to that key's locations if it isn't there already, and nothing else is touched. This is how a second copy on a backup drive gets registered.

Otherwise sf decides two things independently — what the key should be, and where the metadata comes from.

**The key** is the directory's own name if that name is a well-formed key. If it isn't, sf generates a fresh one and renames the directory, asking first unless `--force` is given. Renaming only happens inside the contents root; a directory anywhere else is registered under its existing name or not at all, so move it into the root first if you want it renamed. The old name is preserved in the `source_name` field rather than discarded.

**The metadata** is honored as-is if a valid file is there. If there is none, sf prompts, defaulting `purpose` to the first paragraph of the directory's README and `author` to the usual fallback. If a metadata file is present but has validation errors, the import stops and reports them — nothing is renamed or registered until they're fixed. Warnings and notes are printed but don't block.

Prompting needs a terminal. With stdin redirected, a directory with no metadata is an error rather than a hang, so `sf import` stays safe to run in a loop.

After importing, run `sf sync` to generate embeddings for search.

## Cloning repositories

```
sf clone https://github.com/myersm0/sf
```

`sf clone` is `git clone` followed by an import, with the repository landing directly in the contents root. The repository name is recorded as `source_name`, which is what `sf search --source` later matches on.

If the repository carries its own `.meta.json`, it is honored and the clone is registered without asking anything. Otherwise you are prompted, as with `sf import`.

If the clone succeeds but it can't be registered with `sf` for some reason (e.g. invalid metadata), then the clone is left in the contents root under the repository's own name, and `sf import <name>` finishes the job once you've fixed whatever problem was preventing registration.

## Validating metadata

```
sf validate            # every registered directory
sf validate a3f1c2     # one key
sf validate ./somedir  # one path, registered or not
```

Findings come at three levels. **Errors**: a `created` field that isn't a `YYYY-MM-DD` date, or an empty `purpose` (the primary search signal). **Warnings**: an empty `author`, an `index` entry naming a file that isn't there. **Notes** simply list fields sf doesn't recognize, since those are preserved deliberately and are not a fault.

`sf validate` exits nonzero if any errors were found; warnings and notes do not affect the exit code.

## Embedding backends

Two backends are supported, selected by `embedding_backend` in the config.

**ollama** (the default) talks to a local ollama server at `ollama_url`. Requests are bounded by `ollama_timeout_seconds` so a wedged server can't hang sf indefinitely.

**openai** talks to an OpenAI-compatible embeddings endpoint, with the same connect and overall timeout treatment (its own `timeout_seconds`, default 120, covering both the token fetch and the embedding request). It authenticates via OAuth2 client credentials — the shape corporate API gateways typically require, and the reason this backend exists (for machines where running ollama isn't approved). Configure it like so:

```toml
embedding_backend = "openai"
embedding_model = "text-embedding-3-small"

[openai]
url = "https://your-gateway.example.com/v1/embeddings"
oauth2_token_url = "https://login.example.com/oauth2/v2.0/token"
oauth2_scope = "api://your-app/.default"
timeout_seconds = 120
```

The OAuth2 client id and secret are read from the `SF_OAUTH2_CLIENT_ID` and `SF_OAUTH2_CLIENT_SECRET` environment variables, never from the config file. Tokens are cached in memory and refreshed shortly before expiry.

Because embeddings record their backend and model, you can move a database between machines with different backends: embeddings your current config doesn't match are simply skipped in search until re-embedded. Two backends serving the same model name are treated as distinct vector spaces, since there is no guarantee the vectors are comparable.

## Configuration

Optional. Create a `config.toml` at the platform-appropriate config path:

- macOS: `~/Library/Application Support/sf/config.toml`
- Linux: `~/.config/sf/config.toml`

```toml
contents_path = "~/contents"
default_author = "myersm0"
embedding_backend = "ollama"
ollama_url = "http://localhost:11434"
embedding_model = "qwen3-embedding"
ollama_timeout_seconds = 120
max_embed_chars = 6000
min_similarity = 0.5
max_search_results = 15
warn_no_docs = true
coaccess_window = 3
meta_filenames = [".meta.json", ".meta"]

backup_locations = [
  "/media/backup1",
  "/media/backup2",
]
```

Paths in the config may use `~`, which is expanded to your home directory. A config file that exists but fails to parse is an error — sf will not silently fall back to defaults.

`meta_filenames` controls which filenames are recognized as metadata files, tried in order. The default is `[".meta.json", ".meta"]`.

`default_author` supplies the `author` field for new directories. If it isn't set, sf falls back to `$USER`, and prompts only if that is unset too.

`max_embed_chars` controls how much text is sent to ollama for embedding. Content beyond this limit is truncated. The right value depends on your embedding model's context window. The default of 6000 works well with `qwen3-embedding`; if using `nomic-embed-text`, try 2000.

`ollama_timeout_seconds` bounds how long sf waits for an ollama response before giving up (default 120, generous enough for cold model loads on slow hardware).

`min_similarity` sets a floor for semantic search results. Anything below this cosine similarity score is dropped before results are shown. The default of 0.5 is a reasonable starting point; tune it based on your experience with the embedding model.

The SQLite database lives at the platform data directory:

- macOS: `~/Library/Application Support/sf/sf.db`
- Linux: `~/.local/share/sf/sf.db`

The database schema is versioned; sf migrates older databases automatically on startup and refuses to touch a database created by a newer sf.

## Dependencies

- For the default backend: [ollama](https://ollama.com) running locally, with an embedding model pulled (e.g. `ollama pull qwen3-embedding`)
- Alternatively: access to an OpenAI-compatible embeddings API behind an OAuth2 client-credentials gateway (see Embedding backends)

## Building

```
cargo build --release
```

The binary is at `target/release/sf`. Copy it somewhere in your `$PATH`.

## Shell setup

Add to your `.bashrc`, `.zshrc`, etc.:

```bash
source /path/to/sf/shell/sf.sh
```

This wraps `sf search` and `sf coaccess` so that selecting a directory actually `cd`s into it (the raw binary prints the path to stdout; the shell wrapper captures it).

## Roadmap

- Per-key locations with roles and sync policies, replacing the global `backup_locations` (v0.3.0)

See `docs/PLAN.md` for the full development plan.

## License
MIT

