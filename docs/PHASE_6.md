# Phase 6 — Asset Versioning and History

Phase 6 adds immutable revision history to DragonForge assets.

## Version model

Every asset now has a version history.

Existing Phase 1–5 assets are automatically imported as:

```text
v1 — Imported from pre-Phase-6 asset
```

No manual migration is required.

New assets created under Phase 6 also begin at version 1.

Each version records:

- version number
- original filename
- extension
- MIME type
- byte size
- SHA-256
- immutable vault storage path
- optional change note
- creation timestamp

The current asset record points at the binary for the active/latest revision.

## Uploading a revision

In the desktop client:

1. Select an asset.
2. Click **Version History**.
3. Enter an optional change note.
4. Click **Upload New Version**.
5. Choose the replacement file.

DragonForge stores the new file using the same content-addressed storage system and creates the next sequential version.

Uploading a file identical to the current revision is rejected.

## Restoring an older revision

Version History provides:

```text
Restore vN as New Current Version
```

A restore never rewrites or deletes history.

For example:

```text
v1  Original
v2  Updated mesh
v3  New textures
v4  Restored from version 1
```

This means the complete audit trail remains intact.

## Project version pinning

When an asset is added to a DragonForge project, the project relationship now records the exact asset version used at that time.

Example:

```text
Project: Test
Asset: Missiles.glb
Pinned version: v2
```

Uploading v3 to the vault does not silently rewrite the project record to say it used v3.

Re-adding the asset to the project updates the project record to the currently selected/current vault revision.

## API

Version history:

```text
GET /api/assets/:id/versions
```

Upload a new revision:

```text
POST /api/assets/:id/versions
multipart:
  file=<binary>
  note=<optional text>
```

Download a specific revision:

```text
GET /api/assets/:id/versions/:version/download
```

Restore an older revision as a new current version:

```text
POST /api/assets/:id/versions/:version/restore
{
  "note": "optional restore note"
}
```

## Storage behavior

Versioning does not duplicate identical underlying files unnecessarily.

DragonForge's content-addressed vault means a restored version can point to the already stored SHA-256 object.

No historical binary is deleted when a newer version is uploaded.

## Preview behavior

When a new revision becomes current:

- image/3D preview generation runs for the new SHA-256 when supported
- the desktop client invalidates its old preview texture
- the new current preview is fetched
- restoring an old revision reuses its already cached SHA-based preview when available

## Logging

Server logs include:

- version history requests
- revision uploads
- version numbers
- hashes
- version downloads
- restores

Client logs include:

- version-history loading
- revision upload attempts
- restores
- resulting current version

## Validation test

1. Pull/build Phase 6.
2. Start the server and client.
3. Confirm Phase 6 / v0.6.0.
4. Select an existing asset such as `cube.obj` or `Missiles.glb`.
5. Open **Version History** and confirm the existing asset appears as v1.
6. Make a small change to a copy of that asset.
7. Upload it as a new version with a note.
8. Confirm the asset reports v2 and the preview refreshes.
9. Open Version History and confirm both v2 and v1 appear.
10. Restore v1.
11. Confirm DragonForge creates v3 representing the restore.
12. Add the asset to your test project and confirm the server logs the project registration with the current version.
13. Upload the client and server logs for review.

## Migration safety

Phase 6 only adds the `asset_versions` table and a `version_number` field to project-asset tracking. Existing stored binaries and metadata remain intact.
