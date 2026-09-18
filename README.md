# DragonForge Asset Vault

DragonForge Asset Vault is a LAN-first game-development asset manager for source assets, metadata, hashes, previews, revision history, multi-file packages, dependencies, projects, and license/attribution tracking.

## Current milestone

**Phase 16.1 — DragonForge Visual Redesign**

### Server

- LAN HTTP API
- SQLite catalog
- content-addressed filesystem storage
- streaming uploads/downloads
- SHA-256 duplicate-aware storage
- categories and tags
- recycle bin
- image and 3D previews
- immutable asset revision history
- ZIP package import
- package/dependency manifests
- project/version pinning
- canonical license presets
- asset license assessment
- project-wide license reports
- generated attribution text and CSV/JSON manifest data
- persistent action logs
- local Ollama semantic embedding index
- hybrid semantic + keyword search with offline fallback
- verified vault snapshots with SHA-256 manifests
- configurable retention and replication targets
- engine-aware project export presets for Godot, Unity, Unreal, Roblox/Rojo, Minecraft Bedrock, and Generic projects
- project drift detection, pinned-file repair, and explicit update-to-latest workflows
- trusted-LAN asset checkout/check-in ownership
- server-enforced mutation locks for checked-out assets
- verified hot/archive storage tier transitions
- transparent reads, previews, downloads, versions, and project exports from archived assets
- archive-aware verified backups
- optional API-token authentication
- Administrator / Developer / Read-only role enforcement
- server-side user management with token hashing
- authenticated checkout ownership
- append-only authenticated audit trail
- searchable activity history with user/action/target/result/date filters
- Administrator JSON/CSV audit export
- role-aware desktop controls that reflect server permissions

### Desktop client

- native Windows-first UI
- single-file and package imports
- package/dependency viewer
- Version History
- project export
- safe **Remove from Project** for single assets and packages
- license preset selector
- license health badges
- license-status filter
- attribution warnings
- automatic `CREDITS.txt`
- automatic JSON and CSV license manifests
- confirmation + rollback-aware project asset removal
- manual **Refresh Credits / License Manifest**
- Smart / Keyword search modes
- AI index status and one-click reindex
- one-click Create Backup / Verify Latest controls
- engine-specific project destinations
- Check Project Sync / Repair Pinned Files / Update Project to Latest controls
- asset Check Out / Check In controls with holder/workstation status
- per-asset HOT/ARCHIVE status with Archive Asset / Recall to Hot Storage controls
- persistent daily logs
- masked API token configuration
- authenticated user/role display
- Administrator user-management window
- Activity window and per-asset activity shortcut
- Administrator JSON + CSV activity export
- role-aware disabled mutation controls for Read-only users
- modern sidebar navigation and dedicated application views
- password-manager-inspired three-column asset workflow
- fixed-width searchable asset list instead of the Phase 16 card grid
- large asset detail/preview workspace
- orange-accent DragonForge visual system
- dedicated Projects, Activity, Backups, AI Search, Users, and Settings pages
- persistent theme, UI scale, card size, and layout preferences

See `docs/PHASE_1.md` through `docs/PHASE_16.md`.

## Quick start

```powershell
cargo run --release --bin dragonforge-server
```

Then:

```powershell
cargo run --release --bin dragonforge-client
```

## Phase 8 project output

When licensed assets are registered to a project, DragonForge maintains:

```text
<Project>/
├── CREDITS.txt
├── DragonForge-License-Manifest.json
├── DragonForge-License-Manifest.csv
└── DragonForgeAssets/
```

Current built-in license presets:

```text
CC0-1.0
CC-BY-4.0
Custom
Unknown
```

Custom/site-specific licenses remain marked for manual review rather than being interpreted automatically.

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


## Phase 9 AI search setup

DragonForge defaults to local Ollama at `http://127.0.0.1:11434` with the `nomic-embed-text` embedding model.

```powershell
ollama pull nomic-embed-text
```

Then start DragonForge and click **Reindex AI Search** in the desktop client.

Smart Search automatically falls back to keyword ranking if Ollama is unavailable. See `docs/PHASE_9.md` and `DragonForge.example.toml` for configuration.


## Phase 10 backup setup

By default, DragonForge writes verified backups to `./backups` and keeps the 10 newest snapshots.

A backup contains:

```text
dragonforge-<timestamp>/
├── manifest.json
├── database/
│   └── dragonforge.db
├── assets/
└── previews/
```

Every manifest entry contains a SHA-256 hash and byte size. Backups are verified after creation before they are considered complete.

Optional replication targets can be configured in `DragonForge.toml`:

```toml
[backup]
directory = "./backups"
keep = 10
replication_targets = ["D:/DragonForge-Backup"]
```

The backup directory must be outside the live DragonForge data directory.


## Phase 11 engine-aware project workflow

Project engine presets now determine the default DragonForge export folder:

| Engine | Export destination |
| --- | --- |
| Generic | `DragonForgeAssets` |
| Godot | `assets/dragonforge` |
| Unity | `Assets/DragonForge` |
| Unreal Engine | `Content/DragonForge` |
| Roblox / Rojo | `src/DragonForgeAssets` |
| Minecraft Bedrock | `DragonForgeAssets` |

Existing pre-Phase-11 project links under `DragonForgeAssets` remain supported.

Use **Check Project Sync** to verify exported files against the vault using SHA-256. **Repair Pinned Files** restores the exact versions already pinned to the project without upgrading them. **Update Project to Latest** is the explicit action that advances project pins to the current vault revisions.


