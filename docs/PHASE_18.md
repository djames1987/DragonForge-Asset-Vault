# Phase 18 — Asset Relationships, Variants & Derivatives

Phase 18 adds explicit asset lineage to DragonForge.

Phase 17 remains intentionally unimplemented in this branch. Collections, Favorites, Saved Searches, and Bulk Asset Management can be added later without changing the Phase 18 graph model.

## Purpose

A game-development asset is often represented by several independently useful files:

```text
Blender source
   ├── GLB export
   ├── FBX export
   ├── Unity-ready export
   ├── Unreal-ready export
   ├── LOD0
   ├── LOD1
   ├── collision mesh
   ├── textures
   └── materials
```

Before Phase 18 those files could be stored in DragonForge but there was no durable vault-level statement that they belonged to the same asset lineage.

Phase 18 adds that relationship layer without merging those files into one revision history.

## Relationship model

Each relationship contains:

- unique relationship ID
- source/base asset ID
- related asset ID
- relationship type
- optional label
- optional note
- created timestamp
- updated timestamp

The relationship is directional:

```text
source/base asset  ──relationship──> related asset
```

When viewing the source asset, the entry is reported as `outgoing`.

When viewing the related asset, the same database record is reported as `incoming`.

## Supported relationship types

```text
variant
derivative
export
lod
collision
texture
material
animation
engine_export
reference
```

These cover the common DragonForge workflows while keeping a generic Reference type for relationships that do not fit a derivative hierarchy.

## Database

Phase 18 adds:

```text
asset_relationships
```

with foreign keys to both assets.

The database enforces:

- no duplicate source/related/type combination
- cascading cleanup if an asset record is permanently removed

The API additionally rejects self-relations.

Soft-deleted/recycle-bin assets keep their relationships so lineage is not lost during ordinary recycle/restore workflows.

## API

List all incoming/outgoing relationships for an asset:

```text
GET /api/assets/:id/relationships
```

Create an outgoing relationship:

```text
POST /api/assets/:id/relationships
```

Example:

```json
{
  "related_asset_id": "asset-id",
  "kind": "lod",
  "label": "LOD1",
  "note": "50 percent triangle reduction"
}
```

Update relationship metadata/type:

```text
PATCH /api/assets/:id/relationships/:relationship_id
```

Delete:

```text
DELETE /api/assets/:id/relationships/:relationship_id
```

## Collaboration and authorization

Relationship mutations use the existing Phase 12/13 mutation boundary.

- Administrator: read/write
- Developer: read/write
- Read-only: read only

If the selected asset is checked out by another identity, relationship mutations on that asset are rejected by the same server-side lock logic used for other protected asset changes.

## Audit integration

Phase 15 normalizes these actions:

```text
asset.relationship.create
asset.relationship.update
asset.relationship.delete
asset.relationship.view
```

Bearer tokens are never stored in audit details.

## Desktop client

Asset Details now includes:

```text
RELATIONSHIPS
```

Each relationship shows:

- incoming/outgoing direction
- type
- related asset name
- optional label
- optional note
- recycle-bin status

Actions:

- **Show Asset**
- **Remove Link**
- **+ Add Relationship**
- **Refresh Links**

## Add Relationship dialog

Developer/Administrator users can:

1. choose a relationship type
2. search active vault assets by name, filename, category, or tag
3. choose the related asset
4. add an optional label
5. add an optional note
6. create the relationship

The source asset itself is excluded from the candidate list.

The candidate request is intentionally bounded to the newest 500 active assets, matching the current server's maximum normal asset-list page size. Larger vaults can be expanded later with server-side candidate paging/search.

## Relationship navigation

**Show Asset** switches the Asset workspace to the related asset and refreshes its:

- checkout status
- storage tier
- relationships

This works regardless of whether the original asset was opened from All Assets or project-scoped browsing.

## Revision behavior

Relationships attach to asset identity, not one revision.

Creating:

```text
spaceship.blend --export--> spaceship.glb
```

continues to mean the same thing if either asset later advances from v2 to v3.

Phase 6 immutable revision history remains independent.

## Package behavior

Phase 7 packages remain self-contained multi-file asset revisions.

Phase 18 relationships are for relationships between separate DragonForge assets.

A package can itself participate in the relationship graph, for example:

```text
character-source.blend --export--> character-runtime-package.zip
```

## Project behavior

Phase 16.2 project-scoped browsing can display relationships normally.

Project membership and asset lineage are intentionally separate:

- Project membership answers: "Is this asset exported/pinned into this game project?"
- Relationship graph answers: "How is this asset related to other vault assets?"

## Backup and storage behavior

Relationship records live in SQLite and are therefore included automatically in Phase 10 verified backups.

Archiving or recalling an asset in Phase 14 does not alter its relationships.

## Suggested validation

1. Pull v0.18.0.
2. Start server/client and confirm Phase 18 / v0.18.0.
3. Select a source asset such as a Blender/OBJ/GLTF source.
4. Add a Derivative or Export relationship to another asset.
5. Confirm it appears as outgoing on the source.
6. Click **Show Asset**.
7. Confirm the same relationship appears as incoming on the related asset.
8. Add LOD, Collision, Texture, or Engine Export links.
9. Attempt a duplicate source/target/type relationship and confirm DragonForge rejects it.
10. Attempt to relate an asset to itself and confirm rejection.
11. Remove a relationship and confirm it disappears from both sides.
12. Test as Read-only and confirm lineage is visible but Add/Remove is disabled.
13. Check out a source asset as another user and confirm relationship mutations respect the lock.
14. Verify relationship create/delete events appear in Activity.
15. Create/verify a backup and confirm the SQLite snapshot includes the Phase 18 relationship table.
