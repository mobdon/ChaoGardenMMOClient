# Chao evolution and appearance

Native port of the Chao body, evolution and face presentation. Rules come from
the owner's own SA2B GameCube disc (GSNE8P). `ChaoMain.rel` was PRS-decoded
locally (SHA-1 `e7e24b8b…`, matching the SA2B decomp configuration) and read
with the decomp's GSNE8P symbols. Addresses are `ChaoMain.rel` section offsets.
Geometry comes from the PC `Data_DLL.dll` `AL_RootObject` array. Every
game-derived file stays under ignored `.local/`.

The server owns the body state (`ChaoGardenMMOServer/src/body.rs`); the client
only presents it.

## Body families and morphs

`AL_RootObject[0..107]` is 18 families of six models in the order z, n, s, f,
r, p (SA Tools `PC_SA2/Chao_DLL.ini`). All six models in a family share one
40-node tree and identical per-attach vertex counts, which was checked on the
local extraction.

`scripts/convert-chao-evolution.py` (portable Blender + SAIO, the same setup as
`convert-assets.py`) writes 16 GLBs to `.local/imports`:

- `chao-body-al_ncz.glb`: the child. It is based on al_ncz with 17 morph targets
  in this order: n, s, f, r, p, hz, hn, hs, hf, hr, hp, dz, dn, ds, df, dr, dp.
  These cover the neutral, hero and dark child sets.
- `chao-body-al_{n,h,d}{n,s,f,r,p}z.glb`: the 15 adult families, each with the
  targets n, s, f, r, p.

Morph targets are node-local positions and normals. Native node rest offsets
differ across a family. They are stored per target on the armature node as
`extras.chaoMorph.nodeOffsets` in glTF node space, and `src/chao-body.js`
blends them on top of animation. The GLB rest pose is the native import pose,
including native node scale. Unit-only scale channels that SAIO adds are
dropped.

In 10 of 15 adult families, a preview at weight 1 per target matches the native
model rendered alone (mean pixel difference ≤ 1.2e-4). The other five (al_hsz,
al_nsz s/f/r, al_drz f) differ only in UVs, and that difference is native: the
live Chao model is a deep copy of the z model (fn_60_266B4/fn_60_27058), so the
game always draws z's strips and UVs, as the morphs here do.

Run: `blender --background --factory-startup --python scripts/convert-chao-evolution.py [-- --only al_nnz] [-- --preview]`.

### AL_Deform weights (server `BodyState::morph`)

AL_DeformObject 0x70D4 computes, per vertex and normal:
`z + g·((v/(h+v))·lerp(n,V,v) + (h/(h+v))·lerp(n,H,h) − z)`. In that formula:

- h = |HPos| and v = |VPos|, with zeros replaced by 1e-6.
- H = p if HPos ≥ 0, otherwise r. V = f if VPos ≥ 0, otherwise s.
- g = growth, which is not clamped to 1 here.

AL_DeformObjectChild 0x6328 blends the neutral set with the hero (APos ≥ 0) or
dark set by |APos|. The server publishes these as named glTF weights.

## Native rules ported on the server

| Rule | Source |
| --- | --- |
| HPos/VPos clamp ±1 (DeformLimit), APos ±1, growth 0..1.2 (GrowthLimit) | AL_ParameterAdd*, ChaoGlobal |
| Time growth per life step: child 1/60, adult 1/240 | AL_CalcParameter 0x213CC (clock in life.rs) |
| Animal capture: kinds 0–2 VPos −F, 3–5 HPos −F, 6–8 HPos +F, 9–11 VPos +F, drives 21–24 ±F/2; F = 0.1 for a child, 0.05 for an adult | ALBHV_Capture 0x33A2C |
| Fruit growth per bite: child 0.005, adult 0.00125; Hero/Dark Fruit APos ±0.15 after the last bite | AL_ParameterGrow, ALO_FruitHeldC 0x2B6A4 |
| Stat level-ups do not change the body shape | fn_60_2131C |
| Cocoon when a child reaches growth ≥ 1.0 (GrowthMayuPoint) and stands still on the ground | fn_60_1531C |
| Adult type: Normal if \|H\| < 0.5 and \|V\| < 0.5 (Chaos if all 21 animals, like > 80 and +0x8E ≥ 2); otherwise Fly, Run, Power or Swim by the diagonals; +1 hero / +2 dark if \|A\| ≥ 0.5; growth, H and V reset | fn_60_3B5C0 |

