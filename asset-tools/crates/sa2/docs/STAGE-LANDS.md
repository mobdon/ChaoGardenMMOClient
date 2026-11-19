# Installed stage landtables

`stage_land::catalog()` records the exact installed input hash, length, REL module
and section, source mapping and output path, table symbol, original counts and
header offset for each supported static table. `stage_land::read(bytes)` chooses
the first mapped table for that source; `read_named(bytes,symbol)` selects a
specific table, including side tables and individual Kindergarten rooms. Only
recognized input files are accepted. No name alone establishes source identity.

**102 tables, 79 files, 27,936 native entries.** These cover 57 numbered stage
RELs, seven boss RELs and fifteen PRS-compressed Chao stage RELs. There are
84 SA2B tables and 18 SA2 tables, including UV, poison, light and animated
side-table descriptors. The pure Rust reader decompresses PRS when necessary,
reads file-backed data from section four or five, resolves local ADDR32
relocations and decodes original transforms, Basic polygon indices and surface
flags. It does not execute a module. Only the observed 1, 4, 6 and 10
relocation types and 201–203 stream commands are recognized. REL instruction
relocations and imported pointers cannot be mistaken for local data pointers.

The result is a **static collision view**: Chunk and GC render attachments are
omitted or opaque. Zero Basic meshes are legitimate: the King Boom Boo stage
has geometry separate from its boss actors, and both Egg Golem landtables
classify every entry as visual. The existence of a descriptor never creates
a collision mesh. UV/animated side tables do not become walkable surfaces.
This reader does not implement object-driven collision, player response, route
construction, textures, event execution, camera or enemy behavior.

## How the locations were established

SA Tools 786 `GameConfig/GC_SA2B/*.ini` declares 101 landtables. It describes a
different revision in some modules. On the locally installed Steam files, 26
mapped headers in 23 files required offset corrections; blindly using the
original addresses produced empty or corrupt levels, or no split output. The
catalog records both offsets. The corrected locations were identified using
the local land-entry-table relocation, the complete strided run of root-node
relocations, the native entry and visual counts, and the referenced texture
identifier when present. A separate SA Tools binary split at each actual
location produced an original-format level. A successful splitter exit code
alone was not counted as evidence.

Three special mappings affect reproducibility:

- The GC `stg58D.ini` file names `stg56D.rel` in its preamble. The installed
  Iron Gate 2P table at 0x781C8 is in `stg58D.rel`; Eternal Engine 2P has
  its own, different, table and file.
- The GC Meteor Herd INI omits the UV table referenced by
  `PC_SA2/sa2lvl.ini` and named in `PC_SA2/LeftoverDLL.ini`. The installed
  REL contains a fourth table at 0x90D14, with 17 visible entries and the
  texture name `landtx32_uv`. A separate SA2-format split confirms it.
  For this single supplemental row both catalog offsets identify the verified
  REL location; the PC mapping supplies a name/path, not a GC file offset.
- Final Rush's UV header says 514 entries and 516 visual. Every one of the 514
  surface flags is exactly 0x80000000. The static view writes a visual count
  of 514, matching the independent SA Tools output; the catalog preserves 516.

## Other installed RELs

The other twelve loose modules have no matching GC landtable mapping and no
additional standard table found by the relocation/entry-layout search:
`advertiseD.rel`, `Boss_Last2D.rel`, `CartD.rel`, `ChaoMotionsD.rel`,
`ChaoStgLobby.rel`, `EmblemGetD.rel`, `EndingD.rel`, `eventD.rel`,
`mcwarnD.rel`, `otherprintD.rel`, `stg42D.rel`, `titleD.rel`.
This does not prove the absence of geometry. Finalhazard uses separate actor
models; the Sonic vs. Shadow 2 platform and kart pieces have separate object
or model sources. A kart course is assembled from segment IDs and piece
models, not from a single landtable. The five mapped lobby variants are
present in separate compressed Chao stage files.

## Verification

With the local `resource/gd_PC` folder in `SA2_GD_PC`, run:

```powershell
cargo test --manifest-path asset-tools/crates/sa2/Cargo.toml --test stage_land_catalog -- --ignored
```

The installed-source test decoded and baked each of the 102 tables using only
the shipped bytes and Rust code. To regenerate the independent reference for
a row, decompress PRS modules to an ignored private REL first. Run SA Tools
786 `split binary` on that REL with a private one-entry mapping using
`key=C900000`, `bigendian=True`, `game=SA2B`, `type=landtable` and the
catalog's verified file offset/format. Keep original and converted data
outside the repository. A comparison through the same Rust baker measures input
decoder equivalence, not independent retail engine physics.
