# Original animals, Chaos Drives and care

Updated November 18, 2026. This audit supports mission pickups → carried animals/drives → care in the garden. It identifies original Steam data and concrete implementation boundaries. Extraction does not yet establish the complete original item-use formula, behavioral learning or Chaos Chao evolution rules.

## Reproduce the local audit

With the Steam game and existing local SA Tools 786 setup available, run:

```powershell
node scripts/extract-animal-care.js
node scripts/verify-animal-care.js
```

An optional game-directory argument follows the script name. `SA_TOOLS_DIR` can point to an existing SA Tools setup. The extractor reads installed files and writes only under ignored `.local/`. It does not modify the installation or a Chao save. Keep all extracted assets, textures, stat tables, tool binaries and generated reports local.

| Local output | Contents |
| --- | --- |
| `.local/care/animals.json` | Native animal/drive IDs, signed stat fields, model/motion references, species bits, part-list mappings and explicit gaps. |
| `.local/care/verification-report.json` | Source hashes, independent table comparisons, resource checks and remaining mapping gaps. |
| `.local/extracted/care/manifest.json` | Original-file provenance and resource counts. |
| `.local/extracted/care/Chao/` | Original garden animals, their mapped motions, body-part resources and stat INIs. |
| `.local/extracted/care/OBJECT/` | Four original drive models and their texture-list resource. |
| `.local/extracted/textures/` | Decoded `al_minimal_tex`, `al_mini_parts_tex` and `chaosdrive_tex` packs. |

Verification on the local installation passed: **25 original files unchanged; 21 animal models; 67 animal motion sections; 247 part model sections; four drive models; 26 animal/drive stat rows and 24 fruit stat rows**. All 50 rows agree with a separate SA Tools extraction. There are **342 extracted resources** overall. No animal GLB or browser play acceptance is implied by these counts.

## Native identity and resources

The primary [SA2 Mod Loader enum](https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Enums.h) gives the following native species IDs. Preserve these IDs across collection, history and appearance. A species-history bit is `1 << id`; it is distinct from possession of an animal item and from the species currently shown in a body-part slot.

| IDs | Species in native order |
| --- | --- |
| 0–2 | Penguin, Seal, Otter |
| 3–5 | Rabbit, Cheetah, Warthog |
| 6–8 | Bear, Tiger, Gorilla |
| 9–11 | Peacock, Parrot, Condor |
| 12–14 | Skunk, Sheep, Raccoon |
| 15–17 | Half Fish, Skeleton Dog, Bat |
| 18–20 | Dragon, Unicorn, Phoenix |
| 21–24 | Yellow/Swim, Green/Run, Red/Power, Purple/Fly Chaos Drives |

