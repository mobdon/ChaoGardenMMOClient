# Original Garden actor inspection

`native_garden_home_art::paint` overlays the cartridge's egg, fruit, Chao body
and neutral emotion sprite on a 240×160 picture. It consumes the verified A2NE
ROM and a native record. All tiles, colors, OBJ shapes, source offsets and
per-piece flips come from the embedded payload. No extracted artwork ships in
this crate. This read-only picture never writes the supplied save or wallet.

The record preserves ground coordinates, but contains no live Chao action,
facing, expression or orb phase. The inspection picture therefore selects
original pose 0, facing 1, and the neutral orb at its settled source anchor.
Original action 11 selects that body pose in `02006A54`; the original `02007240`
orb callback confirms the zero-bob settled point. These are explicit frozen
inspection parameters, not inferred AI or a new animation. Orb movement,
independent task tie ordering and full OAM scheduling are outside this painter.

The native egg (`02003E04`) and fruit (`02008324`) constructors clamp ground Y
to 159 and settle it upward through the original 22×20 collision table. The
Chao constructor (`020041E0`) supplies (88,104) for unset X, then applies that
same collision-map search. The report includes both stored and settled points.
The port bounds the search to the verified map and rejects an invalid position
before modifying the picture. The source routines can modify stored positions;
this inspector leaves the record unchanged.

Native loader `02001C60` installs OBJ palette `246E4`, body stream `24C24` and
items stream `268B4`. The main Garden callback `02001D68` installs Chao palettes
from `248E4` and egg palette bank 14 from `24AA4`. Fruit descriptor `205E4` uses
the original `20EFC` palette-bank row and tile `BC + 4*id`. The egg uses the same
descriptor, tile `D8` for egg 0 or `DC` otherwise, and packed layer/palette `EC`.
The two body descriptors and nine tiles per body pose preserve native
proportions. Appearances 0–11 and the cartridge's form-to-palette selection are
supported. Transferred/runtime palettes above 11 are explicitly omitted and
reported, with no replacement palette.

`tests/native_garden_home_oracle.py` runs the actual Thumb resource loaders,
constructors, palette callback and `02001088` OAM emitter in Unicorn 2.1.4.
An independent OAM rasterizer resolves native 1D OBJ tiles, flips, original
BGR555 colors and transparent index zero. The 93-case corpus covers all 12 egg
IDs and 7 fruit IDs at four source-map positions, all 12 known Chao appearances,
the normal-form mapping and unset-position behavior. All 161 native OAM pieces
and 14,284,800 RGBA output bytes match Rust. Source spans, fingerprints and
pixel sample hashes are in `catalog/native-garden-home-parity.json`.

The proof deliberately intercepts constructor follow-up AI and unrelated frame
services. Presentation BIOS copies and decompression are bounded models. It
does not establish authentic BIOS execution, cycle timing, live native Garden
gameplay, toys/weeds, hardware blending or sound. The standalone session may
compose a cursor separately. No MMO or browser rollout is implied.

For reproduction, set the local Rust toolchain, then run the Python verifier
with `--rom <A2NE> --payload <decoded Garden> --unicorn-dir <2.1.4 package>
--output-dir <new ignored .local directory>`. Run
`cargo test --test source_native_garden_home -- --include-ignored` with
`A2NE_ROM` pointing at the exact local ROM. All generated input records,
native images, traces and probe binaries remain below the new private output
directory.
