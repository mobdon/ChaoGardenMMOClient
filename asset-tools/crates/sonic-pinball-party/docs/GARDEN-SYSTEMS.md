# A3VE Tiny Garden systems map

The native route is mapped below; **the all-system end-to-end requirement has
not passed**. The strict audit attempts each original entry path. Traced source, a Rust port, native instruction
parity, and service integration are different evidence levels. No asset or native
save bytes are included here.

All offsets refer to the authenticated decoded A3VE payload unless marked ROM.
Execution address is `0x02000000 + offset`, with Thumb callback pointers adding
one. These are independent of the cartridge engine's offsets and of Advance
layouts. The payload has `0x2E12C` bytes, SHA-256
`33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb`.
ROM `0xD0970` stores its `0x253E8`-byte compressed stream, SHA-256
`72fd1b2645fc1358ad131a697dac1c53b4ae57f195759ff78e1535ebda70d29f`.
See [GARDEN.md](GARDEN.md) for ROM, palettes and opaque flash-record provenance.

## Implemented boundary

`garden_frame` ports input initialization `0xC6C`, input polling `0xBD0`, main
entry scheduling `0x168` and one main-loop iteration. `garden_home` now ports
boot `0xC4`, IRQ installation transactions, home task installation `0x18C8`
and authentic graphics loader `0x1C60`. Boot stops at actual flash identification
`0x12028`; VBlank, mixer and unresolved callbacks remain strict dependencies.
`garden_palette` ports the whole source-order callback `0x1D68`.

`garden_graphics` ports `0xD294` static graphics and native register/map helpers.
`garden_payload` ports the full `0xD504` board initializer and the next callback
`0xD7CC` up to its strict dependencies, plus the bounded board helpers.
`garden_services` closes the native background/task/draw/OAM services, snapshots,
queues and PRNG. `garden_care` implements every action target in the 64-state
switch, Chao initialization and movement; `garden_home_tasks` adds native part
poses, cursor/petting, HUD and object callbacks. `garden_shop` covers all three
purchase categories and inventory contact/consumption setup.

`garden_puzzle_objects` closes the ball puzzle task/rule chains, including shot
resolution, groups, collapse, result banners, ring cap and return transitions.
Joined execution has no inner function stubs: 600 home callbacks across five
languages and present/absent Chao, and 2,160 board callbacks across four rounds
and three input profiles match original instructions and all workspace hashes.
Audio is not initialized in these explicit snapshots; native song routing runs
with that same inactive state. This is callback composition, not full boot or
audible gameplay.

`garden_save` executes the original loader, initializer and rotating/retrying
writer. `garden_transfer` executes the bounded local JoyBus endpoint and UI
transaction, including receive-to-save order. `garden_audio` independently
authenticates this payload's four players and 37 songs, then ports routing and
player lifecycle. Device inputs/calls are explicit seams in their component
fixtures. `garden_runtime` joins these on one bounded memory image, rejects
unknown calls, and never invokes native instructions. The all-system strict
audit and remaining gates are recorded in [GARDEN-VALIDATION.md](GARDEN-VALIDATION.md).

## Entry, input and frame

ARM reset at `0x0` branches to `0x20`; `0x0..0xC4` contains reset and IRQ setup.
Thumb main `0x168` calls `0xC4`, installs callback `0x020018C9` at `0x03004250`
and loops at `0x178`. Each iteration waits through BIOS wrapper `0x11B04`, polls
input at `0xBD0`, invokes the current callback through `0x12498`, runs sound main
`0x10660`, increments `0x03004258` and sets `0x0300425E`.

Input structure `0x03005310` holds native held, edge and repeat state.
`0xBD0` samples KEYINPUT `0x04000130`; `0xC6C` installs initial/repeat delays
15/7. The reset-button combination invokes sound/reset dependencies before
reloading current input. Frame counters do not imply trusted wall-clock time.

Garden initializer `0x18C8` loads native graphics through `0x1C60`, initializes
the linked task pool through `0xC90`, installs scene tasks, and selects frame
callback `0x02001D69`. Task base is `0x03005320`, stride `0x24`, with 70 native
free entries plus sentinels. Allocator `0xD04` links tasks; dispatcher `0xEAC`
walks the list and invokes each callback through `0x1249C`.

