# Phase 8.1 — Project Asset Removal

Phase 8.1 adds a complete **Remove from Project** workflow for single-file assets and multi-file asset packages.

## Desktop workflow

With both an asset and project selected, DragonForge now shows:

- **Remove Selected from Project** in the project toolbar
- **Remove from Selected Project** in Asset Details

Removal always opens a confirmation dialog.

The dialog makes clear that:

- the project copy will be removed
- an exported package removes the whole package folder
- the vault asset remains intact
- immutable asset revision history remains intact

## Single-file assets

For a single-file asset, DragonForge removes the exact exported file recorded by the project relationship.

Example:

```text
<Project>/
└── DragonForgeAssets/
    └── wood.png
```

Only `wood.png` is removed.

## Multi-file packages

For packages, DragonForge uses the project-pinned package version and its primary relative path to determine the exported package root.

Example:

```text
<Project>/
└── DragonForgeAssets/
    └── Spaceship/
        └── spaceship/
            ├── spaceship.gltf
            ├── spaceship.bin
            └── textures/
                └── basecolor.png
```

Removing the Spaceship package removes the complete:

```text
DragonForgeAssets/Spaceship/
```

folder rather than leaving orphan dependency files behind.

## Filesystem safety

DragonForge validates the stored project-relative path before touching the filesystem.

Removal is rejected when a path:

- is absolute
- contains `..`
- contains a root/prefix component
- resolves outside `<Project>/DragonForgeAssets`
- would attempt to remove the `DragonForgeAssets` root itself

## Rollback-aware unlink

To avoid leaving the project/database in an inconsistent state, DragonForge uses this sequence:

1. Resolve the exact exported file or package folder.
2. Rename it to a temporary staging path in the same directory.
3. Request the server project unlink.
4. If the unlink fails, rename the staged copy back to its original path.
5. If the unlink succeeds, permanently delete the staged copy.
6. Regenerate project license/credit files.

If the exported file was already manually deleted, DragonForge can still remove the project relationship cleanly.

## Licensing integration

After successful removal DragonForge refreshes:

```text
CREDITS.txt
DragonForge-License-Manifest.json
DragonForge-License-Manifest.csv
```

The removed asset is no longer included in those files.

## API addition

The existing removal endpoint remains:

```text
DELETE /api/projects/:id/assets/:asset_id
```

Phase 8.1 also adds a lookup on the same resource:

```text
GET /api/projects/:id/assets/:asset_id
```

The GET response returns the project asset relationship, including its pinned version and exported relative path.

## Suggested validation

1. Pull v0.8.1 and start the server/client.
2. Select an existing single-file asset already added to a project.
3. Click **Remove Selected from Project**.
4. Confirm the file disappears from `DragonForgeAssets`.
5. Confirm the asset remains in the vault.
6. Add it back to the project.
7. Select a multi-file Phase 7 package.
8. Remove it from the project.
9. Confirm its complete exported package directory is removed.
10. Confirm unrelated assets in `DragonForgeAssets` remain untouched.
11. Open `CREDITS.txt` and both license manifests and confirm removed assets no longer appear.
12. Send the new client/server logs for validation.
