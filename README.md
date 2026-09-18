# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager for source assets, metadata, hashes, previews, revision history, multi-file packages, dependencies, projects, and license/attribution tracking.

## Current milestone

**Phase 12.1 — Scrollable Asset Details Maintenance**

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
- verified vault snapshots with SHA-256 manifests
- configurable retention and replication targets
- engine-aware project export presets for Godot, Unity, Unreal, Roblox/Rojo, Minecraft Bedrock, and Generic projects
- project drift detection, pinned-file repair, and explicit update-to-latest workflows
- trusted-LAN asset checkout/check-in ownership
- server-enforced mutation locks for checked-out assets

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
- one-click Create Backup / Verify Latest controls
- engine-specific project destinations
- Check Project Sync / Repair Pinned Files / Update Project to Latest controls
- asset Check Out / Check In controls with holder/workstation status
- persistent daily logs

See `docs/PHASE_1.md` through `docs/PHASE_12.md`.

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


## Phase 10 backup setup

By default, DragonForge writes verified backups to `./backups` and keeps the 10 newest snapshots.

A backup contains:

```text
dragonforge-<timestamp>/
├── manifest.json
├── database/
│   └── dragonforge.db
├── assets/
└── previews/
```

Every manifest entry contains a SHA-256 hash and byte size. Backups are verified after creation before they are considered complete.

Optional replication targets can be configured in `DragonForge.toml`:

```toml
[backup]
directory = "./backups"
keep = 10
replication_targets = ["D:/DragonForge-Backup"]
```

The backup directory must be outside the live DragonForge data directory.


## Phase 11 engine-aware project workflow

Project engine presets now determine the default DragonForge export folder:

| Engine | Export destination |
| --- | --- |
| Generic | `DragonForgeAssets` |
| Godot | `assets/dragonforge` |
| Unity | `Assets/DragonForge` |
| Unreal Engine | `Content/DragonForge` |
| Roblox / Rojo | `src/DragonForgeAssets` |
| Minecraft Bedrock | `DragonForgeAssets` |

Existing pre-Phase-11 project links under `DragonForgeAssets` remain supported.

Use **Check Project Sync** to verify exported files against the vault using SHA-256. **Repair Pinned Files** restores the exact versions already pinned to the project without upgrading them. **Update Project to Latest** is the explicit action that advances project pins to the current vault revisions.


## Phase 11.1 maintenance

Engine-aware **Remove from Project** now immediately regenerates `CREDITS.txt`, `DragonForge-License-Manifest.json`, and `DragonForge-License-Manifest.csv` after a successful project unlink. A manifest-write failure is reported without undoing the already-successful removal.


## Phase 12 collaboration locks

DragonForge desktop clients identify themselves using:

```text
<WindowsUser>@<ComputerName>
```

with `DRAGONFORGE_USER` and `DRAGONFORGE_WORKSTATION` environment-variable overrides when needed.

A checked-out asset is protected from mutating requests originating from another client identity. Protected operations include metadata edits, new single-file/package revisions, version restores, soft delete, and restore.

Unlocked assets remain editable for backward compatibility. Phase 12 is still a trusted-LAN collaboration system, not an authentication boundary.


## Phase 12.1 UI maintenance

The Asset Details side panel is now vertically scrollable. This keeps checkout/check-in controls, metadata actions, version controls, project actions, and removal controls accessible on smaller windows and lower-resolution displays.