The literal/call pairs below establish reachability from `0x18C8`. They do not
establish complete semantic naming or Rust coverage.

| Literal | Installed entry | Observed role |
| --- | --- | --- |
| `0x1AE0` | `0x3424` | Garden object spawning |
| `0x1AF0` | `0x35CC` | Restore up to 16 objects from save coordinate pairs |
| `0x1AF8` | `0x74C0` | Two minigame icons; task `+0x1A` is 1/0 |
| `0x1B04` | `0x41E0` | Chao creation; result stored at `0x03001510` |
| `0x1B0C` | `0x89B0` | Rings/stats HUD |
| `0x1B10` | `0x83E4` | Up to three persistent objects |
| `0x1B1C` | `0x8324` | Up to eight inventory objects |
| `0x1B24` | `0x3E04` | Saved special object selected by save `+0x59` |
| `0x1B28` | `0x7500` | Shop initialization |
| `0x1B2C` | `0x8A04` | Cursor; result stored at `0x0300150C` |
| `0x1B34` | `0x218C` | Pause/save menu input |
| `0x1B38` | `0x89D4` | Communication UI task |
| `0x1B3C` | `0x88B0` | Native fade-in |

Frame callback `0x1D68` calls queued register writes `0x16D8`, OAM upload
`0x12E0`, queued background writes `0x1420`, palette updates, JoyBus maintenance
`0x590`, draw-list reset `0x1374`, task dispatch `0xEAC`, sprite ordering
`0x117C` and OAM composition `0x1088`, in that order.

## Subsystems and remaining gates

Here `save` means native RAM `0x03004AA0`, not a server snapshot or upload schema.
Names below describe observed behavior; unaudited saved bytes retain opaque
status and cannot be assigned MMO meanings by analogy.

| System | Source route | Unimplemented acceptance gate |
| --- | --- | --- |
| Chao presentation | `0x41E0` checks save `+0x58` against the absent sentinel, creates components and selects display data using save `+0x72`; it installs `0x4418`. | Audit type-to-display mapping, component descriptors, native motions and Neutral/Hero/Dark Chaos forms. |
| AI and care | `0x4418` updates native timers/needs and dispatches task `+0x17` through 64 pointers at `0x45D4`. All state targets, signed food effects, localized dialogs, numerical movement and `0x41E0` component initialization have native multi-frame parity. | Complete interactive sessions and authoritative MMO field/intent mapping. Exact semantic names are not inferred from Advance. |
| Food/items | `0x7DA0..0x800C` checks selection/contact state, reads seven-byte rows at `0x1269C`, starts a Chao action and removes an inventory entry. `0x8324` restores item positions/callbacks. | Audit each effect row and all consumption/cancel paths before assigning canonical skills or needs. |
| Shop | `0x7500` initializes; `0x7688` controls the panel. Purchases `0x7978`, `0x7B00`, `0x7C4C` compare/subtract working rings using three price categories of 7/12/3 words. Category 0 finds one of eight `0xFF` slots at save `+0x5A`; later categories mutate save `+0x59` and persistent positions. Stock/eligibility/labels, success/failure/cancel and item handoff have native parity. | Complete interactive home-to-shop-to-save session and canonical inventory integration. |
| Rings/stats | `0x89B0 -> 0x292C` animates toward working rings at `0x03006194`, mirrors them byte-swapped at save `+0x64`, and displays seven saved stat values. | Audit display order/scales and every mutation site. HUD output is not a completed economy. |
| Menus | `0x218C` reacts to Start and installs `0x21F4`; its save branch installs `0x8940`, which calls `0x3AC` before the fade route `0x882C`. | Native menu, cancellation, save failure and return sequences. |
| Minigames | `0x3A90` compares object class 4 and selects `0xA8A4` or `0xD294` using task `+0x1A`. `0xD294` loads graphics and invokes `0xD504`. | Full round controls, score/reward, return and save behavior. These are two traced A3VE branches; the five MMO games are a separate cross-source scope. |
| Saves | `0x248` loads all six sectors and rereads the selected record; `0x3AC` writes; `0x43C` initializes; `0x185C` computes CRC. Actual loader/writer/init, interrupted reads, retry rotation, fresh-RAM recovery and opaque-tail preservation match native instructions against synthetic flash. | Actual flash/power-loss behavior, a supplied real native save, full boot and canonical persistence. |
| Transfer | `0x480` initializes; `0x590` maintains; IRQ `0x6B8` has ten first-level states via `0x6E0`. `0xB0C` receives into `0x03004270`; `0xB64` sends from save `+0x58`; `0x89D4 -> 0x2EB8` handles communication UI. All are ported with bounded buffers; a complete synthetic 2,048-byte receive/save transaction matches native register order and record hash. | Physical peer exchange, interrupt timing, on-device persistence and trusted canonical import. Native JoyBus is not MMO networking. |
| Audio | `0x105E8` initializes four players from `0x19BE0`; `0x10660` invokes `0xF938`. `0x1066C/0x10698` index payload song records at `0x19C10`. | This payload's own header/track/voice/sample closure and actual audio parity; cartridge-engine offsets cannot substitute for these sources. |

