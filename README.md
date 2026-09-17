# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first asset management server for 3D game development. The vault keeps source assets, metadata, hashes, licensing information, and future preview/version data on hardware you control while allowing developer workstations on the LAN to access the library through a stable API.

## Current milestone

**Phase 1 — Asset Server**

The first working server provides:

- LAN HTTP API
- SQLite metadata database
- Filesystem-backed asset storage
- Streaming uploads
- SHA-256 integrity hashes and duplicate detection
- Asset metadata, categories, tags, source/license fields
- Search and filtering
- File download
- Metadata updates
- Soft deletion
- Health and statistics endpoints
- Configurable storage/database/listen paths

See `docs/PHASE_1.md` for setup, API examples, and validation steps.
