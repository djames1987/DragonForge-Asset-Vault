# Public Screenshot Capture Checklist

Phase 5 does not fabricate Asset Vault screenshots. Capture these from a clean demo server/client using only synthetic or clearly first-party demo assets.

## Capture set

1. `docs/assets/readme/asset-library-demo.png` — three-region asset library with synthetic asset names, tags, categories, and generated previews.
2. `docs/assets/readme/asset-detail-demo.png` — version/history/license/relationship detail for a synthetic first-party demo asset.
3. `docs/assets/readme/project-workflow-demo.png` — project pinning/export state using a synthetic project and documentation-safe path labels.
4. Optional `docs/assets/readme/ai-search-demo.png` — local AI/keyword search results over the same synthetic dataset.

## Demo data

Use simple project-owned generated assets such as colored geometric PNGs, placeholder text files, and basic generated meshes whose provenance is unambiguous. Use neutral names such as `Demo Crate`, `Demo Material`, `Demo Character`, and `Example Project`. Do not capture externally sourced game art unless its redistribution rights and required attribution are recorded.

## Sanitization

- Exclude API tokens, real usernames, private LAN addresses, machine names, local home paths, server logs, backup destinations, checkout identities, and production project paths.
- Do not show real third-party asset filenames or thumbnails unless their public redistribution terms have been reviewed.
- Prefer PNG at 1600×900 or 1440×900 and optimize before committing.
- Record source commit, demo asset provenance, dimensions, optimized size, and reviewer in the public-readiness report.
- Screenshot work does not resolve `DF-P3-AV-001`; the exact Cargo dependency graph/license inventory remains a separate binary-release gate.