Transfer success/save ordering has concrete evidence: `0x30BE..0x30CA` copies
the dynamic received bytes to save `+0x58`; `0x30DE..0x30FC` byte-swaps received
save `+0x64` into working rings; `0x3122` invokes writer `0x3AC` before interrupts
are re-enabled at `0x3126`. This does not establish packet validity, trustworthy
remote values or failure rollback. Rust transfer tooling must enforce agreed
size/version bounds before applying opaque native data.

The payload audio table has 37 eight-byte records at `0x19C10..0x19D38`.
Native `0x4C00` loads song index 36 and `0x4C02` calls `0x1066C`, establishing
that this highest structural record is used. Record 37 would overlap the first
shared zero-track header; the native lookup itself has no explicit count check.
Bounded inspection finds 30 unique headers (29 nonempty), 38 distinct track
pointers, at most five tracks per header and one referenced voice-group root
`0x1949C`. The four players each have native capacity 16. This establishes
header/pointer structure; it does not establish track control flow, instrument
indirection, sample closure or rendered audio parity.

## Static board graphics semantics

Native loader `0xD294` uses BIOS CpuFastSet (`0x11AE0`, SWI `0x0C`), so control
`0x01000200` clears `0x800` bytes at `0x0600D000`; palette counts are words,
yielding 416 BG and 224 OBJ bytes. `0x11AF4` is LZ77UnCompVram (SWI `0x12`).
Register helper `0x163C` consumes sixteen `(u16 IO offset, u16 value)` pairs at
`0x2E074`. Map helper `0x1718` truncates width/height to bytes, uses a 32-tile
row pitch, and adds the fifth argument with wrapping halfword arithmetic.

| Source | Destination | Width x height | Added value | Condition |
| --- | --- | --- | --- | --- |
| `0x2DD64` | `0x0600D030` | 6 x 5 | `0x61B6` | Always |
| `0x2DDB8` | `0x0600D172` | 3 x 9 | `0x71D6` | Always |
| `0x2DDEE` | `0x0600D430` | 6 x 4 | `0x61E1` | Always |
| `0x2DDA0` | `0x0600D030` | 6 x 2 | `0x61B6` | Normalized host-mode byte is nonzero |
| `0x2DE1E` | `0x0600D800` | 2 x 20 | `0x1` | Always |
| `0x2DE6E` | `0x0600D82C` | 8 x 20 | `0x27` | Always |

Procedural border writes retain the native last-row exception: interior rows
16..18 write one leading tile plus nineteen copies of the next tile, while row
19 repeats `0x8204` across all twenty columns. The OBJ LZ stream ends at
`0x17072`; the following two alignment bytes are excluded from its stored hash.
The loader's tail invokes `0xDB0`, starts/restarts song 4 via `0x10698`, then
enters `0xD504`. Static graphics alone do not execute that route.

## Reproduce source metadata

`tests/garden_systems_reference.py` authenticates the whole local ROM and decoded
Garden, bounds every named span, and compares hashes with the metadata-only
`garden-systems-vectors.json` (local vectors directory, see `tests/spp_vectors.py`) (`evidenceOnly: true`). It uses the independent bounded decoder in
`tests/reference_parity.py`. It neither executes native instructions nor tests
Rust gameplay. No original bytes are written or included in the fixture.

