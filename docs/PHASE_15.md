# Phase 15 — Audit Trail, Activity History & Administrative Oversight

Phase 15 adds durable operational accountability on top of Phase 13 authentication and Phase 12 collaboration.

## Append-only audit data

DragonForge adds the SQLite table:

```text
audit_events
```

Each record stores:

- unique event ID
- RFC3339 timestamp
- authenticated user ID and username
- authenticated role
- workstation identity
- normalized action name
- HTTP method and API path
- target type and target ID when available
- success/failure
- HTTP status code
- safe diagnostic detail when applicable

Normal application APIs expose no update or delete operation for audit records. Audit history is therefore append-only through supported DragonForge interfaces.

Because audit data lives in SQLite, Phase 10 verified backups automatically preserve the activity history.

## What is audited

Phase 15 records successful and failed API requests, including authorization failures.

Normalized actions include:

```text
asset.upload
asset.update
asset.delete
asset.restore
asset.checkout
asset.checkin
asset.version.upload
asset.version.restore
asset.archive
asset.recall
package.import
package.version.upload
project.create
project.update
project.delete
project.asset.add
project.asset.remove
user.create
user.update
user.delete
backup.create
backup.verify
audit.view
```

Other requests receive a generic `api.<method>` action.

Authentication failures are recorded without storing submitted bearer tokens.

## Activity API

Query activity:

```text
GET /api/audit
```

Supported filters:

```text
username
action
result
target_type
target_id
from
to
limit
offset
```

`from` and `to` use RFC3339-compatible timestamp strings.

Administrators can query the whole vault.

Developer and Read-only accounts are restricted to their own authenticated username even if they submit another username filter.

## Audit export

Administrator-only:

```text
GET /api/audit/export
```

The export response contains both structured events and RFC4180-style CSV content.

The desktop client writes:

```text
DragonForge-Audit-<timestamp>.json
DragonForge-Audit-<timestamp>.csv
```

into the Administrator-selected folder.

## Desktop Activity window

The top bar now includes:

```text
Activity
```

The Activity window includes filters for:

- username
- action
- success/failure
- target type
- target ID
- start timestamp
- end timestamp

Each result displays user, role, workstation, action, result, target, method/path, timestamp, and safe detail text.

Administrators also receive:

```text
Export JSON + CSV
```

## Per-asset activity

Asset Details now includes:

```text
Asset Activity
```

This opens Activity pre-filtered to:

```text
target_type = asset
target_id = <selected asset ID>
```

## Role-aware desktop UI

Phase 13 already enforces authorization on the server. Phase 15 mirrors those permissions in the desktop UI for clearer behavior.

Read-only users keep access to read operations such as:

- search
- previews
- downloads
- version history
- package inspection
- project sync inspection
- activity inspection

Mutation controls are disabled, including:

- Add Asset
- Add Package ZIP
- Edit Metadata
- Check Out / Check In
- archive / recall
- restore
- version/package-version uploads
- project creation
- project repair/update
- project add/remove
- semantic reindex

Administrator-only backup mutation controls are disabled for Developer and Read-only users.

These UI rules are convenience and clarity only. Phase 13 server authorization remains the security boundary.

## Failed authorization auditing

Missing/invalid authentication attempts are recorded as failures with HTTP 401.

Authenticated but unauthorized requests are recorded as failures with HTTP 403, including the authenticated user and role.

Raw Authorization headers and bearer tokens are never written to the audit table.

## Phase integration

### Phase 10

Audit history is stored in SQLite and included in verified backups automatically.

### Phase 12

Checkout/check-in events include the authenticated user and workstation.

### Phase 13

User, role, and permission-denied events are attributed to authenticated identities.

### Phase 14

Archive/recall actions are audited like other protected mutations.

## Suggested validation

1. Pull v0.15.0 and start server/client.
2. Confirm Phase 15 / v0.15.0.
3. Log in as Administrator and open **Activity**.
4. Edit an asset, check it out/in, archive/recall it, and add/remove it from a project.
5. Refresh Activity and confirm those actions appear with the Administrator username/workstation.
6. Click **Asset Activity** and verify the selected asset filter is applied.
7. Export JSON + CSV and inspect both files.
8. Log in as Developer and confirm Create Backup is disabled.
9. Perform a normal asset mutation and confirm it appears in that Developer's activity.
10. Log in as Read-only and confirm mutation controls are disabled.
11. Confirm Read-only can still preview/download/search and inspect their activity.
12. Trigger one 403 using a direct API request or older client and confirm a failure event is recorded.
13. Create/verify a backup and confirm the SQLite snapshot contains Phase 15 audit history.
14. Send client/server logs for runtime validation.
