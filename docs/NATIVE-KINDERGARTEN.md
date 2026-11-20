# Native Kindergarten and Black Market slice

Audit date: 2026-11-18. All nine original area landtables are available locally;
this is not a proposal to recreate rooms with primitive geometry. The current
Rust inspector reads every landtable descriptor and the native Basic collision
data. Existing SAIO 2.2.2 imports every room's actual visual geometry. The Rust
reader does **not** yet decode Battle GC render attachments into geometry.

The bounded next slice is **native Kindergarten Exterior + Commons + the Black
Market room**, followed by the other six existing interiors. Prepare reviewed
owner-local render assets first; coordinate real zone/room contracts and native
collision before enabling traversal or transactions. Room visuals are feasible
now, while a Rust GC visual converter and the original room/door behavior remain
separate work. No Kindergarten asset alias, service route, zone or runtime
rollout was installed by this audit.

## Exact sources and provenance

The nine intermediates below were extracted from the installed Steam PC
`resource/gd_PC/DLL/Win32/Data_DLL.dll` using existing SA Tools 786
`GameConfig/PC_SA2/Chao_DLL.ini`. The extraction report records unchanged DLL
SHA-256 `6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59`.
Each intermediate was separately fingerprinted by the Rust inspector manifest.

The common local source prefix is
`.local/extracted/pc-chao/Chao/stg_kinder/`:

| Area | Exact relative landtable | Intermediate SHA-256 |
| --- | --- | --- |
| Exterior | `Exterior/objLandTableKinderHo.sa2blvl` | `619c9d7bc20db7a0f3d6b75b2f07c7e98bce9b2b755ccdda915028cb58e13ae6` |
| Commons | `Commons/objLandTableKinderCo.sa2blvl` | `83c5ae1e5c21725d4c27265f7cda79433f5b35228a085cfa8a923c08920a9cea` |
| Black Market | `Black Market/objLandTableKinderBl.sa2blvl` | `7061485da68f383c0a3d1d42be60e6c98a54a6c3ca058f7f69d2c9f8adf7ea6b` |
| Classroom | `Classroom/objLandTableKinderCl.sa2lvl` | `48843c9b7c5b32115bf3f2e72fba3fcf205b1fc9092a1a5db5867e8b363fd1c1` |
| Health Center | `Doctor's Office/objLandTableKinderHe.sa2blvl` | `44250a0ff5a605b5e60755a98c0a27fefcb8d8dda0d80a2467b0b247870466ac` |
| Principal | `Principal's Office/objLandTableKinderPr.sa2blvl` | `0f1cb03415969d9315719e9c0a1011429e3dd0768712dc5a764f8023dfec59ee` |
| Fortune Teller | `Fortune Teller/objLandTableKinderFo.sa2blvl` | `5a276161e95b3a12c2aa3738141bae22f8287ea12740434dab3406b0210b2db4` |
| Library | `Library/objLandTableKinderLi.sa2lvl` | `853bd1e84fcc4eed94b181816da4c83f2fa46c156ebc30e496829828a8d1ec21` |
| Playground | `Playground/objLandTableKinderPl.sa2lvl` | `3a8c3e2e37f0b708d91ac3d166ab124d347a8139331abe40fa2c5c992676dd82` |

The native landtable texture names agree with the independent editor mappings
in `GameConfig/PC_SA2/sa2lvl.ini:929–972`. These exact installed archives live
under `resource/gd_PC/`; the Rust extractor decoded all nine and the Rust pixel
decoder produced **365/365** PNGs without failures. Texture indices referenced
by every inspected room material are within its corresponding archive.

