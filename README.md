# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first asset management system for 3D game development. The vault keeps source assets, metadata, hashes, licensing information, and future preview/version data on hardware you control while developer workstations access the library through a dedicated desktop client.

## Current milestone

**Phase 2 — LAN Desktop Client**

### Phase 1 server

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

### Phase 2 client

- Native Windows-first desktop UI
- Configurable LAN server address and connection status
- Searchable/filterable asset card grid
- Drag-and-drop and file-picker uploads
- Asset metadata/license entry
- Background network transfers
- Asset detail view
- Safe local downloads
- Persistent workstation settings

See `docs/PHASE_1.md` for the server and `docs/PHASE_2.md` for client build, setup, and the first asset test.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

In another terminal:

```powershell
cargo run --release --bin dragonforge-client
```

The actual asset binaries are stored on the configured LAN vault disk, not in GitHub.
