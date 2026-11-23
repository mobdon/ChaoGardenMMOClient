# Native stage catalog

`shared/stage-catalog.json` is a source-only index of the installed game’s stages, areas, boss arenas, kart modes, and identifiable Chao race course sets. It contains 106 entries. Every entry has `available: false`: an original resource reference establishes that a resource exists, not that the browser prototype can load or play the mission.

The catalog combines all 85 level sections in the official X-Hax SA Tools release 786 `GameConfig/PC_SA2/sa2lvl.ini`, the native IDs in X-Hax sa2-mod-loader `SA2Enums.h`, and the original installed filenames recorded in `.local/full-inventory.json`. It preserves story stages, Cannon’s Core character segments, two-player stages, bosses, and Chao areas separately. Native test IDs 0, 1, and 2 remain internal entries. The native invalid-ID sentinel is excluded.

Route 101 and Route 280 share native stage ID 70. Kart Race is stage 71. Six native kart course slots are listed separately, including the internal Download Course slot; its existence does not establish that downloadable course content is installed. Chao areas share native stage ID 90 and therefore use unique string catalog IDs. Eleven installed `ChaoSetRaceBeg*` and `ChaoSetRaceJwl*` course-set variants retain their internal filenames because a verified retail display-name mapping has not yet been established. Matching camera resources are included where installed.

## Schema and regeneration

The catalog has `{version: 1, stages: [...]}`. Every stage contains `id`, `name`, `character`, `family`, `resources`, and `available`. Families are `speed`, `hunt`, `mech`, `chao`, `boss`, `kart`, or `other`. Additional fields preserve native IDs, editor group, default character, adaptation candidates, and evidence. Resource strings are relative installed paths; neither machine-specific installation paths nor original binary content is included.

Run the installed-file inventory first, then regenerate:

```powershell
node scripts/full-inventory.js
node scripts/catalog-stages.js
```

The default mapping directory is `.local/tools/sa-tools-786`; override it with `SA_TOOLS_DIR`. The generator rejects duplicate catalog IDs, resource paths absent from the inventory, and entries marked playable. Current counts are 15 speed, 16 hunt, 21 mech, 13 boss, 30 Chao, 8 kart, and 3 other entries, with 1,129 installed resource references across them. References may be shared between modes or areas.

## Work required for playable adaptations

Original landtables, objects, textures, camera sets, collision, event scripting, character movement, enemy behavior, goal rules, and mission conditions must be adapted and validated before a native mission can become available. Chao running, jumping, flying, climbing, digging, and combat need rules suitable for each stage. Chao Walker and Dark Chao Walker are candidates for mech stages. These candidates describe planned adaptation work rather than original game support for playable Chao in those missions.

The catalog is intentionally broader than the currently playable prototype routes. The stage-select interface should display original resources as indexed or planned, and only offer entry when a separate implemented route has passed its own validation. Extracted assets and conversion products belong in ignored `.local/`; this document and catalog contain names, IDs, and provenance only.

Primary mappings: X-Hax SA Tools `GameConfig/PC_SA2/sa2lvl.ini`, `CART.ini`, and `CART_DLL.ini`; X-Hax sa2-mod-loader `SA2ModLoader/include/SA2Enums.h`.
