# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager. It stores source assets, metadata, hashes, licensing information, previews, immutable revision history, multi-file packages, dependency relationships, project usage, and logs on hardware you control.

## Current milestone

**Phase 7 — Asset Packaging, Dependencies, and Multi-File Assets**

### Server

- LAN HTTP API
- SQLite catalog
- content-addressed filesystem storage
- streaming uploads/downloads
- SHA-256 duplicate-aware storage
- categories, tags, licensing/source metadata
- recycle bin
- image and 3D previews
- immutable asset revision history
- project/version pinning
- ZIP package import
- package file manifests
- preserved relative directory structure
- glTF external buffer/image dependency discovery
- OBJ → MTL → texture dependency discovery
- missing-dependency diagnostics
- package-aware glTF previews
- package-aware revision restores
- package file download API
- persistent action logs

### Desktop client

- native Windows-first UI
- single-file uploads
- **Add Package ZIP**
- optional primary-file selection
- **Package / Dependencies** viewer
- missing dependency warnings
- image and 3D previews
- Version History
- **Upload Package Version ZIP**
- package-aware Add to Project
- full dependency-tree export while preserving paths
- exact project revision pinning
- persistent daily logs

See `docs/PHASE_1.md` through `docs/PHASE_7.md`.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

Then:

```powershell
cargo run --release --bin dragonforge-client
```

## Package example

```text
DragonForge package
├── model.gltf        ← primary
├── model.bin
└── textures/
    ├── basecolor.png
    └── normal.png
```

DragonForge stores each file by its own SHA-256 while keeping the logical package and directory layout together.

## Preview formats

```text
jpg jpeg png webp obj glb gltf
```

Phase 7 supports external glTF buffers when they are included in the imported package.

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
