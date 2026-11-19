# ASOE entity placements and collision planes

The exact revision-0 source accepted by the crate contains 3,185 interactables, 219 item boxes and 300 enemies across 18 stage slots. The three contiguous 72-byte pointer tables begin at ROM offsets 0xB7F94 (interactables), 0xB7FDC (item boxes) and 0xB8024 (enemies). Each points to a type-0x30 RLE spatial layout. The inner length includes its four-byte size word. Region offsets are relative to byte four. An occupied list ends at the unsigned X marker 255. The generic entity stride is seven bytes, and an item box occupies three. World coordinates are region index times 256 plus local coordinate times eight.

[The evidence manifest](../tests/entity-evidence.tsv) records all 54 pointer targets, consumed compressed lengths, SHA-256 digests of the table, compressed extent, decoded extent and ordered placements, grid dimensions, counts and reference CSV Git blob IDs. The placement digest packs each world X and Y as little-endian signed 32-bit integers, followed by the kind byte and four raw parameter bytes for enemies/interactables. Native identifier resolution and all ordered records matched the original reference CSVs.

The reference is SAT-R/sa2 at commit 2dd3f17d30cd3c156e508aac0017620ae485b9ba. Source paths and Git blob IDs:

| Source | Blob |
| --- | --- |
| src/data/sa1/entity_tables.c | 83081827ca5f565e796f83089d3ac43bb73cdd8b |
| include/game/shared/stage/entity.h | cc10ce933beee2e1dfe892911d4abfe15169e443 |
| src/game/shared/stage/entities_manager.c | 96f75edac8ade35d731e6f7fd8e2ef88882d2c50 |
| src/game/sa1/stage/interactables/toggle_player_layer.c | f0f613cec55f46b655a1386589cb20103c377408 |
| include/constants/sa1/interactables.h | 27aad95567b26db40ed22afca28424cfe5d3cb3c |

From the crate directory, run the following checks with the pinned reference checkout available locally and SONIC_ADVANCE_ASOE_ROM set to the exact ROM path.

~~~
python tests/verify_entity_reference.py /path/to/local/sa2
cargo test --test entities -- --include-ignored
~~~

In regular stage layouts, kind 17 clears collision-plane bit zero and kind 18 sets it. The native task compares the integer player center to both inclusive edges of the rectangle. Width and height come from parameter bytes two and three in tile units. The first two bytes do not offset the rectangle. Separate Single Pak Collect Rings tables and task IDs are outside this reader.

Across the 389 plane triggers, 239 opposite-plane pairs have intersecting inclusive rectangles. LayerTriggers preserves activation order across frames. The manager scans the region window on entry and after a camera jump (>248 horizontally or >208 vertically); smaller moves scan the incoming X strip and then the Y strip. Equal-priority tasks append in creation order (src/task.c). A trigger tests the player before destroying itself outside the 128-pixel camera margin. The overlap regression enters the right-hand trigger first, then the left, and checks their final tick on culling. The stateless collision_layer_at helper is for layout inspection. Retail per-frame traces with native camera motion and cross-task scheduling remain unverified. Decoding enemy, item and interactable placements does not implement their remaining native behaviors.
