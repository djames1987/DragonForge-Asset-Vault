# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager. It stores source assets, metadata, hashes, licensing information, previews, project usage, and logs on hardware you control while developer workstations access the vault through a dedicated desktop client.

## Current milestone

**Phase 5 — Richer Asset Previews**

### Server

- LAN HTTP API
- SQLite metadata database
- content-addressed filesystem storage
- streaming uploads/downloads
- SHA-256 duplicate detection
- categories, tags, source/license metadata
- search by text, category, tag, and file type
- metadata editing
- recycle-bin soft delete and restore
- project registry and project/asset tracking
- cached image previews
- server-rendered OBJ previews
- server-rendered GLB previews
- server-rendered embedded-buffer glTF previews
- generic preview API with backward-compatible thumbnail route
- persistent daily action logs

### Desktop client

- native Windows-first UI
- asset grid with image and 3D previews
- larger preview in Asset Details
- drag/drop and file-picker uploads
- metadata editing
- category, exact-tag, and file-type filters
- recycle bin and restore
- safe downloads
- project creation/selection
- copy selected vault assets into a project's `DragonForgeAssets` folder
- persistent daily action logs

See `docs/PHASE_1.md` through `docs/PHASE_5.md` for milestone-specific setup and validation.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

In another terminal:

```powershell
cargo run --release --bin dragonforge-client
```

## Preview support

Current preview formats:

```text
jpg jpeg png webp obj glb gltf*
```

`gltf*` currently means a glTF file with embedded base64 binary buffers. External companion `.bin` files are not yet resolved.

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
