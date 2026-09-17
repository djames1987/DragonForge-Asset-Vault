# Phase 3 — Visual Previews and Full Client/Server Logging

Phase 3 adds visual image previews and persistent activity logging to DragonForge.

## Preview support

The server now supports thumbnail generation for:

- PNG
- JPG / JPEG
- WebP

Thumbnails are generated server-side, cached, and shared by every client.

New endpoint:

```text
GET /api/assets/:id/thumbnail
```

If an image asset was uploaded before Phase 3, its preview is generated automatically the first time a Phase 3 client requests it. You do not need to upload the file again.

Server preview cache:

```text
<vault data directory>/
└── previews/
    └── <sha256 prefix>/
        └── <sha256>.png
```

The desktop client shows image thumbnails in the main asset grid and a larger preview in the asset detail panel. Unsupported asset types currently display a type placeholder such as 3D MODEL, AUDIO, ARCHIVE, or ASSET.

## Logging

Both programs now produce persistent daily log files.

### Server logs

Server logs are stored inside the configured vault data directory:

```text
<storage.data_dir>/logs/
```

Typical filename:

```text
server.log.2026-09-17
```

The server records actions including:

- process startup/shutdown
- storage/database initialization
- health checks
- asset-list/search requests
- asset metadata requests
- upload start/completion
- uploaded byte size and SHA-256
- duplicate upload blocking
- asset catalog creation
- thumbnail generation
- thumbnail delivery
- downloads
- metadata updates
- soft deletes
- warnings/errors

### Client logs

On Windows, client logs are stored at:

```text
%APPDATA%\DragonForge\AssetVault\logs\
```

Typical filename:

```text
client.log.2026-09-17
```

The path is also displayed in the client footer.

The client records actions including:

- process startup/shutdown
- saved server configuration
- connection attempts/results
- refresh/search actions
- file picker and drag/drop selection
- upload attempts and results
- duplicate detection result
- asset selection
- thumbnail fetch/decode results
- downloads and destination paths
- failures and warnings

## Updating from Phase 2

Pull the latest source:

```powershell
git pull
```

Build both programs:

```powershell
cargo build --release --bin dragonforge-server
cargo build --release --bin dragonforge-client
```

Run the server:

```powershell
cargo run --release --bin dragonforge-server
```

Run the client in another terminal:

```powershell
cargo run --release --bin dragonforge-client
```

## Phase 3 validation test

Use the image asset you already uploaded during Phase 2.

1. Start the updated server.
2. Start the updated client.
3. The existing image should appear with a thumbnail.
4. Select it; a larger image preview should appear in the right details panel.
5. Search for the asset.
6. Download the asset.
7. Attempt to upload the same original image again.
8. Confirm DragonForge reports that the asset already exists.
9. Close the client and server cleanly.

Then collect:

```text
Server:
<storage.data_dir>\logs\server.log.<date>

Client:
%APPDATA%\DragonForge\AssetVault\logs\client.log.<date>
```

Upload those two log files to ChatGPT for review. They should provide enough information to verify the complete connection, search, preview, download, upload, and duplicate-detection workflow.

## Privacy note

DragonForge logs operational metadata such as file paths, asset IDs, filenames, hashes, configured server URLs, source/license metadata, and local download destinations. Do not publish the logs publicly if those paths or names are private.

The logger does not intentionally record asset file contents.

## Current preview limitations

Phase 3 does not yet render actual preview images for:

- FBX
- OBJ
- GLB / GLTF
- Blender files
- audio waveforms
- animation clips

Those types currently use placeholders. Later phases can add a preview worker that converts/renders 3D assets and generates richer previews without modifying the original stored asset.
