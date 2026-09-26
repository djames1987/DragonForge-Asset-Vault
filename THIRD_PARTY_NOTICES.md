# Third-Party Notices

This repository is proprietary/source-visible DragonForge source under the current `LICENSE`. That license applies only to original DragonForge material. Third-party software and material retain their own copyright and license terms and independently granted rights. Historical rights already granted to earlier copies of this project are not revoked by this notice.

## Rust dependencies

The application uses Cargo-resolved third-party crates for GUI, HTTP, serialization, SQLite/database access, archive handling, image/model formats, logging, UUIDs, hashing, and asynchronous runtime support. The current repository tree does not vendor those crates.

The current tree does not contain a committed `Cargo.lock`, so an exact transitive release graph cannot be reconstructed from GitHub source inspection alone. Reproducible discovery command:

```powershell
cargo metadata --format-version 1 > dependency-metadata.json
```

For a public release, first generate and commit/review a release lockfile (or otherwise freeze the exact dependency graph), then run:

```powershell
cargo metadata --locked --format-version 1 > dependency-metadata-locked.json
```

Inspect every package whose `source` is non-null and record its `license` or `license_file`. Missing or uncertain license metadata, or redistribution terms not reviewed for the intended release, are blockers. A compatible tool such as `cargo-deny` may be used in addition, with its configuration and output retained as release evidence.

Common permissive Rust license families such as MIT, Apache-2.0, BSD-style, ISC, Zlib, and Unicode-related licenses may occur in Cargo graphs; this notice does not assert that a family is present unless the generated metadata for the exact release shows it.

**Publication gate `DF-P3-AV-001`:** before a public binary release, freeze the exact Cargo dependency graph and complete the transitive license inventory. Source-repository publication may proceed only if no third-party material with unresolved redistribution rights is added to the tree, but binary release qualification remains blocked until this evidence exists.

## Bundled material

No third-party font set, icon library, artwork collection, screenshot pack, npm dependency tree, Python package tree, vendored dependency directory, or copied upstream source bundle was identified in the current repository tree during the Phase 3 inspection.

## Release rule

Preserve all notices and license texts required by the exact dependencies/material actually distributed. Do not infer permission from a package name, common ecosystem practice, or an older dependency graph. Uncertain or missing redistribution rights block the affected artifact until resolved.
