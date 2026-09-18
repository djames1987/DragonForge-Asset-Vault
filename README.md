# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager for source assets, metadata, hashes, previews, revision history, multi-file packages, dependencies, projects, and license/attribution tracking.

## Current milestone

**Phase 9 — Local AI Semantic Search**

### Server

- LAN HTTP API
- SQLite catalog
- content-addressed filesystem storage
- streaming uploads/downloads
- SHA-256 duplicate-aware storage
- categories and tags
- recycle bin
- image and 3D previews
- immutable asset revision history
- ZIP package import
- package/dependency manifests
- project/version pinning
- canonical license presets
- asset license assessment
- project-wide license reports
- generated attribution text and CSV/JSON manifest data
- persistent action logs
- local Ollama semantic embedding index
- hybrid semantic + keyword search with offline fallback

### Desktop client

- native Windows-first UI
- single-file and package imports
- package/dependency viewer
- Version History
- project export
- safe **Remove from Project** for single assets and packages
- license preset selector
- license health badges
- license-status filter
- attribution warnings
- automatic `CREDITS.txt`
- automatic JSON and CSV license manifests
- confirmation + rollback-aware project asset removal
- manual **Refresh Credits / License Manifest**
- Smart / Keyword search modes
- AI index status and one-click reindex
- persistent daily logs

See `docs/PHASE_1.md` through `docs/PHASE_9.md`.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

Then:

```powershell
cargo run --release --bin dragonforge-client
```

## Phase 8 project output

When licensed assets are registered to a project, DragonForge maintains:

```text
<Project>/
├── CREDITS.txt
├── DragonForge-License-Manifest.json
├── DragonForge-License-Manifest.csv
└── DragonForgeAssets/
```

Current built-in license presets:

```text
CC0-1.0
CC-BY-4.0
Custom
Unknown
```

Custom/site-specific licenses remain marked for manual review rather than being interpreted automatically.

## Logs

Server:

```text
<storage.data_dir>/logs/
```

Windows client:

```text
%APPDATA%\DragonForge\AssetVault\logs\
```

Vault binaries, package files, historical revisions, previews, database data, and logs are not stored in GitHub.


## Phase 9 AI search setup

DragonForge defaults to local Ollama at `http://127.0.0.1:11434` with the `nomic-embed-text` embedding model.

```powershell
ollama pull nomic-embed-text
```

Then start DragonForge and click **Reindex AI Search** in the desktop client.

Smart Search automatically falls back to keyword ranking if Ollama is unavailable. See `docs/PHASE_9.md` and `DragonForge.example.toml` for configuration.
