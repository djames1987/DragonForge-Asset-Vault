# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager. It stores source assets, metadata, hashes, licensing information, previews, version history, project usage, and logs on hardware you control.

## Current milestone

**Phase 6 — Asset Versioning + History**

### Server

- LAN HTTP API
- SQLite metadata database
- content-addressed filesystem storage
- streaming uploads/downloads
- SHA-256 duplicate detection
- categories, tags, source/license metadata
- metadata editing
- recycle-bin soft delete and restore
- image and 3D previews
- immutable asset revision history
- version-specific downloads
- restore old revision as new current revision
- project registry
- project/asset version pinning
- persistent daily action logs

### Desktop client

- native Windows-first UI
- image and 3D previews
- searchable/filterable asset library
- metadata editing
- recycle bin and restore
- project creation and add-to-project workflow
- **Version History** window
- upload new asset revision
- revision notes
- restore previous revision
- current version displayed in Asset Details
- persistent daily logs

See `docs/PHASE_1.md` through `docs/PHASE_6.md` for milestone-specific setup and validation.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

In another terminal:

```powershell
cargo run --release --bin dragonforge-client
```

## Current preview formats

```text
jpg jpeg png webp obj glb gltf*
```

`gltf*` currently means a glTF file with embedded base64 binary buffers.

## Logs

Server:

```text
<storage.data_dir>/logs/
```

Windows client:

```text
%APPDATA%\DragonForge\AssetVault\logs\
```

Asset binaries, historical revisions, previews, database data, and logs remain on the LAN vault/server or local workstation and are not stored in GitHub.
