# Phase 8 — Licensing, Attribution, and Project Credits

Phase 8 adds license-safety workflows to DragonForge so assets can carry consistent license metadata and projects can automatically produce attribution files.

## License presets

DragonForge currently provides these canonical presets:

- `CC0-1.0` — Creative Commons Zero v1.0 Universal
- `CC-BY-4.0` — Creative Commons Attribution 4.0 International
- `Custom` — site-specific or otherwise custom terms
- `Unknown` — license not yet verified

The Creative Commons preset identifiers use SPDX-style short identifiers.

## License assessment states

Every asset can be evaluated as:

- **Complete** — recognized preset and required metadata is present
- **Warning** — recognized preset, but important provenance/attribution information is missing
- **Unknown** — license is not recorded or not verified
- **Custom** — license requires manual review

Current validation rules include:

### CC0-1.0

Attribution is not required by the CC0 legal tool, but DragonForge warns when the source URL is missing because provenance becomes difficult to verify later.

### CC-BY-4.0

DragonForge expects:

- creator / credit name
- source URL
- attribution-required flag enabled

Missing fields create a warning status.

### Custom

DragonForge does not attempt to interpret custom legal terms. The asset remains visibly marked for manual review.

## Desktop UI

Phase 8 adds:

- license preset selectors in upload, package import, and metadata edit forms
- automatic attribution-required behavior for CC-BY-4.0
- license health displayed on asset cards
- warnings shown in Asset Details
- library filter for Complete / Warning / Unknown / Custom
- project button to regenerate license files manually

## Project credits and manifests

Whenever **Add Selected to Project** succeeds, DragonForge regenerates:

```text
<Project>/
├── CREDITS.txt
├── DragonForge-License-Manifest.json
├── DragonForge-License-Manifest.csv
└── DragonForgeAssets/
```

The report uses the project's pinned asset-version records.

The JSON report contains:

- asset ID
- asset name
- pinned version
- normalized license ID
- license status
- creator
- source URL
- attribution-required flag
- warnings
- generated credit line

The CSV contains a compact tabular form suitable for spreadsheets and audit review.

## API

License presets:

```text
GET /api/licenses/presets
```

Asset license assessment:

```text
GET /api/assets/:id/license-status
```

Project license report:

```text
GET /api/projects/:id/license-report
```

## Compatibility

Phase 8 does not change stored asset binaries, package files, revision history, or project version pinning.

Existing free-form license strings continue to load. Known CC0 and CC BY aliases are normalized during assessment; other strings are treated as custom/manual-review terms.

## Suggested validation

1. Pull Phase 8 and start server/client.
2. Confirm server phase 8 and v0.8.0.
3. Edit one asset to `CC0-1.0` with a source URL.
4. Confirm License Status shows Complete.
5. Edit another asset to `CC-BY-4.0`, provide creator and source, and confirm attribution is required.
6. Remove its creator temporarily and confirm the status becomes Warning.
7. Set another asset to Unknown and use the License filter to show Unknown assets.
8. Add a licensed asset to a project.
9. Confirm the project root contains `CREDITS.txt`, JSON manifest, and CSV manifest.
10. Open the files and verify the asset, pinned version, creator, source, and license are recorded.
11. Use **Refresh Credits / License Manifest** and confirm the files update.
12. Upload server/client logs for validation.

## Scope note

DragonForge's status is an organizational aid, not legal advice. Custom licenses and unusual site terms still require manual review.