## Face (`src/chao-face.js`)

- **Eye textures:** the eye maps to an al_eye archive index (0,1,2,3,4,5,6,7,5,0,0,10,11,12). Eyes 0, 9 and 10 use the type's eye colour from AL_FaceInit 0xB088: (type−5)%3 gives neutral, hero or dark; children use al_eye01.
- **Eyelid targets (AL_FaceSetEye 0xAAC0):** eye 9 → 0x4000; eye 10, or eye 0 with default eye 10 (dark) → 13653 with slope 3640.
- **Lid motion:** 1024 per frame, slope 512 per frame. Blink every 100+400·rand (85%) or 5·rand frames, at 4096 per frame.
- **Lid drawing (al_draw_B):** node 19 gets Rz(+slope)·Rx(close), node 22 gets Rz(−slope)·Rx(close). CloseAng = blink + lid − 0x4000. Lids are not drawn when CloseAng is −0x4000.
- **Behaviour faces:** behaviour call sites are keyed by native address in the AI thread's presenter.

## Look (`src/chao-look.js`)

al_draw_B calls fn_60_52480 → fn_60_5140C for every body node except the eyes
18/21 (0x8DC8), the mouth 27 (0x8DA0) and nodes 37/39 (0x8E3C/0x8E44):

- **Shiny** (MultiNum ≠ 0): body texture × lit colour, plus chao_rare00
  (al_body 34) sphere-mapped by view normal, added weighted by its own alpha.
- **Jewel** (JewelNum > 0): the body texture is replaced by al_body[JewelNum+17]
  (gold … topaz, pearl, metal0/1, glass, golf), sphere-mapped, times white.
- **Colour** (JewelNum = 0, ColorNum > 0): gxColorTable[ColorNum−1] (data
  0x38E7D0, 13 entries in SA2BColour order) replaces the material colour.
- **Monotone** (NonTex): no texture, lit colour only. Two-tone is the default.
- Sphere map (TEXMTX9): u = 0.5 + 0.5·nx, v = 0.5 − 0.5·ny in view space.
- **Material diffuse:** AL_DeformObject blends it across the morph set with the
  same weights as the vertices but growth clamped to 1 (server
  `morph.diffuseWeights`; per-node values in `extras.chaoMorph.materialDiffuse`).

`scripts/extract-chao-look.py` checks the local `ChaoMain.rel` SHA-1, then writes
`chao-look.json` (gxColorTable, bodyTextureFiles 18–34, lookNodesExcluded,
palettes) and copies the al_body look PNGs unchanged to `chao-look-*.png`.

## Palettes (traced, not drawn yet)

ALPaletteInit loads 39 GVPs (pointer table .data 0x43CE8) into gPalette (bss
0x1D50, 39 × 48 RGBA8): 0 AL_NC00, 1 AL_NC01, 2–7 AL_HC{Z,N,S,F,R,P}, 8 AL_DC,
9–38 AL_H{N,S,F,R,P}{Z,N,S,F,R,P}. GVP: 'GVPL', u16 format 1 (RGB565), 48
entries from +0x10. The jumptable at .data 0x43DD0 gives the blend base by type:
child 0, hero normal 9, swim 15, fly 21, run 27, power 33. Other types keep
their texture's own bank (alpalSetBank, jumptable 0x43D84). Hero adults blend
slots base+0..5 (z, n, s, f, r, p) with the diffuse weights.

