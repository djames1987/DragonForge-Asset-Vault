# Phase 13 — Authenticated Users & Role-Based Access Control

Phase 13 adds optional server-enforced authentication and authorization while preserving DragonForge's existing trusted-LAN mode for installations that do not enable it.

Phase 14 remains present in the same codebase; this phase fills the intentionally reserved security milestone between Phase 12 collaboration locks and Phase 14 storage tiers.

## Authentication model

DragonForge uses bearer API tokens.

When authentication is enabled, clients send:

```http
Authorization: Bearer <token>
```

The server stores only:

```text
SHA-256(token)
```

in SQLite. Raw user tokens are never returned by the user API and are not stored in the vault database.

Tokens must be at least 16 characters.

## Configuration

Authentication is disabled by default for backward compatibility.

Enable it in `DragonForge.toml`:

```toml
[auth]
enabled = true
bootstrap_admin_user = "admin"
bootstrap_admin_token = "replace-with-a-long-random-token"
```

If authentication is enabled and the user table is empty, startup requires `bootstrap_admin_token`. DragonForge creates the first Administrator automatically.

Once at least one user exists, the bootstrap values are ignored on later startups.

If authentication is enabled with an empty user table and no bootstrap token, the server refuses to start instead of exposing a locked or unauthenticated vault accidentally.

## Roles

### Administrator

Administrator can:

- read all normal vault APIs
- upload/edit/delete/restore assets
- create versions and package versions
- check assets out/in
- create/update/delete projects
- archive/recall storage-tier assets
- reindex semantic search
- create and verify backups
- list/create/update/delete DragonForge users
- rotate user tokens
- enable/disable users
- change roles

### Developer

Developer can:

- use all normal read APIs
- upload/edit/delete/restore assets
- manage revisions/packages
- manage projects and project links
- check assets out/in
- archive/recall assets
- reindex semantic search

Developer cannot:

- manage users
- perform backup mutations

Developers may still read backup status.

### Read-only

Read-only can use GET/HEAD APIs, including:

- search
- metadata
- previews
- downloads
- package inspection
- revision history
- project/license reports
- semantic search
- backup status

Read-only cannot perform mutations.

## User database

Phase 13 adds:

```text
vault_users
```

Fields:

- ID
- username
- role
- token hash
- enabled flag
- created timestamp
- updated timestamp
- last API use timestamp

Usernames are unique case-insensitively. Token hashes are unique.

DragonForge prevents deleting, disabling, or demoting the final enabled Administrator.

## API

Current authenticated identity:

```text
GET /api/auth/me
```

Administrator user management:

```text
GET    /api/users
POST   /api/users
GET    /api/users/:id
PATCH  /api/users/:id
DELETE /api/users/:id
```

Create-user body:

```json
{
  "username": "artist",
  "role": "developer",
  "token": "a-long-random-token"
}
```

Update-user body may contain:

```json
{
  "role": "read_only",
  "enabled": true,
  "token": "optional-new-token"
}
```

Supplying `token` rotates the account token.

## Public health endpoint

```text
GET /api/health
```

remains public and reports:

```json
{
  "auth_enabled": true
}
```

This allows a desktop client to discover the server before it has a valid token.

## Desktop client

The client connection bar now includes a masked API Token field.

The token is persisted in the local client settings JSON so users do not need to re-enter it every launch.

For environments where storing the token in that local settings file is undesirable, set:

```text
DRAGONFORGE_API_TOKEN
```

The environment variable overrides the stored client token.

After connection the client displays:

```text
User: <username> · Administrator
```

or the appropriate role.

Administrators receive a **Manage Users** button.

The user-management window supports:

- create user
- select role
- enable/disable user
- change role
- rotate token
- delete user
- inspect last API use

## Phase 12 integration

Before Phase 13, checkout ownership relied on the trusted:

```text
X-DragonForge-User
```

header.

When authentication is enabled, the Phase 13 middleware overwrites that header with the authenticated username before the request reaches checkout logic.

A client therefore cannot claim another user's checkout identity simply by changing the header.

The workstation identity remains:

```text
X-DragonForge-Workstation
```

so checkout ownership is:

```text
authenticated-user@workstation
```

## Phase 14 integration

Archive and recall are ordinary protected mutation operations.

- Administrator: allowed
- Developer: allowed
- Read-only: forbidden

This preserves all Phase 14 storage-tier behavior while adding server-side role enforcement.

## Backup integration

User accounts and token hashes live in SQLite and are therefore included automatically in Phase 10 verified database snapshots.

Raw API tokens are not stored in the database backup.

## Suggested validation

1. Update to v0.14.1.
2. Enable `[auth]` with a bootstrap Administrator token.
3. Start the server and confirm the bootstrap Administrator is created.
4. Start the client without a token; health should connect but protected APIs should return 401.
5. Enter the bootstrap token and click **Connect**.
6. Confirm the UI displays the Administrator username/role.
7. Open **Manage Users**.
8. Create one Developer and one Read-only account with unique 16+ character tokens.
9. Connect a second client using the Developer token.
10. Confirm asset metadata edits and checkout operations succeed.
11. Confirm user-management and backup-mutation requests return 403 for Developer.
12. Connect using the Read-only token.
13. Confirm search/download/preview work.
14. Confirm metadata edit, checkout, upload, project mutation, and archive/recall return 403.
15. Check an asset out as the Developer and confirm the server records the authenticated username rather than a client-supplied username.
16. Attempt to disable or delete the only enabled Administrator; confirm DragonForge refuses.
17. Send client/server logs for Phase 13 validation.


## Phase 13.1 checkout identity maintenance

Runtime testing found an authenticated identity/UI mismatch:

- server checkout holder: authenticated DragonForge username
- client ownership comparison: local Windows username

For example, `Djames@LAPTOP-JAMES` could be a valid server checkout while the local Windows identity remained `DJame@LAPTOP-JAMES`. The client therefore displayed the user's own checkout as locked by somebody else and hid **Check In**.

The client now resolves checkout ownership as:

1. authenticated DragonForge username when Phase 13 auth is enabled and authenticated
2. legacy local user identity when authentication is disabled
3. the normal workstation identity in both modes

This also makes the checkout request UI use the same resolved identity for consistency, while the server remains authoritative and continues overwriting the username from authentication middleware.
