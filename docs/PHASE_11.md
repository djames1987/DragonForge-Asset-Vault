# Phase 11 — Engine Integrations & Project Sync

Phase 11 turns DragonForge projects from one-time export targets into engine-aware, auditable working copies.

## Engine presets

DragonForge now has built-in project presets for:

- Generic
- Godot
- Unity
- Unreal Engine
- Roblox / Rojo
- Minecraft Bedrock

Each preset defines:

- canonical engine ID
- display name
- default DragonForge export subdirectory
- project marker files/directories used for validation
- engine-specific import notes

### Export destinations

```text
Generic            <Project>/DragonForgeAssets
Godot              <Project>/assets/dragonforge
Unity              <Project>/Assets/DragonForge
Unreal Engine      <Project>/Content/DragonForge
Roblox / Rojo      <Project>/src/DragonForgeAssets
Minecraft Bedrock  <Project>/DragonForgeAssets
```

Unreal source assets may still require import through Unreal Editor. DragonForge does not generate `.uasset` files.

Minecraft Bedrock layouts vary by asset type, so Phase 11 preserves source material under the project export area rather than guessing behavior/resource-pack references.

## Project creation

The desktop project dialog now uses the server-provided engine preset list instead of a free-form engine field.

The dialog displays:

- selected engine
- export destination under the project root
- engine-specific import notes

The server validates and normalizes the engine ID on both project creation and project update.

## Engine-aware export

New project asset exports use the engine preset destination rather than always writing to `DragonForgeAssets`.

Single-file assets are written directly into the engine export root.

Packages are written below an asset-name folder while preserving all internal relative paths and dependencies.

Every downloaded file is written to a temporary partial file, SHA-256 verified against the vault manifest, and only then promoted to its final project location.

## Backward compatibility

Projects created before Phase 11 may already have relationships pointing into:

```text
<Project>/DragonForgeAssets
```

Those relationships remain valid even when the project engine would now choose another export directory.

Existing links are not silently migrated. Repair/update operations preserve the recorded location unless the asset is newly added.

## Project Sync Status

The desktop client adds **Check Project Sync**.

For every asset registered to the selected project DragonForge checks:

- the project-pinned version
- the current vault version
- every expected exported file
- local SHA-256 against the exact package/version manifest
- engine project markers

Each asset can report:

- in sync
- outdated
- missing files
- modified files
- an explicit error

A project can therefore detect both vault-version drift and local project-file drift.

## Repair Pinned Files

**Repair Pinned Files** restores the exact versions currently pinned to the project.

This operation:

- does not upgrade project versions
- downloads the pinned revision
- restores missing files
- overwrites modified DragonForge-managed files
- verifies every download before promotion
- preserves package folder structure
- refreshes project credit/license manifests

This is the safe choice when a project intentionally remains on an older asset revision.

## Update Project to Latest

**Update Project to Latest** is explicit and separate from repair.

For every project asset it:

1. fetches the current vault revision
2. exports that revision
3. verifies downloaded files
4. updates the project relationship to the new version
5. refreshes project attribution/license files
6. reruns the sync report

DragonForge therefore never upgrades a project merely because a newer vault revision exists.

## Removal safety

Phase 8.1 removal was updated for engine-aware paths.

Removal now accepts:

- the active engine preset export root
- legacy `DragonForgeAssets` links created before Phase 11

Path traversal, absolute paths, paths outside the project root, and attempts to remove the project root remain rejected.

Package removal continues to remove the complete exported package folder while leaving the vault asset and immutable revision history untouched.

## Engine marker checks

When checking project sync, DragonForge looks for engine-specific markers:

```text
Godot      project.godot
Unity      Assets or ProjectSettings
Unreal     Content
Roblox     default.project.json
Minecraft  manifest.json
Generic    no marker required
```

A missing marker is a warning, not a destructive failure. This accommodates incomplete/new project folders while still surfacing likely configuration mistakes.

## API

List engine presets:

```text
GET /api/engines/presets
```

Get the normalized export plan for a project:

```text
GET /api/projects/:id/export-plan
```

Existing project asset APIs continue to provide the pinned version and relative exported path used by the desktop sync engine.

## Suggested validation

1. Pull v0.11.0 and start server/client.
2. Confirm the banner shows server phase 11 / v0.11.0.
3. Create a new Godot project and select a folder containing `project.godot`.
4. Add a normal asset and confirm it exports under `assets/dragonforge`.
5. Add a package and confirm its entire hierarchy exports below that same engine root.
6. Click **Check Project Sync** and confirm both entries are in sync.
7. Modify one exported file manually and delete another.
8. Check sync again; confirm modified/missing states are reported.
9. Click **Repair Pinned Files** and confirm hashes/files return to the pinned revisions.
10. Create/upload a newer vault revision for one project asset.
11. Check sync and confirm it reports outdated while the project remains pinned.
12. Click **Update Project to Latest** and confirm the project pin advances and the report becomes clean.
13. Remove an engine-exported single asset and package; confirm only their managed project copies are removed.
14. Check `CREDITS.txt` and both license manifests after repair/update/removal.
15. Send the client/server logs for Phase 11 validation.
