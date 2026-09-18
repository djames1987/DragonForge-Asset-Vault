# Phase 4 — Asset Management and Projects

Phase 4 turns DragonForge from a browse/download client into an asset-management workflow.

## Asset management

The desktop client now supports:

- editing asset metadata
- editing categories
- editing tags
- editing creator/source/license fields
- changing attribution-required status
- dedicated file-type filtering by extension
- exact tag filtering
- recycle-bin soft deletion
- restoring deleted assets
- persistent logging for edit/delete/restore actions

New/expanded API behavior:

```text
GET    /api/assets?extension=obj
GET    /api/assets?tag=wood
GET    /api/assets?deleted_only=true
PATCH  /api/assets/:id
DELETE /api/assets/:id
POST   /api/assets/:id/restore
```

Soft deletion never removes the original binary from vault storage.

## Projects

Phase 4 adds the first project workflow.

Projects have:

- name
- engine
- workstation-local project path
- description

API:

```text
GET    /api/projects
POST   /api/projects
GET    /api/projects/:id
PATCH  /api/projects/:id
DELETE /api/projects/:id
GET    /api/projects/:id/assets
POST   /api/projects/:id/assets
DELETE /api/projects/:id/assets/:asset_id
```

The desktop client provides a project selector and **New Project** dialog.

When **Add Selected to Project** is used, the client:

1. downloads the selected asset from the vault
2. creates `DragonForgeAssets` under the registered local project root
3. copies the asset into that folder
4. registers the asset/project relationship on the server
5. logs both client and server activity

Example:

```text
C:\MyGame\
└── DragonForgeAssets\
    └── cube.obj
```

The original vault copy remains untouched.

### Current project-path limitation

A project's `local_path` is a path on the workstation that registered it. If the same DragonForge server is used from multiple developer PCs, that path may not exist on the other workstation. A later phase can introduce per-workstation project mappings.

## Database migration

No manual migration is required. On startup, DragonForge creates the new `projects` and `project_assets` tables and the new indexes if they do not already exist.

Existing Phase 1–3 assets remain intact.

## Validation test

1. Pull/build Phase 4.
2. Confirm the server reports phase 4 / v0.4.0.
3. Filter the existing OBJ using Type = `obj`.
4. Filter the texture using its exact tag.
5. Select an asset and edit its metadata.
6. Move it to the recycle bin.
7. Open Recycle Bin and restore it.
8. Create a project and choose a local project folder.
9. Select an asset and click **Add Selected to Project**.
10. Confirm the file appears under `<project>\DragonForgeAssets\`.
11. Upload the client and server logs for verification.

## Logs

Server:

```text
<storage.data_dir>\logs\server.log.<date>
```

Client:

```text
%APPDATA%\DragonForge\AssetVault\logs\client.log.<date>
```
