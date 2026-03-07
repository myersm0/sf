# sf

A CLI tool for managing a flat hierarchy of directories, each identified by a 6-digit hexadecimal key. Directories are related to each other through metadata, semantic search, and co-access patterns rather than filesystem hierarchy.

Inspired by my days as a cyclist in San Francisco.

## Why flat directories?

Instead of encoding meaning into paths, meaning lives in metadata (`.meta.json`) and in the documentation within each directory. Search — both semantic and metadata-filtered — replaces browsing. Co-access patterns uncover implicit relationships: directories you tend to visit together become associated automatically.

This sounds counterintuitive, but the advantages are considerable:

- You never waste time choosing where to put something.
- You never lose track of something because you filed it under the wrong parent.
- You can freely associate a directory with multiple topics, projects, or contexts through tags and co-access, rather than being forced into a single hierarchy.
- Because folder names ("keys") are stable, every reference to a directory — in scripts, notes, other projects — remains valid forever (except in the case of user error, data loss, etc)
- Writing good documentation pays off directly: the richer your READMEs and metadata, the better your search results

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
  "tags": ["CLI tools", "Rust"]
  "index": ["DEVELOPMENT.md"]
}
```

A local SQLite database stores metadata, embedding vectors, backup locations, and a visit log. Semantic search is powered by [ollama](https://ollama.com) running locally.

## Commands

```
sf new                                  # interactive: prompts for purpose, tags
sf new --purpose="..." --tags="x,y"     # non-interactive
sf info a3f1c2                          # print metadata and backup locations
sf search "linear algebra"              # semantic search with interactive menu
sf search --tags="hcp,ceph"             # filter by tags
sf search --author="myersm0" --since="2025-01-01"
sf sync                                 # (re)embed directories via ollama
sf sync --force                         # include directories without docs
sf coaccess a3f1c2                      # show co-access neighbors (NPMI)
sf audit                                # check backup invariants (planned)
sf edit a3f1c2                          # edit .meta.json in $EDITOR (planned)
sf import a3f1c2                        # register an existing directory
```

## Search

Two complementary signals drive search results:

**Semantic search** uses cosine similarity over ollama embeddings. Each directory is embedded from its `purpose` field and a content document — by default `README.md`, or the files listed in the `index` field of `.meta.json` if specified (as an override, not additive). Results below a configurable similarity threshold are dropped. Scores are shown beside each result. Directories without docs are marked with `(!)` since their scores tend to be unreliable.

**Metadata filters** narrow candidates by author, tags, or creation date. These can be combined with a semantic query or used alone.

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

Each directory can exist in multiple locations (e.g. `~/contents`, `/media/backup1`, `/media/backup2`). The registry remembers locations even when drives aren't mounted. `sf audit` reports directories with fewer than two backup copies, strays on disk not in the registry, and keys in the registry missing from all locations.

## Importing existing directories

To register a directory that already exists under `~/contents/`:

```
sf import a3f1c2
```

This reads the directory's `.meta.json` and adds it to the registry. After importing, run `sf sync` to generate embeddings for search. You can import directories one at a time to verify each `.meta.json` conforms to the schema.

## Configuration

Optional. Create a `config.toml` at the platform-appropriate config path:

- macOS: `~/Library/Application Support/sf/config.toml`
- Linux: `~/.config/sf/config.toml`

```toml
contents_path = "~/contents"
default_author = "myersm0"
embedding_model = "qwen3-embedding"
max_embed_chars = 6000
min_similarity = 0.5
coaccess_window = 3
meta_filenames = [".meta.json", ".meta"]

backup_locations = [
  "/media/backup1",
  "/media/backup2",
]
```

`meta_filenames` controls which filenames are recognized as metadata files, tried in order. The default is `[".meta.json", ".meta"]`.

`max_embed_chars` controls how much text is sent to ollama for embedding. Content beyond this limit is truncated. The right value depends on your embedding model's context window. The default of 6000 works well with `qwen3-embedding`; if using `nomic-embed-text`, try 2000.

`min_similarity` sets a floor for semantic search results. Anything below this cosine similarity score is dropped before results are shown. The default of 0.5 is a reasonable starting point; tune it based on your experience with the embedding model.

The SQLite database lives at the platform data directory:

- macOS: `~/Library/Application Support/sf/sf.db`
- Linux: `~/.local/share/sf/sf.db`

## Dependencies

- [ollama](https://ollama.com) running locally (for embeddings)
- An embedding model pulled in ollama, e.g. `ollama pull qwen3-embedding`

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

These features are planned but not yet implemented:

- `sf audit` — scan backup locations, report directories with fewer than two copies, find strays and missing keys
- `sf edit` — open `.meta.json` in `$EDITOR`, re-embed if content changed

## License
MIT