| Area | Texture archive | Entries | Installed archive SHA-256 |
| --- | --- | ---: | --- |
| Exterior | `al_stg_kinder_ho_tex.prs` | 43 | `f1be8027d8e634af9b971b4278826157c7b206906879c085b19b33f68b6a59fe` |
| Commons | `al_stg_kinder_co_tex.prs` | 44 | `6da5ab71a83661b43e4476878b393cbb6f57ac6665c1a58909737cc31178d829` |
| Black Market | `al_stg_kinder_bl_tex.prs` | 45 | `a4e392c2f6f63136441be75cabf1ff2a8040ee257435df38933ce37dfb70b27d` |
| Classroom | `al_stg_kinder_cl_tex.prs` | 25 | `26aeea6ead00bf488ce6f98e9301ae8a975b9f592bd74612e47022ae6579649a` |
| Health Center | `al_stg_kinder_he_tex.prs` | 35 | `24168ada69d6ad45c80f51f6373750a142a055533a2ba461acf98ed0b7407aa2` |
| Principal | `al_stg_kinder_pr_tex.prs` | 46 | `b3e0fc8e3908f8100ce2a2579ed70efa64634b5f553c4295003b0a6af66371e4` |
| Fortune Teller | `al_stg_kinder_fo_tex.prs` | 48 | `4dce1636e8c03a2bc0d9f0eab91dfd892176cef5b526bb720d0bcdc937bc8602` |
| Library | `al_stg_kinder_li_tex.prs` | 46 | `3f1caa14797b7a58b8567cd2d0f7333feb774fad6d73a69b50abe238784e4fce` |
| Playground | `al_stg_kinder_pl_tex.prs` | 33 | `d97d8bca4e51643785d0f55eb1081a17faf0e1386a96570568e27c4957795d6e` |

The additional object/NPC/door archive is `al_stg_kinder_obj_tex.prs`, SHA-256
`e7db7714edbcbaf9c951d3cb32e529ec5ad804b63a5bc609e077d72da3f5b85e`.
It successfully extracts to 100 original GVR entries. Its pixels were not part
of the 365 room-texture decode count above. Other installed candidates include
`AL_STG_KINDER_TEX.prs`, advertisement, pill and stage-title packs; do not merge
their texture indices into a room's independent texture list.

## Actual Rust inspection results

Existing local inspector:
`.local/worktrees/sa2-assets/asset-tools/crates/sa2/target/debug/sa2-assets.exe`.
Audited binary SHA-256:
`d4e649ec42d95e97b30ade72e9183bfb211a1f3984638f33d16c1909322410dd`.
Its source is an isolated, separately tracked asset-tool worktree; this audit
does not imply that those readers have been published into the main client.

```powershell
$inspect = '.local/worktrees/sa2-assets/asset-tools/crates/sa2/target/debug/sa2-assets.exe'
$review = '.local/reviews/kindergarten-new-proof'
New-Item -ItemType Directory -Path $review
& $inspect inspect '.local/extracted/pc-chao/Chao/stg_kinder/Commons/objLandTableKinderCo.sa2blvl' "$review/commons.json"
& $inspect inspect '.local/extracted/pc-chao/Chao/stg_kinder/Black Market/objLandTableKinderBl.sa2blvl' "$review/black-market.json"
& $inspect extract 'C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2/resource/gd_PC/al_stg_kinder_bl_tex.prs' "$review/black-market-textures"
& $inspect texture "$review/black-market-textures/00000.gvr" "$review/black-market-texture0.png"
```

Choose new output paths; the tool refuses replacement. Every inspect/texture
output has a source/output manifest. Extraction preserves archive entry order
using numeric filenames, with original names and spans in `manifest.json`.
There is no new reader or hand-written game-format decoder in this audit.

| Area | Header format | Entries / visual entries | Native Basic collision entries | Unique Basic attachments / vertices / polygons |
| --- | --- | --- | ---: | --- |
| Exterior | SA2 Battle | 57 / 54 | 3 | 3 / 24 / 3 |
| Commons | SA2 Battle | 21 / 19 | 2 | 2 / 96 / 14 |
| Black Market | SA2 Battle | 18 / 18 | 0 | 0 / 0 / 0 |
| Classroom | SA2 Battle | 23 / 16 | 7 | 6 / 64 / 24 |
| Health Center | SA2 Battle | 18 / 18 | 0 | 0 / 0 / 0 |
| Principal | SA2 Battle | 17 / 17 | 0 | 0 / 0 / 0 |
| Fortune Teller | SA2 Battle | 17 / 17 | 0 | 0 / 0 / 0 |
| Library | SA2 | 19 / 19 | 0 | 0 / 0 / 0 |
| Playground | SA2 | 30 / 23 | 7 | 6 / 205 / 51 |

The Classroom filename ends `.sa2lvl`, but its actual header is Battle. Format
must be selected from the header. Library has 19 decoded Chunk attachments;
Playground has 20 unique Chunk attachments. The other visual attachments are
reported `gc-opaque` by the existing Rust reader, not triangulated geometry.

## Geometry, scale and materials

