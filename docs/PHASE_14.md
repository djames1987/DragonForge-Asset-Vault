# Phase 14 — Vault Storage Tiers & Archival

Phase 14 adds a reversible hot/archive storage system for large game-development asset libraries.

Phase 13 remains intentionally unimplemented in this branch so a future authenticated users/roles milestone can be designed without mixing security semantics into storage migration.

## Goals

DragonForge's normal content-addressed store remains the **hot** tier:

```text
<storage.data_dir>/assets/
```

An optional external disk, larger HDD, NAS mount, or other filesystem path may be configured as the **archive** tier:

```toml
[storage]
data_dir = "./data"
archive_dir = "D:/DragonForge-Archive"
```

The archive root must be outside `storage.data_dir`.

## What moves

Archiving an asset migrates every unique binary referenced by that asset:

- current primary asset binary
- all immutable historical asset revisions
- every package file for every package revision
- package dependencies that are physically stored as package files

Metadata, project relationships, licenses, semantic embeddings, checkouts, and revision records remain in SQLite.

Previews remain in hot cache storage because they are small, regenerable derived data.

## Verified transition workflow

For every storage object DragonForge:

1. resolves the current source path
2. copies to a temporary destination file
3. computes SHA-256 on the destination
4. compares it to the catalog SHA-256
5. promotes the verified temporary file to its final archive/hot path
6. after all copies succeed, updates all matching catalog references in one SQL transaction
7. removes an old source copy only if the catalog no longer contains any reference to that source path

A failed copy or hash verification occurs before catalog path mutation.

## Shared-object safety

Historical revisions and package files may reference the same content-addressed object.

DragonForge counts references across:

```text
assets
asset_versions
package_files
```

A source file is deleted only when the reference count for its old storage key reaches zero after the transition.

This prevents moving one asset from deleting a physical object that another asset/revision still uses.

## Transparent reads

Archived paths are stored using:

```text
archive://assets/<sha-prefix>/<sha>.<ext>
```

The storage resolver transparently maps that URI to the configured archive root.

Existing features therefore continue working without special archive-specific versions of their APIs:

- asset download
- thumbnails / preview generation
- immutable version download
- package-file download
- project export
- project repair/update
- license workflows
- semantic search metadata
- checkout/check-in

## Checkout integration

Archive and recall are treated as mutating operations for collaboration purposes.

If an asset is checked out, another workstation cannot archive or recall it. The checkout holder may perform the transition.

Unlocked assets preserve the current trusted-LAN behavior.

## Desktop client

The scrollable Asset Details panel now shows:

```text
Storage Tier: HOT · N objects
```

or:

```text
Storage Tier: ARCHIVE · N objects
Last tier transition: <timestamp>
```

Available actions:

- **Archive Asset**
- **Recall to Hot Storage**
- **Refresh Storage Status**

If the server has no `archive_dir`, the client displays that the archive tier is not configured and does not offer an archive operation.

## API

Read status:

```text
GET /api/assets/:id/storage-tier
```

Archive:

```text
POST /api/assets/:id/archive
```

Recall:

```text
POST /api/assets/:id/recall
```

Archive/recall honor Phase 12 DragonForge identity headers and lock enforcement.

## Backup integration

Phase 10 verified backups now include the configured archive tier under:

```text
archive/assets/
```

Those files participate in the same backup manifest and SHA-256 verification process.

Recovery must restore that folder to the configured archive root because archived catalog paths retain the `archive://` scheme.

## Suggested validation

1. Pull v0.14.0.
2. Configure an archive directory on another test drive/folder outside `data_dir`.
3. Start server/client and confirm phase 14 / v0.14.0.
4. Select a normal single-file asset and confirm Storage Tier = HOT.
5. Click **Archive Asset**.
6. Confirm the client reports ARCHIVE and the hot content file disappears only if no other references require it.
7. Download and preview the archived asset; both should still work.
8. Add the archived asset to a project and confirm project export works.
9. Click **Recall to Hot Storage** and confirm the tier returns to HOT.
10. Repeat with the multi-file spaceship package; verify every package/revision object transitions.
11. While an asset is checked out by a different identity, attempt Archive/Recall and confirm 409 lock enforcement.
12. Create a Phase 10 backup while at least one asset is archived.
13. Verify the backup and confirm `archive/assets/` exists in the snapshot.
14. Send client/server logs for Phase 14 runtime validation.
