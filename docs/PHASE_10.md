# Phase 10 — Verified Backup & Replication

Phase 10 adds server-authoritative vault snapshots, cryptographic verification, retention, and optional replication.

## Snapshot contents

Each backup contains:

```text
dragonforge-<UTC timestamp>/
├── manifest.json
├── database/
│   └── dragonforge.db
├── assets/
└── previews/
```

The SQLite database contains asset metadata, immutable revision history, package metadata, project links, license data, and Phase 9 semantic embeddings.

The content-addressed asset tree and preview cache are copied into the snapshot. Temporary uploads and logs are deliberately excluded.

## Database consistency

DragonForge uses SQLite `VACUUM INTO` to create the backup database. This produces a consistent standalone SQLite snapshot while the server remains online.

## Backup manifest

`manifest.json` records:

- backup format version
- backup ID
- creation timestamp
- DragonForge version
- database filename
- every backed-up file
- file byte size
- SHA-256 hash
- total bytes
- asset-file count
- preview-file count

## Verification

After the snapshot is completed, DragonForge rereads every manifest entry and verifies both size and SHA-256.

A backup that does not pass verification is not reported as successful.

You can rerun verification at any time from the desktop client with **Verify Latest**.

## Retention

Default:

```toml
[backup]
directory = "./backups"
keep = 10
```

After a successful backup, DragonForge removes local snapshots older than the configured retention count.

Partial directories use a hidden `.partial` name and are never treated as completed snapshots.

## Replication

Optional additional roots:

```toml
[backup]
replication_targets = [
    "D:/DragonForge-Backup",
    "//NAS/DragonForge"
]
```

Each completed backup is copied independently to each target, verified at the destination, and then promoted from its temporary partial directory.

A replication failure does not invalidate the verified primary backup. The response and logs clearly identify failed targets.

## Safety constraints

- `backup.directory` cannot be inside the live DragonForge data directory.
- backup IDs reject path traversal
- manifest paths reject parent traversal
- replication uses staged directories before final promotion
- live assets are never deleted or changed by the backup process

## Desktop controls

The top bar now includes:

- **Create Backup**
- **Verify Latest**
- **Backup Status**

The client displays the newest backup ID and size.

## API

List/status:

```text
GET /api/backups
```

Create:

```text
POST /api/backups
```

Verify:

```text
POST /api/backups/:id/verify
```

## Recovery workflow

Phase 10 creates recovery-ready snapshots but intentionally does not overwrite a running live vault from the API.

For disaster recovery:

1. Stop the DragonForge server.
2. Verify the selected backup.
3. Preserve or rename the damaged live data directory.
4. Restore the backup's `database/dragonforge.db`, `assets/`, and `previews/` into the configured data directory.
5. Start DragonForge.
6. Confirm health, assets, packages, revisions, projects, licenses, and semantic search.

Avoiding online in-place restore prevents accidental destruction of a running vault and keeps recovery deliberate and auditable.

## Suggested validation

1. Pull v0.10.0.
2. Start server/client.
3. Click **Backup Status**.
4. Confirm no error and configured backup directory is recognized.
5. Click **Create Backup**.
6. Confirm the client reports a verified backup.
7. Inspect the backup folder and confirm manifest/database/assets/previews.
8. Click **Verify Latest**.
9. Confirm all files verify successfully.
10. If a replication target is configured, confirm the same backup appears there and has been verified.
11. Create more backups than the retention count in a small test configuration and confirm old snapshots are pruned.
12. Send client/server logs for Phase 10 validation.
