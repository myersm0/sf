# clew
A CLI tool for managing a flat hierarchy of directories, each identified by a 6-digit hex key. Directories are related to each other through metadata, semantic search, and co-access patterns rather than filesystem hierarchy.

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
  "created": "2025-03-15",
  "purpose": "HCP neuroimaging deduplication scripts and index outputs",
  "author": "myersm0",
  "tags": ["neuroimaging", "hcp", "deduplication"],
  "index": ["DEVELOPMENT.md"]
}
```

A local SQLite database stores metadata, embedding vectors, backup locations, and a visit log. Semantic search is powered by [ollama](https://ollama.com) running locally.

## Commands

```
clew new                                  # interactive: prompts for purpose, tags
clew new --purpose="..." --tags="x,y"     # non-interactive
clew info a3f1c2                          # print metadata and backup locations
clew search "linear algebra"              # semantic search with interactive menu
clew search --tags="hcp,ceph"             # filter by tags
clew search --author="myersm0" --since="2025-01-01"
clew sync                                 # (re)embed all directories via ollama
clew coaccess a3f1c2                      # show co-access neighbors (NPMI)
clew audit                                # check backup invariants
clew edit a3f1c2                          # edit .meta.json in $EDITOR
```

## Search

Two complementary signals drive search results:

**Semantic search** uses cosine similarity over ollama embeddings. Each directory is embedded from its `purpose` field, any root-level `README.md`, and any additional files listed in the `index` field of `.meta.json`. An elbow method auto-truncates results to a natural relevance boundary. Scores are shown beside each result.

**Metadata filters** narrow candidates by author, tags, or creation date. These can be combined with a semantic query or used alone.

Results are presented as a numbered menu. Type a number to `cd` into the selected directory, or `q` to cancel.

```
 search
 1) 55b3e2: MIT linear algebra lecture notes and julia code [0.834]
 2) 334334: code to accompany Vectors, Matrices, and Least Squares [0.761]
 3) 2234f5: MIT opencourseware 217 graph theory materials [0.623]

 go to (q to cancel):
```

## Co-access

`clew coaccess` uses normalized pointwise mutual information (NPMI) over the visit log to surface directories you tend to visit in the same session. If you frequently switch between `a3f1c2` and `def456`, running `clew coaccess a3f1c2` will suggest `def456`.

Visits are recorded whenever you select a directory through clew's picker.

## Backup tracking

Each directory can exist in multiple locations (e.g. `~/contents`, `/media/backup1`, `/media/backup2`). The registry remembers locations even when drives aren't mounted. `clew audit` reports directories with fewer than two backup copies, strays on disk not in the registry, and keys in the registry missing from all locations.

## Configuration

Optional. Create `~/.config/clew/config.toml`:

```toml
contents_path = "~/contents"
default_author = "myersm0"
embedding_model = "nomic-embed-text"
coaccess_window = 3

backup_locations = [
  "/media/backup1",
  "/media/backup2",
]
```

## Dependencies

- [ollama](https://ollama.com) running locally (for embeddings)
- An embedding model pulled in ollama, e.g. `ollama pull nomic-embed-text`

## Building

```
cargo build --release
```

The binary is at `target/release/clew`.