## Phase 11.1 maintenance

Engine-aware **Remove from Project** now immediately regenerates `CREDITS.txt`, `DragonForge-License-Manifest.json`, and `DragonForge-License-Manifest.csv` after a successful project unlink. A manifest-write failure is reported without undoing the already-successful removal.


## Phase 12 collaboration locks

DragonForge desktop clients identify themselves using:

```text
<WindowsUser>@<ComputerName>
```

with `DRAGONFORGE_USER` and `DRAGONFORGE_WORKSTATION` environment-variable overrides when needed.

A checked-out asset is protected from mutating requests originating from another client identity. Protected operations include metadata edits, new single-file/package revisions, version restores, soft delete, and restore.

Unlocked assets remain editable for backward compatibility. Phase 12 is still a trusted-LAN collaboration system, not an authentication boundary.


## Phase 12.1 UI maintenance

The Asset Details side panel is now vertically scrollable. This keeps checkout/check-in controls, metadata actions, version controls, project actions, and removal controls accessible on smaller windows and lower-resolution displays.


## Phase 14 storage tiers

Phase 14 adds an optional archive tier for large or infrequently edited source assets. Configure it in `DragonForge.toml`:

```toml
[storage]
data_dir = "./data"
archive_dir = "D:/DragonForge-Archive"
```

The archive directory must be outside the live data directory.

Archiving copies every binary referenced by the asset—including historical revisions and package files—to the archive tier, verifies SHA-256 before changing catalog paths, and removes an old hot copy only when no remaining catalog reference still points to it. **Recall to Hot Storage** performs the inverse operation.

All existing read paths resolve archived content transparently, so previews, downloads, version history downloads, package files, and project export continue to work while an asset is archived.

Phase 10 backups now include configured archive-tier binaries under `archive/assets/`.


## Phase 13 authentication and roles

Phase 13 can be enabled in `DragonForge.toml`:

```toml
[auth]
enabled = true
bootstrap_admin_user = "admin"
bootstrap_admin_token = "replace-with-a-long-random-token"
```

On the first authenticated startup, if the user table is empty, DragonForge creates the bootstrap Administrator. The bootstrap token must be at least 16 characters. Only a SHA-256 token hash is stored in SQLite.

Roles:

| Role | Access |
| --- | --- |
| Administrator | Full access, user management, backup mutations |
| Developer | Read access plus normal asset/project/check-out/storage mutations |
| Read-only | GET/HEAD access only |

The public health endpoint remains available so clients can discover whether authentication is enabled. All other API routes require `Authorization: Bearer <token>` when Phase 13 auth is enabled.

The desktop client stores its configured API token in the local client settings file and masks it in the UI. `DRAGONFORGE_API_TOKEN` may be used as an environment-variable override.

Phase 12 checkout ownership is now bound to the authenticated username on the server; the client cannot spoof another checkout holder by changing `X-DragonForge-User`.


## Phase 13.1 checkout identity maintenance

When authentication is enabled, the desktop client now uses the authenticated DragonForge username—not the local Windows username—to determine whether the selected checkout belongs to the current user. This fixes a case where a Developer could successfully check out an asset but the **Check In** button was hidden when the DragonForge username differed from the Windows account name.


## Phase 15 audit trail and activity

Phase 15 records API activity in an append-only SQLite audit table. Events include timestamp, authenticated user, role, workstation, action, HTTP method/path, target type/id, success/failure, status code, and safe detail text.

The desktop **Activity** window supports filtering by user, action, result, target type/id, and RFC3339 date range. Administrators can export the filtered result set to JSON and CSV.

Non-Administrator users can inspect their own activity. Administrators can inspect vault-wide activity.

Read-only users now see mutating desktop controls disabled instead of being invited to click operations that the server will reject with HTTP 403. Server-side authorization remains authoritative.


## Phase 16 modern desktop UI

Phase 16 reorganizes the native desktop client around a modern three-region shell:

```text
Navigation sidebar | Main workspace | Contextual Inspector
```

The sidebar now separates Library, Workspace, and System destinations instead of keeping every control in the top toolbar.

Dedicated views:

```text
Assets
Recycle Bin
Projects
Activity
Backups
AI Search
Users (Administrator)
Settings
```

The Asset Library now uses an adaptive card grid that responds to available width and a redesigned Inspector with collapsible General, Collaboration, Versions & Package, Project, Storage, and Actions sections.

Appearance/layout preferences are persisted in the existing client settings JSON, including dark/light mode, UI scale, asset card width, sidebar width, inspector width, filter visibility, and last active view.


## Phase 16.1 visual redesign

Phase 16.1 replaces the Phase 16 card-grid presentation with a visual layout modeled on the established DragonForge Password Manager design language.

The Assets view now uses:

```text
Brand/navigation sidebar | Searchable asset list | Large asset detail workspace
```

The left navigation uses a DragonForge logo block, a prominent orange **Add Asset** action, grouped navigation, and a bottom connection/account card.

The middle asset browser is intentionally fixed-width and list-oriented so names and metadata no longer collapse into narrow wrapped columns.

The right workspace provides a large preview, primary actions, and clearly separated General, Tags, Collaboration, Storage, Project, and More sections.

The dark theme uses a blue-black base, raised panels, muted blue-gray secondary text, and a restrained orange accent derived from the password-manager reference.