The official [SA Tools PC mapping](https://github.com/X-Hax/sa_tools/blob/master/GameConfig/PC_SA2/ChaoCommon.ini) supplies garden models and animation names. Every animal-model pointer in the original executable's 21-entry array at `0x012966F0` joins to its mapped garden model. Stage `minimal_*.prs` resources are independently hashed inventory candidates; this audit does **not** claim their stage drop/spawn rules or that the garden model was extracted from those archives. Drive models and their texture list come from `PC_SA2/CommonObjects.ini`.

## Stats are several separate systems

`ChaoItemStats` is ten signed shorts, 20 bytes per row: Mood, Belly, Swim, Fly, Run, Power, Stamina, Luck, Intelligence and an unresolved final short. The native animal/drive array starts at `0x008A6240`. Rows 0–20 are the species, 21–24 are drives, and row 25 remains **reserved/unidentified**. The SA Tools mapping includes it, but the animal enum does not identify a usable item there. Fruit rows are a separate 24-entry table at `0x008A6448`; they are not additional animals.

Keep negative effects. For example, the native Penguin row contains Swim `36`, Fly `12`, Run `8`, Power `-24`; it does not represent four positive bonuses. Drive rows contain `24` in their named principal ability and zero in the other three, with their own hidden-stat fields. Original table values are preserved in ignored data rather than published as a source-controlled copy of the decoded table.

These numbers are **raw item-table units**. The complete consuming path, multipliers, experience overflow, point calculation and clamping have not been validated. Adding `24` directly to a browser level, an S grade or an unrelated RPG skill would be an MMO adaptation, not an established original formula.

The [native parameter definitions](https://github.com/X-Hax/sa2-mod-loader/blob/master/SA2ModLoader/include/SA2Structs.h) keep these fields independent:

| Native state | Meaning established by the structural audit |
| --- | --- |
| `Exp[8]`, `Abl[8]`, `Lev[8]`, `Skill[8]` | Progress, grade, level and accumulated skill are separate arrays; exact application/ranges still need consuming-path validation. |
| `body.HPos`, `VPos`, `APos`, growth and target values | Run/Power, Swim/Fly, Hero/Dark and growth appearance state; not ordinary stat grades. |
| `PartsBTL.MinimalFlag` | Recorded species bitmask; reset timing and complete evolution usage remain untraced. |
| `PartsBTL.MinimalParts[8]` | Current animal appearance choices for eight part categories. |
| `emotion.Mood`, `State`, `Personality`, tastes | Needs, mood, personality and preferences; these are not a single animal-granted “trait” value. |
| `knowledge` | Classroom/art/song/music/toy and social knowledge structures; do not infer all animal behaviors from lesson bits. |
| `gene`, `nbSucceed`, age/life fields | Inheritance and lifecycle state, separate from current item ownership and form selection. |

Luck and Intelligence must retain explicit units and bounds; neither is automatically an ordinary displayed S-rank grade. The owner perfect-stat privilege remains an independently authorized server rule.

## Body parts and learned behavior

The native child/adult arrays are at `0x012E59A0` and `0x012E5938`. Eight persistent categories expand to thirteen render sockets: paired arms, ears, horns, legs and wings, plus forehead, tail and tongue. The mapped SA Tools resource can wrap a live part in a parent object. The audit follows actual child/sibling pointers, so a nearby address or matching filename alone does not count as a join.

Of **245 nonzero file-backed part slots**, **244** resolve to actual object nodes in mapped resources, with no ambiguous joins. Remaining gaps stay visible:

- Half Fish, Skeleton Dog and Bat each have child/adult list pointers outside file-backed data: **six lists require runtime inspection**. This does not establish that their retail effects are absent or grant arbitrary parts.
- The mapped **Child Chao Rabbit Right Ear** resource fails native object-layout validation; its live part pointer does not resolve through the declared export. Keep this attachment unavailable until the mapping is corrected and visually verified.
- Part selection, replacement/removal probability, animation compatibility, special animal effects and item reuse timing still require original behavior tracing.

Animal-derived habits and rare-animal effects are separate research records. An animal's own walk/idle motion is not proof that the Chao learns a behavior. Do not automatically grant songs, classroom lessons, drawing, personality changes or tools from an animal-model match.

Preserve the original Chao hierarchy, proportions, rest pose and weights. Attach each approved part to its genuine node; do not stretch a Chao to fit an animal model. Recheck children, adults, alignment forms and Chaos forms separately before enabling a part across them.

## Authority and delivery order

1. **Stage pickup:** the server owns available spawn/drop IDs, world, position, pickup range and once-only claim. A cataloged stage or animal archive is not an enabled drop table.
2. **Carry:** store owned animal/drive items separately from species history and current appearance. Original stage carry limits and garden capacity remain unknown; any interim limits need explicit MMO labels.
3. **Care use:** atomically validate ownership, eligible target/location and item quantity, consume once, then apply a versioned effect with declared units and bounds. Reject client-provided gains, history bits, appearance, behavior and evolution results.
4. **Appearance and habits:** persist approved part/learned behavior changes separately and broadcast them. Cancelled visuals or reconnects cannot duplicate an item effect.
5. **Evolution:** connect verified care history to lifecycle/alignment rules only after native conditions are traced. Species collection alone must not directly change the player into a Chaos Chao.

Before gameplay is accepted, prove collection → carry → care → signed effects → save/reload with two clients; reject forged ownership, remote pickups, duplicate claims, negative inventory, forged stat deltas and replayed consumption. Visual acceptance additionally requires real native part/motion inspection. This audit does not mark those integration checks complete.

## Source and version limits

The audit uses the installed Steam executable, official SA Tools 786 PC mappings, SA2 Mod Loader structures/enums and the [CWE developer source](https://github.com/Exant64/CWE) as structural leads. Preserve the distinction between original bytes and mod code:

- CWE's 15-entry literal `AnimalStats[]` is for added SADX animals. It is **not** the original SA2 species table; original rows are read from the installed array instead.
- CWE's `AnimalInfluence[]` and drive comments are research leads for morph influence, not a validated retail magnitude/clamp formula.
- CWE replaces Bat arms, changes Raccoon parts and adds SADX parts. Those patches are not evidence that Steam vanilla behaves identically.
- CWE's optional character-Chao evolution is mod behavior. It does not prove the retail Chaos Chao condition formula.

Open native-rule work includes item application/grade curves, stage spawn/drop tables and carry limits, behavior acquisition and persistence, rare-animal handling, morph/alignment influence, animal-history reset timing, full Neutral/Hero/Dark Chaos evolution criteria and independent breeding/personality inheritance. Cross-reference `CAT-ANIMAL-*`, `CAT-GROW-*` and the ordered `ANIMAL-*` tasks rather than marking the whole animal system complete from an asset count.
