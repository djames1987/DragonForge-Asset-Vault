# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager. It stores source assets, metadata, hashes, licensing information, previews, project usage, and logs on hardware you control while developer workstations access the vault through a dedicated desktop client.

## Current milestone

**Phase 4 — Asset Management + Projects**

### Server

- LAN HTTP API
- SQLite metadata database
- content-addressed filesystem storage
- streaming uploads/downloads
- SHA-256 duplicate detection
- categories, tags, source/license metadata
- image thumbnails
- search by text, category, tag, and file type
- metadata editing
- recycle-bin soft delete and restore
- project registry
- project/asset usage tracking
- persistent daily action logs

### Desktop client

- native Windows-first UI
- asset grid with image previews
- drag/drop and file-picker uploads
- metadata editing
- category, exact-tag, and file-type filters
- recycle bin and restore
- safe downloads
- project creation/selection
- copy selected vault assets into a project's `DragonForgeAssets` folder
- persistent daily action logs

See `docs/PHASE_1.md` through `docs/PHASE_4.md` for milestone-specific setup and validation.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

In another terminal:

```powershell
cargo run --release --bin dragonforge-client
```

## Logs

Server:

```text
<storage.data_dir>/logs/
```

Windows client:

```text
%APPDATA%\DragonForge\AssetVault\logs\
```

Asset binaries, previews, database data, and logs remain on the LAN vault/server or local workstation and are not stored in GitHub.