Independent existing SAIO/Blender imports expose the complete original visual
landtables. Bounds below are original native XYZ, before any recentering; mesh,
vertex and triangle-equivalent counts exclude collision-only entries.

| Area | Visual meshes / vertices / triangles | Native min XYZ | Native max XYZ |
| --- | --- | --- | --- |
| Exterior | 54 / 3428 / 3534 | `[-657.812, -2, -390.500]` | `[657.812, 448, 302.500]` |
| Commons | 19 / 2043 / 2039 | `[-97.299, -0.383, -50.652]` | `[95.855, 40, 95.501]` |
| Black Market | 18 / 1569 / 1321 | `[-8.200, -0.900, -17.487]` | `[8.200, 16.509, 5.766]` |
| Classroom | 16 / 2187 / 2260 | `[-27.057, 0, -31.250]` | `[27.011, 28.831, 50.129]` |
| Health Center | 18 / 1610 / 1884 | `[-20.375, -0.007, -31.336]` | `[20.375, 23.080, 35]` |
| Principal | 17 / 1841 / 1879 | `[-23.375, 0, -31.256]` | `[23.375, 23.050, 35]` |
| Fortune Teller | 17 / 2273 / 1774 | `[-18.358, 0, -26.371]` | `[18.358, 20.214, 35]` |
| Library | 19 / 2372 / 2333 | `[-23.381, 0, -29.098]` | `[23.375, 23.050, 35]` |
| Playground | 23 / 1457 / 1900 | `[-260, -15.637, -247.275]` | `[260, 199.699, 247.275]` |

The established browser convention is uniform scale `.1`, glTF native XYZ with
Babylon's X reflection: actor-local `(x,y,z) = (-nativeX,nativeY,nativeZ)*.1`
before an explicitly recorded area translation. Do not normalize all rooms by
their decorative/sky bounds. Exterior visual width is about 131.56 browser
units, while its native solid corridor is only about 4.89 units wide. Its solid
Z range reaches approximately `[-6.244,29.854]`, beyond the current fixed
terrain-build Z maximum 22. Commons solid bounds become approximately
`X[-8.672,8.672],Y[0,3.2],Z[-1.846,8.454]` with zero translation.

Most Battle room materials are double-sided and unlit/baked. The Black Market
import includes 17 alpha material definitions and two additive definitions.
Preserve native diffuse RGBA, vertex colors, Src/InvSrc versus additive blend,
depth-write rules, texture index, filtering and U/V clamp/repeat flags. Do not
run these rooms through the current generic `scene.js` fallback that changes
BLEND materials into alpha-tested cutouts. Library/Playground also have distinct
lit versus unlit source material flags; a blanket stage override loses data.

Existing `scripts/convert-assets.py` and its SAIO import/export orchestration
demonstrate the owner-local native geometry path. They currently enumerate
only the three garden landtables. A bounded new orchestration script could
prepare the real rooms using the extracted original PNGs and preserve their
material metadata in glTF extras. That is a reference-tool bridge, **not** a
completed Rust GC converter or native-engine port. Production Rust conversion
must implement/validate the currently opaque GC attachments against that
independent reference; no procedural substitute resolves this gap.

## Entrances, NPCs and room behavior

The installed executable has separate mapped native objects. Existing SA Tools
786 `GameConfig/PC_SA2/ChaoStgKinder.ini` declares `datafile=sonic2app.exe` and
maps the following actual assets. These do not come from the Data DLL's nine
static landtables:

- `Commons/models/GC/BlackMarketEntrance.sa2bmdl` and matching `.saanim`.
- `Commons/models/GC/ClassroomDoor.sa2bmdl` and matching `.saanim`.
- `Commons/models/GC/ExitDoor.sa2bmdl` and matching `.saanim`.
- `Commons/models/GC/HealthCenterDoor.sa2bmdl` and matching `.saanim`.
- `Commons/models/GC/PrincipalsRoomDoor.sa2bmdl` and matching `.saanim`.
- `Commons/models/GC/FortunetellerDoor.sa2bmdl` and matching `.saanim`.
- `Commons/models/GC/ExteriorDoor.sa2bmdl` and matching `.saanim`.
- Black Market `Merchant.sa2bmdl`, `HalfFish.sa2bmdl`, three motions each,
  lantern/motion and original theme-item preview models.
