# Phase 1 — Asset Server

Phase 1 establishes the first usable DragonForge Asset Vault server for a trusted LAN. It stores binary assets on the local filesystem and stores searchable metadata in SQLite.

## Requirements

- 64-bit Windows or Linux server
- Rust stable toolchain
- LAN access to the chosen TCP port (default `8080`)

## Build

```powershell
git clone https://github.com/djames1987/DragonForge-Asset-Vault.git
cd DragonForge-Asset-Vault
cargo build --release
```

The release binary will be under `target/release/`.

## Configure

Copy the example configuration:

```powershell
Copy-Item DragonForge.toml.example DragonForge.toml
```

Default configuration:

```toml
[server]
bind = "0.0.0.0"
port = 8080
max_upload_bytes = 8589934592

[storage]
data_dir = "./data"

[database]
filename = "dragonforge.db"
```

`data_dir` can point at a dedicated local storage disk, for example:

```toml
[storage]
data_dir = "D:/DragonForgeVault"
```

You can also set `DRAGONFORGE_CONFIG` to load a configuration file from another location.

## Run

```powershell
$env:RUST_LOG="dragonforge_asset_vault=info,tower_http=info"
cargo run --release
```

From another LAN computer, replace `SERVER-IP` below with the vault server's LAN address.

## Health check

```powershell
Invoke-RestMethod http://SERVER-IP:8080/api/health
```

Expected fields include:

```json
{
  "ok": true,
  "service": "dragonforge-asset-vault",
  "phase": 1,
  "version": "0.1.0"
}
```

## Upload an asset

With curl on Windows:

```powershell
curl.exe -X POST "http://SERVER-IP:8080/api/assets" `
  -F "file=@C:\Assets\iron_sword.glb" `
  -F "name=Iron Sword" `
  -F "category=weapon" `
  -F "description=Low-poly medieval iron sword" `
  -F "creator=Example Creator" `
  -F "license=CC0" `
  -F "attribution_required=false" `
  -F "tags=sword,weapon,medieval,low-poly"
```

The server streams the upload to a temporary file while calculating SHA-256. Identical binary content is recognized as a duplicate even when uploaded with a different filename.

## List/search assets

```powershell
Invoke-RestMethod "http://SERVER-IP:8080/api/assets"
Invoke-RestMethod "http://SERVER-IP:8080/api/assets?q=sword"
Invoke-RestMethod "http://SERVER-IP:8080/api/assets?category=weapon"
Invoke-RestMethod "http://SERVER-IP:8080/api/assets?tag=medieval"
Invoke-RestMethod "http://SERVER-IP:8080/api/assets?q=sword&category=weapon&limit=25&offset=0"
```

The default result limit is 100 and the maximum per request is 500.

## Fetch metadata

```powershell
Invoke-RestMethod "http://SERVER-IP:8080/api/assets/ASSET-ID"
```

## Download asset

```powershell
curl.exe -L "http://SERVER-IP:8080/api/assets/ASSET-ID/download" -o recovered_asset.glb
```

Downloads stream from disk rather than loading the whole file into server memory.

## Update metadata

```powershell
$body = @{
  name = "Iron Longsword"
  category = "weapon"
  description = "Updated catalog description"
  license = "CC0"
  attribution_required = $false
  tags = @("sword", "weapon", "medieval", "low-poly")
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Patch `
  -Uri "http://SERVER-IP:8080/api/assets/ASSET-ID" `
  -ContentType "application/json" `
  -Body $body
```

## Delete asset

Phase 1 uses soft deletion so the catalog record and binary are not immediately destroyed.

```powershell
Invoke-RestMethod -Method Delete "http://SERVER-IP:8080/api/assets/ASSET-ID"
```

A future phase will add restore, retention, and permanent purge policies.

## Statistics

```powershell
Invoke-RestMethod "http://SERVER-IP:8080/api/stats"
```

Returns active asset count, soft-deleted asset count, active logical byte count, and unique tag count.

## Storage layout

A typical data directory becomes:

```text
data/
├── dragonforge.db
├── dragonforge.db-shm
├── dragonforge.db-wal
├── assets/
│   └── ab/
│       └── cd/
│           └── abcdef...1234.glb
└── temp/
```

Asset binaries use content-addressed paths derived from SHA-256. The SQLite catalog stores the original filename and relative storage path.

## Phase 1 API

| Method | Route | Purpose |
|---|---|---|
| GET | `/api/health` | Server health/version |
| GET | `/api/stats` | Vault statistics |
| GET | `/api/assets` | List/search assets |
| POST | `/api/assets` | Upload one asset |
| GET | `/api/assets/:id` | Fetch asset metadata |
| PATCH | `/api/assets/:id` | Edit metadata/tags |
| DELETE | `/api/assets/:id` | Soft-delete asset |
| GET | `/api/assets/:id/download` | Stream asset binary |

## Security scope

Phase 1 is intended for a trusted private LAN. It does **not** yet implement user authentication, TLS termination, per-user authorization, restore/purge controls, or check-in/check-out locking. Do not expose this Phase 1 server directly to the public Internet.

## Phase 1 acceptance checklist

1. Server starts and creates its data directory/database.
2. `/api/health` returns `ok: true`.
3. A file can be uploaded from another LAN computer.
4. Re-uploading identical bytes returns the existing asset with `duplicate: true`.
5. Assets can be searched by text, category, and tag.
6. Metadata can be edited.
7. The original file can be downloaded and its SHA-256 matches the catalog hash.
8. Soft-deleted assets disappear from normal search results.
9. `/api/stats` updates as assets are added/deleted.
