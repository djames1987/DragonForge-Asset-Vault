# Phase 2 — LAN Desktop Client

Phase 2 adds the first day-to-day DragonForge desktop application. The client talks to the Phase 1 server over HTTP; asset binaries stay on the vault server.

## Features

- Configurable server URL
- Server health/connection status
- Browse the vault in a card grid
- Search by name/description/creator/license
- Category filtering
- File picker upload
- Drag-and-drop upload
- Metadata entry before upload
- Tags, creator, source URL, license, attribution and description fields
- SHA-256 duplicate detection through the server
- Asset detail panel
- Download selected assets to a local folder
- Collision-safe downloads (existing files are not overwritten)
- Background network operations so transfers do not block the UI
- Persistent client connection settings on Windows

## Build

From the repository root:

```powershell
cargo build --release --bin dragonforge-client
```

The executable will be:

```text
target\release\dragonforge-client.exe
```

The server is built separately:

```powershell
cargo build --release --bin dragonforge-server
```

## Run

Start the server first:

```powershell
cargo run --release --bin dragonforge-server
```

Then start the client:

```powershell
cargo run --release --bin dragonforge-client
```

For testing on the same machine, leave the server URL as:

```text
http://127.0.0.1:8080
```

For another computer on the LAN, change it to the vault server's LAN IP, for example:

```text
http://192.168.1.50:8080
```

Click **Connect**. The top bar should report the server service and version.

## Add the first real asset

1. Click **Add Asset** or drag a file onto the DragonForge window.
2. Enter a useful name.
3. Add a category such as `3D Model`, `Texture`, `Audio`, `Animation`, or `UI`.
4. Add comma-separated tags.
5. Record creator/source/license information whenever known.
6. Mark **Attribution required** when the asset license requires it.
7. Click **Upload to Vault**.
8. The client refreshes the asset library automatically.

To test duplicate detection, upload the exact same file again. DragonForge should report that it is already in the vault instead of creating a second stored binary.

## Download test

Select an asset card, then click **Download Asset** in the details panel. Choose a local folder. If the same filename already exists, the client creates a numbered filename rather than overwriting it.

## Client settings

On Windows the client stores its server URL under:

```text
%APPDATA%\DragonForge\AssetVault\client.json
```

This is workstation-local configuration only. The asset catalog and files remain on the server.

## Current limitations

Phase 2 intentionally does not yet include:

- Thumbnail generation or rendered 3D previews
- Metadata editing from the GUI
- Authentication/users
- Asset version history
- Check-out/check-in locking
- Project integration/copy-to-project
- Automatic server discovery
- Transfer progress percentages

Those are later phases. The card grid currently uses file/type metadata as the preview surface so this phase can concentrate on proving the full LAN add/search/download workflow.
