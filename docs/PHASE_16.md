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


# Phase 16.1 — DragonForge Visual Redesign

Runtime review of Phase 16 showed that the adaptive asset grid still behaved like a dense developer utility and produced severe text wrapping when the library, navigation, and Inspector competed for width.

Phase 16.1 changes the visual hierarchy rather than merely tuning card dimensions.

## Reference direction

The redesign follows the visual language of the DragonForge Password Manager project:

- dark blue-black surfaces rather than flat gray
- strong orange brand accent
- large intentional spacing
- clear left navigation hierarchy
- fixed-width browsing column
- large detail workspace
- restrained secondary metadata
- clear empty state
- fewer simultaneous controls

## Assets layout

```text
Navigation | Asset Browser | Asset Details
```

The asset browser is now a vertical list. Each item contains:

- thumbnail/type block
- asset name
- extension
- size
- version
- category
- checkout owner when applicable
- archived indicator

Selecting a list item updates the large detail workspace.

## Empty state

With no selection, the details workspace displays a centered **Select an asset** prompt rather than an empty Inspector panel.

## Asset details

The detail workspace emphasizes:

1. asset name and source filename
2. large preview
3. Download / Check Out-In / Edit / Version History / Activity
4. General metadata
5. Tags
6. Collaboration
7. Storage
8. Project actions
9. Package / recycle actions

## Visual system

Dark palette:

```text
Base        #0B0E14
Panel       #10141D
Raised      #171C27
Secondary   #8D9AB5
Accent      #FF8A2A
```

The accent is used for branding, primary actions, selected elements, focus, and important status rather than as a general background color.

All Phase 1–16 functionality and server APIs remain intact.


# Phase 16.2 — Project Asset Browser

Phase 16.2 makes projects a first-class library browsing scope.

## Project counts

DragonForge exposes active linked-asset counts for every project and displays them in:

- the Projects page
- project quick links in the left navigation

Deleted vault assets do not contribute to the displayed count.

## Project-scoped Assets view

Selecting **Browse Assets** from a project switches the standard Phase 16.1 three-column Assets interface into project scope:

```text
DragonForge Navigation | Project Asset List | Asset Details
```

No separate project-file browser is required.

The selected project is preserved as the current project context so existing project actions continue working.

## Project asset browser API

```text
GET /api/projects/:id/asset-browser
GET /api/projects/asset-counts
```

Each project-browser entry contains:

- full current vault Asset metadata
- pinned project version
- exported relative path
- date added to the project
- whether the project pin is behind the vault's current version

## Search and filtering

The normal asset search field remains available while browsing a project.

Project scope supports filtering by:

- asset name
- original filename
- category
- creator
- tags
- category filter
- tag filter
- extension
- license status

Semantic Smart Search is intentionally hidden while project-scoped because the project asset set is already bounded and local filtering guarantees that no non-project asset appears.

## Asset rows

Project-scoped rows display:

```text
Pinned v2 · Latest v4 · OUTDATED · assets/dragonforge/wood.jpeg
```

or:

```text
Pinned v4 · Latest v4 · CURRENT · assets/dragonforge/cube.obj
```

The normal thumbnail, checkout, storage, preview, download, version-history, and Activity behavior remains available.

## Detail workspace

The Project section in Asset Details shows:

- selected project name
- pinned version
- current vault version
- exported relative path
- update availability

When outdated and the authenticated role can write:

```text
Update This Asset to Latest
```

reuses the existing engine-aware project export flow and updates only the selected project asset.

## Remove from Project

The existing safe project-removal workflow is available directly in the project-scoped browser.

It retains:

- package-aware path handling
- project-root safety validation
- staging/rollback
- server unlink
- license manifest regeneration
- Phase 15 audit recording

After successful removal, the project browser and project asset count refresh automatically.

## Sidebar quick access

Up to six projects appear beneath **Projects** in the Workspace section with their linked active-asset count.

Selecting one opens that project's scoped asset browser immediately.

All projects remain available on the full Projects page.

## Read-only behavior

Read-only users can:

- browse a project's linked assets
- search/filter within a project
- inspect previews and metadata
- see pinned/current versions
- see exported paths
- download
- inspect versions/activity

Read-only users cannot:

- update a project asset to latest
- add/remove project assets
- repair/update project files

Phase 13 server authorization remains authoritative.
