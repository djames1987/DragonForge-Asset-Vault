# Phase 12 — Asset Checkout & Collaboration Locks

Phase 12 adds the check-out/check-in workflow that was deferred from the earliest DragonForge phases.

The goal is to let multiple developers share one vault without accidentally editing or versioning the same source asset at the same time.

## Trusted-LAN identity

The desktop client derives a collaboration identity from the workstation environment.

Default Windows identity:

```text
USERNAME@COMPUTERNAME
```

Cross-platform fallbacks use `USER` and `HOSTNAME`.

Optional explicit overrides:

```text
DRAGONFORGE_USER
DRAGONFORGE_WORKSTATION
```

Every desktop HTTP client automatically sends:

```text
X-DragonForge-User
X-DragonForge-Workstation
```

These headers identify the workstation for collaboration locking.

This is **not authentication**. DragonForge remains scoped to a trusted private LAN. A later security phase can introduce authenticated users, roles, and stronger authorization.

## Checkout records

Phase 12 adds the SQLite table:

```text
asset_checkouts
```

Each active checkout records:

- asset ID
- holder
- workstation
- optional checkout note
- checkout timestamp

Only one checkout may exist for an asset at a time.

Checkout records are part of the normal SQLite catalog and therefore included in Phase 10 backups.

## Check Out

The Asset Details panel now shows the selected asset's checkout state.

When unlocked, the user can enter an optional note and click:

```text
Check Out
```

The server atomically acquires the checkout.

If another workstation acquired the checkout first, the request returns a conflict identifying the current holder.

Repeated checkout by the same holder/workstation is idempotent and returns the existing lock.

## Check In

When the currently selected asset is checked out by the local identity, the details panel shows:

```text
Check In
```

Only the matching holder/workstation identity may release the checkout.

Another workstation receives a conflict rather than silently stealing the lock.

## Server-enforced mutation protection

When an asset has an active checkout, DragonForge blocks mutation requests from any different identity.

Protected operations:

- metadata edits
- single-file version uploads
- package version uploads
- version restores
- soft delete
- recycle-bin restore

Read-only operations continue to work:

- search
- previews
- downloads
- package inspection
- version history
- project sync checks

Project export does not mutate the vault asset itself and therefore remains available.

## Backward compatibility

DragonForge does **not** require every asset to be checked out before editing.

If an asset has no checkout record, existing mutation behavior remains unchanged.

This makes Phase 12 safe for current single-user workflows while enabling enforced coordination whenever a checkout is acquired.

A future strict-workflow option may require checkout before mutation if desired.

## Delete behavior

If the checkout holder soft-deletes an asset, DragonForge automatically clears that asset's checkout record after the delete succeeds.

This prevents stale locks from surviving a move to the recycle bin.

## Desktop identity visibility

The client top bar displays:

```text
Identity: user@workstation
```

Asset Details displays one of:

- checkout status not loaded
- unlocked / Check Out
- checked out by the local workstation / Check In
- checked out by another workstation / LOCKED

## API

List active checkouts:

```text
GET /api/checkouts
```

Read an asset checkout:

```text
GET /api/assets/:id/checkout
```

Acquire:

```text
POST /api/assets/:id/checkout
```

Body:

```json
{
  "holder": "David",
  "workstation": "DEV-PC",
  "note": "Editing model topology"
}
```

Release:

```text
DELETE /api/assets/:id/checkout
```

Release uses the DragonForge identity headers and succeeds only for the current holder/workstation.

## Phase 11.1 integration

Phase 12 includes the Phase 11.1 maintenance fix.

After an engine-aware **Remove from Project**, DragonForge regenerates:

```text
CREDITS.txt
DragonForge-License-Manifest.json
DragonForge-License-Manifest.csv
```

so removed project assets disappear from attribution output automatically.

## Suggested validation

### Phase 11.1

1. Add a licensed asset to a project.
2. Confirm it appears in project credits/manifests.
3. Remove it from the project.
4. Confirm the server logs a new project license report.
5. Confirm the asset no longer appears in the three generated attribution files.

### Phase 12 single-client

1. Pull v0.12.0 and start server/client.
2. Confirm the banner reports phase 12 / v0.12.0.
3. Select an active asset.
4. Load checkout status if needed.
5. Enter a note and click **Check Out**.
6. Confirm the holder/workstation and timestamp appear.
7. Edit metadata or upload a new revision; it should succeed for the same identity.
8. Click **Check In**.
9. Confirm the asset becomes unlocked.

### Phase 12 two-client lock enforcement

1. Start a second DragonForge client from another Windows account/computer, or set different `DRAGONFORGE_USER` / `DRAGONFORGE_WORKSTATION` values before starting it.
2. Check out an asset in client A.
3. In client B, refresh/load that asset's checkout state.
4. Confirm it shows client A as the holder.
5. Attempt a metadata edit or version upload from client B.
6. Confirm the server rejects the mutation with a checkout conflict.
7. Check in from client A.
8. Repeat the mutation from client B; it should now succeed.
9. Send client/server logs for Phase 12 validation.
