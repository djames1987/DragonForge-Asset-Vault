# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager for source assets, metadata, hashes, previews, revision history, multi-file packages, dependencies, projects, and license/attribution tracking.

## Current milestone

**Phase 8.1 — Project Asset Removal**

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
- persistent daily logs

See `docs/PHASE_1.md` through `docs/PHASE_8.md`.

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
