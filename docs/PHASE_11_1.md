# Phase 11.1 — Project Removal Attribution Maintenance

Phase 11.1 closes the attribution-refresh regression discovered during Phase 11 runtime validation.

After an engine-aware **Remove from Project** operation succeeds, the desktop client now immediately requests a fresh project license report and rewrites:

```text
CREDITS.txt
DragonForge-License-Manifest.json
DragonForge-License-Manifest.csv
```

The removed asset therefore disappears from project attribution output without requiring the manual **Refresh Credits / License Manifest** action.

Removal remains authoritative even if attribution-file regeneration encounters an unrelated filesystem/API error. In that case DragonForge reports the refresh warning while preserving the successful project unlink and file removal.
