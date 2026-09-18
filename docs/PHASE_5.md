# Phase 5 — Richer Asset Previews

Phase 5 expands DragonForge previews beyond 2D image thumbnails and adds server-generated previews for common 3D game-model formats.

## Supported previews

DragonForge now generates cached PNG previews for:

- JPG / JPEG
- PNG
- WebP
- OBJ
- GLB
- glTF with embedded base64 buffers

The preview renderer runs on the DragonForge server and does not require Blender, a GPU, or a game engine.

## Generic preview endpoint

New endpoint:

```text
GET /api/assets/:id/preview
```

The older Phase 3 endpoint remains available as a compatibility alias:

```text
GET /api/assets/:id/thumbnail
```

Both routes return cached PNG preview images.

## 3D preview renderer

OBJ, GLB, and supported glTF assets are parsed server-side and rendered as a static shaded isometric view.

The renderer:

- centers the mesh
- normalizes model scale
- applies an isometric-style camera angle
- renders triangle faces
- adds visible mesh edges
- writes the result as a PNG
- caches the PNG by asset SHA-256

Original asset files are never modified.

Existing Phase 4 OBJ files automatically receive a preview the first time a Phase 5 client requests one, so no re-upload is necessary.

## glTF limitations

Phase 5 supports:

- self-contained `.glb`
- `.gltf` files whose binary buffers are embedded as base64 data URIs

Phase 5 does not yet resolve external glTF companion files such as:

```text
model.gltf
model.bin
texture.png
```

Those multi-file asset packages will be handled by a later package/dependency-aware asset phase.

The Phase 5 renderer also focuses on geometry. It does not yet reproduce:

- material textures
- PBR shaders
- skeletal poses
- animation playback
- scene lighting from the source asset

## Desktop client

The Phase 5 desktop client automatically requests previews for supported image and 3D formats.

This means:

- OBJ files can display a rendered model preview in the asset grid
- OBJ files can display a larger rendered view in Asset Details
- GLB and supported embedded glTF files behave the same way
- image preview behavior from Phase 3 remains unchanged
- unsupported formats continue to use type placeholders

## Logging

Preview generation and retrieval are logged.

Server events include:

- preview requested
- preview generated
- preview served
- preview generation error
- unsupported preview type

Client events include:

- preview loaded
- preview fetch failed
- preview decode failed

This makes preview problems diagnosable from the same client/server log pair used in earlier phases.

## Phase 5 validation test

Your existing `cube.obj` is ideal for this test.

1. Pull Phase 5.
2. Start the server and client.
3. Confirm the client reports Phase 5 / v0.5.0.
4. Find the existing `cube.obj`.
5. Confirm a rendered 3D preview appears in the grid.
6. Select the OBJ and confirm the larger preview appears in Asset Details.
7. Confirm the existing wood image preview still works.
8. If available, upload a self-contained GLB and check its preview.
9. Stop both programs cleanly.
10. Upload the client and server logs for verification.

The server should create preview PNGs under:

```text
<storage.data_dir>/previews/
```

No existing asset migration is required.
