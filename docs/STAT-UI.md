# Original Chao stat UI assets

The browser can use original stat-window artwork from the owner's installed
Sonic Adventure 2. The selected PNG files below are unchanged SA Tools decodes,
copied under stable aliases. They are not painted replacements or generated
approximations.

Run from the client repository:

```sh
node scripts/extract-chao-ui.js
```

An optional first argument selects the game installation. `SA_TOOLS_DIR` selects
SA Tools; the default is the local release-786 installation. The script copies
six PRS archives into `.local/extracted/chao-ui`, runs `ArchiveTool -png` on those
copies, and verifies installed archive SHA-256 hashes before and after. It never
writes to the installed game directory.

The complete inspection/provenance record is
`.local/imports/chao-ui-provenance.json`. It records archive hashes, unchanged
source checks, archive indices, global texture IDs, PNG dimensions/hashes,
selected roles, and source pixel regions. Extracted archives and original PNGs
remain ignored local data. Neither the source archives nor provenance JSON are
public asset endpoints.

## Selected original graphics

| Asset route | Source archive / image | Archive index | Global texture ID | Size | Evidence |
| --- | --- | --- | --- | --- | --- |
| `/api/assets/chao-stat-panel.png` | `al_tex_common.prs` / `level00.png` | 2 | 200080 | 256×128 | Original blue stat-window/bubble artwork, Swim/Fly/Run/Power/Stamina, digits, Lv., LEVEL UP |
| `/api/assets/chao-stat-font.png` | `al_tex_common.prs` / `ad_22ascii.png` | 1 | 200060 | 512×256 | Native common bitmap font, including the actual grade letters |
| `/api/assets/chao-race-parameter.png` | `al_tex_ent_common_en.prs` / `add_parameter_race_en.png` | 2 | 200074 | 256×128 | English race-entry parameter labels, type/alignment names, numbers, Lv. and green meter graphics |

Archive indices are zero-based rows of ArchiveTool's `index.txt`; global texture
IDs are separate identifiers stored on those rows. Selected PNG SHA-256 values:

```text
chao-stat-panel.png     03ee230238c5c4d228ada80f1384894d99910cbf2cbb58682e6ce561d42f60e6
chao-stat-font.png      24f5fe022ceccaf520845f19b21912e511ef8b27410e763a09a5609e9d297018
chao-race-parameter.png 921efb17ea1f5b135d69c5ff137debb83777c7c7ac486ddda8da37cbf8bc7aad
```

The first image is genuinely the Chao stat/level HUD atlas. The second is a
shared native font. The third is genuinely stat-related but belongs to race
entry, not the garden stat panel. The distinction is retained in provenance.

## Pixel regions for the live browser menu

Coordinates use the original top-left origin, before CSS scaling. Clip the
original atlas with `background-position` or an image wrapper. Preserve alpha
and use integer scaling with `image-rendering: pixelated` where practical.

| Region in `chao-stat-panel.png` | x | y | width | height |
| --- | --- | --- | --- | --- |
| Blue window/frame | 0 | 0 | 165 | 76 |
| Swim | 0 | 78 | 46 | 16 |
| Fly | 49 | 78 | 29 | 17 |
| Run | 81 | 78 | 36 | 16 |
| Power | 121 | 78 | 54 | 16 |
| Stamina | 176 | 78 | 71 | 16 |
| Lv. marker | 180 | 58 | 27 | 14 |
| Three bubble decorations | 0 | 96 | 86 | 26 |

The smaller digits occupy two rows of five 16×16 cells beginning at `(166,24)`:
row one is 0–4 and row two is 5–9. The three oval shapes are bubble decorations;
their presence alone does not establish an original progress-bar behavior.
The race parameter atlas separately contains native green meter graphics.

The original common font uses 22×22 cells in 23 columns. Printable ASCII starts
with `!` (code point 33). For an ASCII character, calculate
`index = codePoint - 33`, `x = (index % 23) * 22`,
`y = floor(index / 23) * 22`. A space is blank advance. Verified grade cells:

| Grade | x | y | width | height |
| --- | --- | --- | --- | --- |
| S | 88 | 44 | 22 | 22 |
| A | 198 | 22 | 22 | 22 |
| B | 220 | 22 | 22 | 22 |
| C | 242 | 22 | 22 | 22 |
| D | 264 | 22 | 22 | 22 |
| E | 286 | 22 | 22 | 22 |

The authoritative grade mapping remains `0=E, 1=D, 2=C, 3=B, 4=A, 5=S`.
Levels, points, hidden stats, equipment and activity progress come from live
server snapshots. Additional Luck/Intelligence rows, explanations, tooltips,
responsive layout and accessible text are browser enhancements. Native label
images should have corresponding accessible text, and dynamic values must not
be baked into a static screenshot.

This work ports original graphical pieces into a functional browser menu. It
does not port the original menu engine, animation scripts, input dispatcher,
or grade/stat calculation routines.

## Other inspected archives

| Archive | PNG count | Actual contents / context |
| --- | --- | --- |
| `al_tex_common.prs` | 7 | Stat/level atlas, large common font, common icons and item-panel icons |
| `al_tex_chao.prs` | 10 | Eye textures; not stat UI |
| `al_tex_ent_common_en.prs` | 5 | English race-entry UI and parameter graphics |
| `al_tex_odekake_menu_en.prs` | 7 | Transporter/outings menu and its parameter graphics |
| `al_tex_cave_menu_en.prs` | 3 | Cave/race/karate selection menu graphics |
| `al_common_tex.prs` | 9 | Smaller common font, speech balloon and controller prompts |

All 41 decoded PNGs were inventoried. Only the three selected aliases are exposed
by the Node and Rust asset allowlists. The local inspection image
`.local/extracted/chao-ui/level00-grid.png` shows the unchanged stat atlas at
four times size with a coordinate grid for review; it is not served in the game.
