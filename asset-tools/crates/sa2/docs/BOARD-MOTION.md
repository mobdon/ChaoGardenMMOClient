City Escape supplies 24 two-part board motion defaults, slots 145 through 168.
The stage table does not override existing live slots. In the pinned SA2B
source (roeming/SA2B 874a480379260dce01e8988d6ab13c0deddcee4c),
`src/stg13_cityescape/cityescape.c` function `fn_13_116C4` registers
defaults until the -1 sentinel. The draw callback in `o_ce_board.c` selects
`action + 24`. This reader exposes those defaults; it does not predict action
transitions, animation rate, or whether a slot was replaced at runtime.

| Installed source | SHA-256 | Stage table | Slot 145 header |
| --- | --- | --- | --- |
| stg13D.rel | bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b | module 12, section 5 + 0x1FD2B0 | section 5 + 0x191148 |
| DLL/Win32/Data_DLL.dll | 6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59 | header_stg13, RVA 0x19C7258 | RVA 0x195B110 |

Rust exposes native motion/header and channel offsets as section-relative
offsets for the REL and RVAs for the DLL (preferred image base 0x10000000).
The motion representation uses version 0 for a raw NJS_MOTION with no SAANIM
wrapper. The source has three pairs of aliased slots (145/147, 146/148,
155/156). The PC reader handles zero-filled key records in the virtual tail of
.data; these have no bytes at a corresponding raw file offset. SA Tools'
GameConfig/PC_SA2/LeftoverDLL.ini also identifies header_stg13 as an animindexlist.

To reproduce the independent check, run tests/board_motion_reference.ps1
with -Tools pointing to the local SA Tools checkout and either -RelFile or
-PcDll pointing to the installed source. The script relocates REL data or
maps PE sections into memory, then reads authored tracks using the compiled
SAModel.dll (SHA-256 8b1d6d4fb14b07ed0718a608a297ccd8980c27147da292b624eaf647c74d004b).
Set SA2_SET_DIR to the installed gd_PC directory and run
`cargo test --test board_motion_local -- --include-ignored` for the Rust checks.

Both sources yield 21 distinct headers and 3,300 position/rotation key records
counting every slot. The independent stream contains slot (u16), offset and
frame count (u32), then each part's position and rotation channels: count
(u32), then frame (i32) and three f32 positions or i32 BAMS rotations, all
little endian. SAModel's decoded positions normalize negative zero, so this
comparison treats positive and negative zero alike. Hashes including offsets:
REL a9d92d9a44ac0d5f87f234db38332c22e31988a9ed4a9a2023a7f37f790960d0;
PC 5ceea77a77d2d1ae0f4ce2a34f0b73518af5dd69ff9a21d89b7a9a30b5a3a2cb.
The Rust raw-key digest **preserving signed zero** and omitting offsets is
bd41749f3c03e66b4e2533e68384be38135cb222044d1115f26db32b539f899f
for both sources. This checks authored keys; it does not independently verify
retail interpolation or the poses seen during live play.