```text
python tests/garden_systems_reference.py --rom PATH_TO_LOCAL_A3VE
```

The following tables use exact end-exclusive inspection spans; some span a
named action group and its literal pools. An inspection span is not a claim
that every internal handler is implemented.

### Code inspection spans

| ID | Offset | Length | SHA-256 |
| --- | --- | --- | --- |
| `armResetAndIrq` | `0x0` | `0xC4` | `2c2a7bc57627bdec6775bc3a6524e426b15aab49aa6118f6b1cf04573f30ab6a` |
| `initialization` | `0xC4` | `0xA4` | `7b506458757d373a076b78a18dcfecb522eb8b2fdc980fb89345efa4c13f4a84` |
| `mainLoop` | `0x168` | `0x34` | `32b8a7e43b9990c6c92610cdd8a9bc3f09f43a7ad28adb5def7cbd7094007f6a` |
| `saveLoader` | `0x248` | `0x164` | `25cdd363c8dfbc6b6605bc11614c6f378c039cc52788cc4360a5bdd6941a2e92` |
| `saveWriter` | `0x3AC` | `0x90` | `febb036c5b5a31d039e3047f150466b7367ff6122d1820e8c2f0a1e18067e512` |
| `saveInitializer` | `0x43C` | `0x44` | `eed92d3d6deeac9554f08812f57b3897db3db5ccaea36b7a93606f6ef03fd027` |
| `joybusInitialize` | `0x480` | `0x110` | `95209ade487569aaaa849dffd660451f6de87f94e21b059c311466c8e70e803b` |
| `joybusMaintenance` | `0x590` | `0x128` | `87850bc1fd58007f7dde078a99194dafaf4a69b36276271d34e4b826ed9eb82f` |
| `joybusIrqGroup` | `0x6B8` | `0x518` | `65ef37578af54d815cd5c1b2c67c27788dbc40c04bd5c201eddb571dd5598d9a` |
| `input` | `0xBD0` | `0x9C` | `8bad175b1212b6b021e9d76e435b32379d0c6a87decede83bdfb2a50d4461314` |
| `inputInitialize` | `0xC6C` | `0x24` | `9dfb197990d9ffb1976f56e23382d5f24cd1dd8c0e31d96b74bd9c4675930158` |
| `taskInitialize` | `0xC90` | `0x74` | `c74c9e2cf2914b3b16a6d8418c38997cf1e5d805cf554d532e552e0108bf267a` |
| `taskAllocate` | `0xD04` | `0xAC` | `168464f8e6d170a06a4fea0aef4cb34ceadbe6a119cb48d82fb5956f1b9cad98` |
| `taskDispatch` | `0xEAC` | `0x28` | `9f2bc32c517324764cc4b4daaed554baca0b38c64317fb0225f55864713d2272` |
| `oamComposition` | `0x1088` | `0xF4` | `15820948718dfac281dfda0dc9f9dd7848e59ac9a9561e1bb5a6c086d415694d` |
| `oamUpload` | `0x12E0` | `0x18` | `9ec32d38531fc6fe89fc2eec84318af546cd642694fc584004fa1602393f8b40` |
| `queuedBgTransfer` | `0x1420` | `0xFC` | `a01b5cb53ae43c71940df3480c029d29c69d5d18ab14543b1a7b376271d29a3d` |
| `gardenInitialize` | `0x18C8` | `0x290` | `b9457f72a85b2f7bf4b3ee73dc00ae4cd94820ccf23e07176f4de3d43b6e25fc` |
| `gardenStaticLoader` | `0x1C60` | `0x108` | `37dfa53fc38f5ccf96e15354c915c13863c396aa99fbb9d1760711fd1fe1bc39` |
| `gardenFrame` | `0x1D68` | `0x98` | `925d4befceac5d71ab610b5eeb3e71ae6792fc3d532c0b205ca70af91d1ce467` |
| `pauseAndSaveUi` | `0x218C` | `0x1D8` | `1f985cc100c67c977961a6e1f266614d6b4b75c6e7472eb0565e283e8a04318c` |
| `ringsAndStatsHud` | `0x292C` | `0x468` | `a3dd758a4122d631d495ac957cee49dde3238f429d7b938ff1f9678e5ff35014` |
| `communicationTaskGroup` | `0x2EB8` | `0x56C` | `46c867596a4d5ee4d32b5c3c5c16957a77682608e5209654ca54f27add33d342` |
| `objectSpawner` | `0x3424` | `0x110` | `85a58d91e775536e1780c2e3d27a6ba6ec2f85ebfe2d659807d2a42a17b859d9` |
| `chaoInitialize` | `0x41E0` | `0x238` | `91217ee971fc7ce7287a3a6fabeef7b3e0533b104c004cb1efe9aa37369bc738` |
| `chaoFrameAndActions` | `0x4418` | `0x1A40` | `b9f190d0fe89d2b29a1b060e4f5f1592e5ee9e8792f3268a21e6dc00c0e937ed` |
| `highestSongCallWitness` | `0x4BC6` | `0x40` | `1079294c88651825c0001840c6e128f2959580ae41e1322f3c0f618163d1fe47` |
| `chaoMovement` | `0x5E58` | `0xA4` | `d6c301969987ea3835d362dfe5f3684fd0865bd3f2f22b9b5bd0cd051862989a` |
| `chaoStateSelection` | `0x5EFC` | `0x674` | `36589100c780da5de31ecb769b05f3e001a93e0735bb2e68195018c700dfa39a` |
| `shopInitialize` | `0x7500` | `0x188` | `77ade55b6cf80de42372f58b0b346820231b8d2258be784ce83b40f5dd5a3f2a` |
| `shopPanel` | `0x7688` | `0x2F0` | `ed2f913ffbd94f64d8cfffac1f332234c6c7cc28c0a04d26ca90e34ecec2ddea` |
| `purchaseCategory0` | `0x7978` | `0x188` | `274fbb2bb3ccf9210f5c331ddae6140e9c03e1dc644bfed072b54bd25cc93e3d` |
| `purchaseCategory1` | `0x7B00` | `0x14C` | `cdbb7a30955be24878485c2f4c5a51ce481625d402b0ad4c63a27041dfd7ee01` |
| `purchaseCategory2` | `0x7C4C` | `0x154` | `320037ec695af04add11623d4ed5dcb7e9a06c8f8729617827bc6a7ac9abd05e` |
| `foodInteraction` | `0x7DA0` | `0x26C` | `4a698278ded4621c62540cc9eaa47fbd9b11d14dfd17693d1bb73d0ea2a6e7b5` |
| `otherItemInteractions` | `0x800C` | `0x318` | `4d9d693a8735ba2370ba89ed7cd16c2c388bde041cbdbe96f65e79e322657845` |
| `inventoryItemInitialize` | `0x8324` | `0xC0` | `0098d3a90f509ed9b57e997384b8d2d4b37ac358868727d97c220f7eb0efd53c` |
| `soundInitialize` | `0x105E8` | `0x78` | `d30e3b7303bcedf45aad2972b163beb6f491ee3d33c24a065c62b4bbdf51b8e1` |
| `soundMain` | `0x10660` | `0xC` | `be97755229b87ccfc5d520e15242c95995c59ad569dce54f4e9eb0fbc9c3bb5d` |
| `songStart` | `0x1066C` | `0x2C` | `12628e8926a91827d7b875c4d73b42c0407510c64dbe55f0bc70f6e500d2907b` |
| `songStartOrRestart` | `0x10698` | `0x4C` | `20a1d66edbace4b6d7be82cc8478c08718210f043c2f36b56e9f36e710fb0219` |

