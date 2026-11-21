# Chao hats (native)

Worn Black Market hats are drawn on the Chao the way the GameCube game draws
them. Sources: GSNE8P `ChaoMain.rel` (decomp symbols) for the code, and the
player's Steam `Data_DLL.dll` for the models. No hat data is in Git.

## How the game draws a worn hat

`al_draw_B` (ChaoMain `.text` 0x88CC) draws the Chao skeleton node by node,
counting nodes in draw order. At node 16 (the head), when the Chao's headgear
byte (+0xD5 of its parameter block) is not 0, it sets the `AL_BODY` texture
list (`.data` 0x97A3C, loaded at `.text` 0x287A8) and switches on the
headgear through the jump table at `.data` 0xD98. Each case draws the hat
object under node 16's matrix with no extra offset:

| Headgear | Hat | GC object (`.data`) | Steam model (`Data_DLL.dll`) |
| ---: | --- | --- | --- |
| 1 | Pumpkin | 0x315320 | `object_al_pumpkinhead_pumpkinhead` |
| 2 | Skull | 0x315D78 | `object_al_skullhead_skullhead` |
| 3 | Apple | 0x3191C8 | `object_al_mask_apple_mask_apple` |
| 4 | Bucket | 0x319C90 | `object_al_mask_bucket_mask_bucket` |
| 5 | Empty can | 0x31A3EC | `object_al_mask_can_mask_can` |
| 6 | Cardboard box | 0x31AA94 | `object_al_mask_cdbox_mask_cdbox` |
| 7 | Flower pot | 0x31B570 | `object_al_mask_flowerpot_mask_flowerpot` |
| 8 | Paper bag | 0x31B99C | `object_al_mask_paperbag_mask_paperbag` |
| 9 | Pan | 0x31C550, 0x31C6F8, 0x31C810 | `object_al_mask_stewpan_mask_stewpan`, `..._stewpan_eye_...`, `..._stewpan_jaw_...` |
| 10 | Stump | 0x31D3C0 | `object_al_mask_stump_mask_stump` |
| 11 | Watermelon | 0x31E178 | `object_al_mask_wmelon_mask_wmelon` |
| 12 | Red wool hat | 0x31F120 | `object_al_mask_wool_a_mask_wool_a` |
| 13 | Blue wool hat | 0x3200C8 | `object_al_mask_wool_b_mask_wool_b` |
| 14 | Black wool hat | 0x321070 | `object_al_mask_wool_c_mask_wool_c` |
| 15 | Pacifier | 0x3216EC | `object_al_mzsk_teethingring_mzsk_teethingring` |
| 16+ | Egg shell | 0x316428 (+ eye 0x3165D0, jaw 0x3167E4) | `object_al_egghead_*` |

Each GC object was matched to the Steam model of the same name by its bounds
centre, radius and vertex chunk header; all of them match exactly. The Steam
`MaskObjObjectList` (0x10258B30) uses the `alo_mask_*` models instead: that
list is the hat lying on the ground (GC `.data` 0x3343EC, drawn by
`fn_60_3B1B4`), not the worn hat.

The headgear numbers are the same ids the server's Black Market uses (3-15).
Pumpkin, skull and egg shells have no source in this game yet, so they are
not drawn (they fail closed).

## In this repo

- `scripts/convert-animal-care.py` converts the worn hats to
  `.local/imports/care/hat-<id>.glb` (and `hat-9-eye`, `hat-9-jaw`) with the
  same Chao-joint frame as the animal parts. `-- --hats-only` converts just the
  hats. It refuses a model whose root transform is not identity.
- The server serves those names from its exact asset allowlist.
- `src/scene.js` (`setHat`) parents the hat parts to Chao node `016` at
  identity whenever the server's `hat` changes.

## Hats falling off (traced, not ported yet)

`ALBHV_Thrown` (`.text` 0x357B4, state 2 at 0x35A0C) and `fn_60_35F9C`
(0x360A8): when the thrown Chao's movement flag 0x800 is set and it wears a
hat, the game takes an item slot of category 9 (`AL_GetNewItemSaveInfo`,
0xC1FC), creates the hat as a loose object at the Chao's position
(`ALO_ObakeHeadCreate`, 0x2E3A4) with a random facing
(`(int)(182.04445 * 360 * 3.0517578e-5 * rand())`), and clears the headgear.
The server has no native Chao throwing yet, so this waits on that port.
