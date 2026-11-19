# Native Chao contact-shadow presentation

SA2 PC contains a native Chao shadow system. Its original archive is
`resource/gd_PC/al_mm_kage.prs`, SHA-256
`d432d417dc07c60ec497ca9c0d4d5f8953b2a0c7957c5a945be329e9dc5139bb`.
The existing Rust SA2 reader extracts its single `kyotu32_marukage` GVR and
decodes the base mip to a 32×32 RGBA PNG. The native RGB5A3 pixels have a
black interior with graduated edge alpha. White RGB texels have zero alpha;
do not flatten this image against a background or discard its alpha.
The PNG hash is
`f40f1132b27a6e01d5ed8a52453798aec52764eca6da22ab42c95ce849fe430e`.
Native inputs, converted images and detailed manifests remain ignored/local.

SAMT's `include/samt/sonic/chao/al_draw.h` at Render Fix commit
`74b8ae8543009501110bc39caca74973b0526c46` identifies original PC routines:

| Routine/data | Address |
| --- | --- |
| AL_CreateShadowTex | 0x00540F70 |
| AL_EntryShadowTex | 0x00540FD0 |
| AL_AllocateShadowTex | 0x00541020 |
| AL_ExecShadowTex | 0x005412E0 |
| AL_DisplayShadowTex | 0x00541370 |
| AL_DestructShadowTex | 0x00541250 |
| texlist_al_mm_kage | 0x01349ED4 |
| ShadowTexTp | 0x01A5D144 |

The installed PC executable's code section is packed. Direct on-disk
disassembly does not establish those routines' internals. Their exact original
projection, part scales, opacity/blend setup and timing remain untraced.

The original Dreamcast game uses a different modifier-volume renderer:
Exant64/sa2dc commit `ba180d99e14936b7eaacabb62fcfb348dc038ca5`,
`src/Chao/al_draw.c:162–254`, contains native `AL_ShadowDraw`. It draws separate
head/body/hands/feet/icon volumes with distance gates and ghost-body handling.
Render Fix's `rf_shadow/chs_chaoworld.c` restores that path while explicitly
disabling the PC texture manager and `AL_DrawChaoShadow` at 0x005405D0.
The local SA Tools `ChaoCommon.ini` calls the leftover PC modifier model
at offset F49D9C **unused**. Do not present a mod/restored Dreamcast renderer
as the retail PC texture system.

## Browser adapter

`src/chao-shadows.js` uses the exact original PC texture, depth testing and
alpha blending. It clips the texture footprint onto actual receiving terrain
triangles, retaining their slope and gaps. A spatial index is built once per
static garden; moving Chao query only nearby cells. The adapter creates no
handmade ellipse/sprite and shows no substitute when the texture/floor is absent.
It excludes vertical walls, known NoShadow surfaces and surfaces outside the
configured ground-height range. Rebuild its terrain index after garden world
transforms or receiving geometry change.

This is **native artwork with adapted browser projection**, not a full port
of `AL_*ShadowTex`. Footprint dimensions (suggested Chao-scale 0.35×0.25 world
units), the default 0.32 opacity, 0.001 depth lift and height/normal thresholds
are explicit browser tuning. The original texture's RGBA data stays intact.
The sun's separate Babylon ShadowGenerator is unrelated to native parity.

```js
const shadow = createNativeChaoShadow(scene, { textureUrl, terrainMeshes });
shadow.update({ center: actor.position, width: .35, depth: .25, groundY });
shadow.setTerrainMeshes(newTerrainMeshes); // after final world transforms
shadow.dispose();
```

For multiple avatars, create one `createNativeShadowTerrainIndex(meshes)` per
zone, pass it as `terrainIndex` to every shadow and use
`setTerrainIndex(sharedIndex)` after a zone change. Passing an absent/invalid
index to that setter clears receiving geometry and hides the patch on update.

Scene integration shares one index per zone, rebuilds it after native world
placement, updates Chao footprints, and disposes their resources with actors.
The original Chao/Neutral/Hero/Dark Chaos assets receive it; separate Tails,
Knuckles and Amy character assets are outside this creation predicate. Actual
Neutral Garden browser inspection reports a visible four-triangle patch under
the Chao and a shadow throughout the slope pass, with no visible white quad.

The exact `/api/assets/chao-shadow.png` Rust and retained Node allowlists have
focused GET/HEAD/MIME/missing/private-path tests. The running frozen Rust
binary is unchanged; the loopback Vite preview serves only this one ignored
PNG alias through an exact GET/HEAD bridge. Production requires the matching
source route and local asset import; missing inputs show no shadow. Nothing
changes gameplay, server state, saves, or opaque Chao materials.

## Reproduce texture evidence

```powershell
./scripts/build-native-chao-shadow.ps1 `
  -GameRoot 'C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2' `
  -InspectorExe .local/worktrees/sa2-assets/asset-tools/crates/sa2/target/debug/sa2-assets.exe `
  -OutputDirectory .local/reviews/native-chao-shadow-new-proof
```

Choose a new output directory. This only orchestrates the existing Rust
PRS/GVM/GVR reader; it does not install assets or start/restart a service.
Archive, GVR and PNG manifests preserve source spans and hashes.
Initial evidence is in `.local/reviews/native-chao-shadow-20261118/`.