### Data spans

| ID | Offset | Length | SHA-256 |
| --- | --- | --- | --- |
| `chaoStatePointers` | `0x45D4` | `0x100` | `bc28317d2050abd69dd85beed9824cc92bedd072c1eddb4e10a2040d9472bd6e` |
| `foodEffects` | `0x1269C` | `0x31` | `b33ae558a366ac031e7326d7ade4d3a38f3818f21c6139f6a97a8ceeaa3c49bd` |
| `shopCategoryOrder` | `0x126CD` | `0x7` | `57355ac3303c148f11aef7cb179456b9232cde33a818dfda2c2fcb9325749a6b` |
| `shopPrices0` | `0x126D4` | `0x1C` | `e9eaed988f3156a9b8c28f72dc48adac7c1735915afd280c7a2406529cc2dc19` |
| `shopPrices1` | `0x126F0` | `0x30` | `53168556e91578a45777528ddd9f9ce8ed0ae63bbdfc0afb8850ca8fdc055b2d` |
| `shopPrices2` | `0x12720` | `0xC` | `0935002e4c7afc324f5aca05efcb6b3d674834a5cd1e6391f2fdf7e66c940909` |
| `soundPlayers` | `0x19BE0` | `0x30` | `376f11609385beff83adf58f0e4c8908ae4f3f11b201ee46ec828d02adba8d7a` |
| `soundSongRecords` | `0x19C10` | `0x128` | `4daf243e3b087f32dd522837437f589adec8cb507706ab61b3a0342d6f0df812` |
| `ballMap0` | `0x2DD64` | `0x3C` | `063a5c750a0de44d16cd341810bc6739f6691770627d7cc806b4c301c3c4e1e9` |
| `ballMap1` | `0x2DDA0` | `0x18` | `b68b2112574772105da1fa222b10c1f5b5be3e911aef8bb45e6b10e39132c465` |
| `ballMap2` | `0x2DDB8` | `0x36` | `c8334e27ff972e99363acfda5aa0bdad9b3f14a698d6d0bf216f292c5a6cc880` |
| `ballMap3` | `0x2DDEE` | `0x30` | `762fc0d0f51e603198a2de182b59cb94c935a6537feaad1ed071f2e88304e99e` |
| `ballMap4` | `0x2DE1E` | `0x50` | `40c615278c72c84cf1b8b525deb1127057f2bcc36dda9d01d341efaa5f8c1487` |
| `ballMap5` | `0x2DE6E` | `0x140` | `ff1cb604b6ef3f74c46e87591fb67b6222cc1fc8d2297672bc0b0f47a12304ff` |
| `ballRegisters` | `0x2E074` | `0x40` | `806901e0ce2ef77e5e3e24b880f1048322c994371a61005195170af556fbed4a` |
| `ballBgPalette` | `0x17074` | `0x1A0` | `1dfb6add24d0c66b29e5b5a27d9cf88599017a198495e5d7168ac6c873f6d33a` |
| `ballObjPalette` | `0x1606C` | `0xE0` | `15927abd5d354edf7c87c1282f4b55c124430d985bf935c5945453181a7331d9` |

