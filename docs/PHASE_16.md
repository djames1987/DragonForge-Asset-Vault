# Phase 16 — Modern Desktop UI/UX Overhaul

Phase 16 is a client-focused redesign intended to make DragonForge easier to navigate as the feature set grows.

The server APIs and validated vault behavior remain unchanged except for reporting the current Phase 16 milestone.

## Goals

- reduce top-toolbar clutter
- make major feature areas discoverable
- improve use of horizontal and vertical space
- make asset browsing scale better on different window sizes
- keep detailed asset controls available without one extremely long flat panel
- preserve all Phase 1–15 workflows
- persist user interface preferences between launches

## Application shell

The desktop client now uses:

```text
┌──────────────┬──────────────────────────────┬─────────────────┐
│ Navigation   │ Main Workspace               │ Inspector       │
│              │                              │ (Assets view)   │
└──────────────┴──────────────────────────────┴─────────────────┘
```

The previous multi-row control-heavy top area is replaced by a compact page header.

## Sidebar navigation

The sidebar is grouped into:

```text
LIBRARY
  Assets
  Recycle Bin

WORKSPACE
  Projects
  Activity

SYSTEM
  Backups
  AI Search
  Users
  Settings
```

The Users entry is shown only to Administrators.

The sidebar footer shows connection state plus the current authenticated user and role.

## Asset Library

The library toolbar includes:

- search
- Smart / Keyword mode
- Add Asset
- Add Package ZIP
- expandable filters
- current result count

Filters are hidden by default to preserve space and can be expanded when needed.

### Adaptive cards

The asset grid determines its column count from the current workspace width and the configured card width.

Cards show:

- preview or type placeholder
- asset name
- extension
- size
- current version
- category
- checkout holder when present
- ARCHIVED status
- license warning status

Card width is configurable in Settings.

## Inspector

The right-side Inspector appears only in the Assets view.

Sections:

```text
General
Collaboration
Versions & Package
Project
Storage
Actions
```

Sections use collapsible headers so users can keep the information they care about visible without scrolling through every feature.

The Inspector still exposes the existing DragonForge operations including:

- metadata editing
- checkout / check-in
- version history
- package/dependency view
- per-asset activity
- project add/remove
- archive/recall
- download
- recycle/restore

Phase 13 role enforcement and Phase 15 role-aware disabling remain intact.

## Projects page

Projects now have a dedicated full-width view showing:

- name
- engine
- local path
- description
- selection state

For the selected project:

- Check Sync
- Repair Pinned
- Update to Latest
- Refresh Credits

Project creation remains available through the existing project form.

## Activity page

Phase 15 activity is now a full workspace page instead of relying on a popup.

It includes filters for:

- user
- action
- result
- target type
- target ID
- from / to timestamp

Administrators retain JSON + CSV export.

**Asset Activity** navigates directly to this page with the selected asset pre-filtered.

## Backups page

Backups now have a dedicated page showing:

- configured backup directory
- retention count
- replication destinations
- backup history
- file counts
- sizes
- verification state

Administrator-only Create Backup and Verify Latest controls remain enforced.

## AI Search page

The AI Search page displays:

- enabled state
- Ollama reachability
- configured model
- indexed asset count
- total asset count
- stale count
- last error

Developers/Administrators can reindex from this page.

## Users page

Administrator user management is now available as a full application page.

It supports:

- create account
- role selection
- enable/disable
- token rotation
- save
- delete

The existing server protections, including preservation of the final enabled Administrator, remain unchanged.

## Settings page

Connection:

- server URL
- API token
- Save & Connect

Appearance:

- dark/light mode
- UI scale
- asset card width
- sidebar width
- inspector width

Client:

- workstation identity
- log directory
- connected server phase/version

## Persistent preferences

The existing client settings JSON now stores:

```text
ui_scale
card_width
sidebar_width
inspector_width
last_view
dark_mode
show_filters
```

Existing settings files remain compatible through Serde defaults.

## Responsive behavior

Default window size increases to provide a more comfortable modern workspace.

The asset grid automatically changes column count based on:

- current main-workspace width
- configured card width

The sidebar and Inspector remain resizable.

## Compatibility

Phase 16 intentionally does not replace the proven server workflows.

Existing:

- uploads
- packages
- versions
- previews
- licensing
- semantic search
- backups
- engine-aware project sync
- checkouts
- authentication/roles
- archive storage
- audit history

continue using the Phase 1–15 API/event plumbing.

## Suggested validation

1. Pull v0.16.0.
2. Start server/client and confirm Phase 16 / v0.16.0.
3. Navigate every sidebar destination.
4. Resize the window and confirm asset-card columns adapt.
5. Change UI scale and card width in Settings, restart the client, and confirm preferences persist.
6. Switch dark/light mode.
7. Select several assets and verify the Inspector remains usable without hidden controls.
8. Test upload, metadata edit, checkout/check-in, version history, package view, archive/recall, and recycle/restore.
9. Test Projects page sync/repair/update.
10. Test Activity filters and per-asset Activity navigation.
11. Test Backups and AI Search pages.
12. Test Administrator Users page.
13. Log in as Read-only and confirm mutation controls remain disabled.
14. Send client/server logs plus screenshots if any layout issue appears.