- Principal, Doctor, Fortune Teller and Librarian candidates, doctor pills,
  native default object lights and fortune-teller shape motions.

All 55 mapped files were extracted into a **new ignored research directory**
using the existing mapper. Original executable SHA-256 remained
`4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16`
before/after extraction. Door geometry/motion availability does not identify
placed trigger volumes, authoritative spawn points, transition eligibility,
NPC placement, lesson timers or original menu/camera behavior. Those runtime
tables/callbacks still require tracing. A room with no Solid-flag landtable
entries must not be declared walkable from its decorative floor geometry.

## Collision and integration gates

Native collision entries retain these original masks:

- Exterior: `0x40000001`, `0x20000001`, `0x60000101`.
- Commons: two `0x20000081` entries.
- Classroom: seven `0x60000001` entries, with shared attachments.
- Playground: six `0x60000001` and one `0x40000001`.
- Black Market, Principal, Health Center, Fortune Teller and Library: visual
  `0x80000000` entries only; no separate native solid landtable entries.

`Solid=0x1`, `Unclimbable=0x80`, `Stairs=0x100`, `CannotLand=0x1000` and
`Visible=0x80000000` are declared in the local native `SA2Enums.h`. The high
`0x20000000/0x40000000` bits remain named Unknown1/Unknown2 there; preserve
them. In particular, Commons `0x80` is **Unclimbable**, not CannotLand.

The Rust stage reader preserves Basic vertex/polygon winding, flags and node
transforms, but explicitly claims no transformed-world collision solver or
navigation parity. Retain every source transform and distinguish upward floor,
wall, doorway and decorative surfaces. Existing `build-terrain.js` has a fixed
grid and visual-mesh sampling; it is not sufficient evidence for these native
room collision/transition contracts.

As audited, `src/world-layout.js` registers only the three gardens, and
`server/src/game.rs` has `TERRAIN_ZONES=[hub,garden,hero,dark]`. The explicit
asset allowlist in `server/src/service.rs:415` contains no Kindergarten rooms.
Adding new GLB files cannot make them served or canonical zones. Do not reuse
the `hub` alias to conceal an unreviewed room rollout.

The next acceptance sequence is:

1. Prepare original Exterior/Commons/Black Market GLBs in a new ignored
   directory; preserve native geometry/material/source receipts. Compare
   geometry, original texture pixels, alpha and actual loaded PBR state.
2. Include original door/NPC objects from the executable mapping. Trace their
   placed transforms, room cameras and native transition/spawn tables. Keep
   menu-style room presentation distinct from newly proposed free walking.
3. Agree Rust room/zone, collision and transition contracts through the server
   coordination board. Implement the native Basic world-transform path and
   explicit domains/spawns before traversal. Keep unidentified menu-room
   movement disabled rather than inventing a floor or teleport contract.
4. Add explicit asset keys/material presentation adapters in the client and
   explicit allowlisted routes in the separate Rust server. Validate a real
   disposable canonical enter/exit round trip before any rollout.
5. Add Black Market buy/sell/stock/refresh/unlock behavior to the authoritative
   Rust economy using traced tables and replay-safe transactions. Current
   Amy's four-item shop remains a custom prototype, not original market parity.

Track mechanics and acceptance against [MECHANICS.md](MECHANICS.md),
[DESIGN.md](DESIGN.md), [WORKSTREAMS.md](WORKSTREAMS.md), [TASKS.md](../TASKS.md)
and the separately tracked server's `docs/coordination/BOARD.md`/`PROTOCOL.md`.
Source preparation/publication does not authorize a live server rollout.

## Local evidence

Everything derived from the installed game remains under the ignored
`.local/reviews/kindergarten-native-slice/` directory:

- `objLandTableKinder*.json` plus Rust source/output `.manifest.json` receipts.
- `saio-scene-inspection.json`: independent original visual bounds, geometry
  counts, native material definitions and separate solid mesh bounds.
- `textures/*/manifest.json`, original numbered GVR entries and room PNGs with
  per-texture Rust manifests; `texture-decode-results.json` records 365 passed.
- `objects/`: 55 original mapped executable objects/motions/lights;
  `object-extraction-proof.json` records the unchanged executable hash.

No native outputs are tracked or published. No existing game file, runtime,
server, credentials, canonical Chao or save was modified. This audit proves
source availability and bounded inspection/reference import; it does not claim
completed room gameplay, production conversion or service integration.
