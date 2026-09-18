# Phase 7 — Asset Packaging, Dependencies, and Multi-File Assets

Phase 7 makes multi-file game assets first-class DragonForge assets.

## Package import

The desktop client now includes **Add Package ZIP**.

A package ZIP can contain an entire asset tree, for example:

```text
spaceship/
├── spaceship.gltf
├── spaceship.bin
├── materials/
│   └── spaceship.mtl
└── textures/
    ├── hull_basecolor.png
    ├── hull_normal.png
    └── hull_roughness.png
```

DragonForge preserves the paths inside the ZIP and stores every file independently in content-addressed vault storage.

A package has one primary file. You can specify its relative path during import, or leave the field blank and DragonForge will prefer common primary formats in this order:

```text
GLB → glTF → OBJ → FBX → BLEND → common image formats → first file
```

ZIP paths are validated before extraction. Absolute paths and traversal paths such as `..` are rejected.

## Dependency discovery

Phase 7 automatically records dependencies referenced by:

- glTF external buffers
- glTF external images
- OBJ `mtllib` references
- MTL texture maps, including common diffuse/specular/bump/normal/opacity mappings

For OBJ packages, DragonForge follows the OBJ → MTL relationship and inspects referenced MTL files for their texture dependencies.

The package manifest records both:

- referenced dependencies
- referenced dependencies that are missing from the package

The desktop **Package / Dependencies** window displays the package file tree and any missing dependency warnings.

## Package-aware previews

Multi-file previews reconstruct the package in a temporary server workspace using the original relative paths.

This allows Phase 7 to preview glTF packages such as:

```text
model.gltf
model.bin
texture.png
```

External glTF binary buffers are now supported when the required files are part of the DragonForge package.

Temporary preview workspaces are removed after rendering.

## Package versioning

A package revision is versioned as one logical asset.

From **Version History**, choose **Upload Package Version ZIP** to upload the next complete package state.

Every revision keeps its own:

- primary file
- package file manifest
- content hashes
- dependency diagnostics
- relative paths
- optional revision note

The primary file may remain unchanged while another dependency changes. DragonForge still creates a new package revision.

Restoring an older version creates a new current revision and clones that old revision's complete package manifest and dependency records.

## Project export

**Add Selected to Project** is now package-aware.

Single-file assets keep the existing behavior.

Multi-file packages export to:

```text
<project>/
└── DragonForgeAssets/
    └── <Asset Name>/
        ├── primary file
        └── dependency files using their original relative paths
```

The project record is pinned to the exact asset version exported.

## Package API

Import a new package:

```text
POST /api/packages
multipart:
  file=<zip>
  primary_path=<optional relative path>
  name=<optional>
  category=<optional>
  tags=<optional>
  description=<optional>
  creator=<optional>
  source_url=<optional>
  license=<optional>
  attribution_required=<optional>
```

Current package manifest:

```text
GET /api/assets/:id/package
```

Specific revision manifest:

```text
GET /api/assets/:id/versions/:version/package
```

Upload a package revision:

```text
POST /api/assets/:id/package-versions
multipart:
  file=<zip>
  primary_path=<optional>
  note=<optional>
```

Download a package file:

```text
GET /api/assets/:id/versions/:version/package/files/:file_id/download
```

## Compatibility

Existing Phase 1–6 single-file assets remain valid.

When the package manifest endpoint is requested for a legacy single-file asset, DragonForge exposes it as a synthetic one-file package so the project-export workflow can use one consistent API.

No existing asset needs to be re-imported.

## Suggested validation

1. Pull Phase 7 and start server/client.
2. Confirm Phase 7 / v0.7.0.
3. Create a ZIP containing a `.gltf`, its external `.bin`, and any textures.
4. Use **Add Package ZIP**.
5. Leave Primary Path blank if the glTF should be auto-selected.
6. Open **Package / Dependencies** and confirm every file appears.
7. Confirm missing dependencies are zero for a complete package.
8. Confirm the glTF preview renders.
9. Add the package to your test project.
10. Verify the complete directory tree appears under `DragonForgeAssets/<Asset Name>/`.
11. Modify only a dependency, rebuild the ZIP, and use **Upload Package Version ZIP**.
12. Confirm a new revision is created even when the primary file did not change.
13. Restore the earlier package version and verify its file manifest returns.
14. Upload the client/server logs for verification.
