# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first asset management system for 3D game development. The vault keeps source assets, metadata, hashes, licensing information, previews, and future version data on hardware you control while developer workstations access the library through a dedicated desktop client.

## Current milestone

**Phase 3 — Visual Previews + Client/Server Logging**

### Asset server

- LAN HTTP API
- SQLite metadata database
- Filesystem-backed asset storage
- Streaming uploads/downloads
- SHA-256 integrity hashes and duplicate detection
- Asset metadata, categories, tags, source/license fields
- Search and filtering
- Metadata updates and soft deletion
- Health and statistics endpoints
- Configurable storage/database/listen paths
- Server-side cached image thumbnails
- On-demand preview generation for existing images
- Persistent daily action logs

### Desktop client

- Native Windows-first desktop UI
- Configurable LAN server address and connection status
- Searchable/filterable asset card grid
- Drag-and-drop and file-picker uploads
- Asset metadata/license entry
- Background network transfers
- Image thumbnails in the asset grid
- Larger image preview in asset details
- Asset detail view
- Safe local downloads
- Persistent workstation settings
- Persistent daily action logs

See `docs/PHASE_1.md`, `docs/PHASE_2.md`, and `docs/PHASE_3.md` for setup and validation steps.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

In another terminal:

```powershell
cargo run --release --bin dragonforge-client
```

The actual asset binaries and generated previews are stored on the configured LAN vault disk, not in GitHub.

## Logs

Server:

```text
<storage.data_dir>/logs/
```

Windows client:

```text
%APPDATA%\DragonForge\AssetVault\logs\
```

These logs are intended to make testing and troubleshooting easy to verify.