### Compressed board resources

| ID | Offset | Stored bytes | Stored SHA-256 | Decoded bytes | Decoded SHA-256 |
| --- | --- | ---: | --- | ---: | --- |
| `ballBgTiles` | `0x17214` | 8109 | `d3a6c072ea314f12db73fac21ab64048dc1f50a8af1ba9e666dde3a8c5576187` | 17760 | `9c0387ae7e61eb585ac7d78bcf138bcae5684dae9097ab6d8fba9b037440c56b` |
| `ballObjTiles` | `0x1614C` | 3878 | `c187d348641b37fdf9afcbf067d32c1330cd8efcbe47749d781fec7bab4798d0` | 10848 | `7fe825cbf93cad4f3344c14b80e33188947c2d49d9995e62608bb3cf14ccf0b9` |

## End-to-end and service acceptance

A native Garden run must execute entry, input/frame/task/render/audio
dependencies, Chao actions, item/shop mutations, minigame completion, save and
return, with source-backed boundaries and bounded memory. Compare multi-frame
inputs and failure paths against real native execution. A rejected unported
dependency is an explicit limitation; a no-op backend does not complete it.

The canonical Chao, identity, trusted time, rings, care, inventory, progression,
training, rewards and saves remain in the separately agreed Rust service.
Native import is an isolated local path, never authority to upload claimed
gains. Integration still requires once-only transactions/receipts and an actual
canonical service round trip. No full Garden or canonical-service end-to-end
acceptance is claimed by this map or its metadata fixture.
