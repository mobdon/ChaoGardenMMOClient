# Steam player physics parameters

The Rust reader loads the original PhysicsArray defaults for the supported Steam sonic2app.exe (SHA-256 4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16). Each JSON row preserves its native slot index, source VA, character identifier when known and all 33 named parameters. Sonic's row contains jump2_timer 60, jmp_y_spd 1.66, run_accel 0.05, run_speed 1.39, rad 4 and height 10. These are values stored before the player update routines run. Their interpretation in each movement mode and resulting per-frame velocity remain separate work.

SA2 Mod Loader declares 19 records at VA 0x017391E8. The independently mapped first 17 entries in SA Tools 786 Characters.ini begin at RVA 0x013391E8 and advance by 0x84 bytes. Slots 0–16 have names in the mod-loader character enum, including slot 11's placeholder B (SA Tools labels it Unused). Slots 17 and 18 have records in the declared array and no corresponding enum names, so their character field is null. The first field is a signed 32-bit jump2_timer, followed by 32 little-endian floats. Output uses the player_parameter / SA Tools names. [Array declaration](https://github.com/X-Hax/sa2-mod-loader/blob/1e98ebc97717c0be43280f0c3fe2afde24dededb/SA2ModLoader/include/SA2Variables.h#L466) · [Layout](https://github.com/X-Hax/sa2-mod-loader/blob/1e98ebc97717c0be43280f0c3fe2afde24dededb/SA2ModLoader/include/SA2Structs.h#L619-L690) · [SA Tools decoder](https://github.com/X-Hax/sa_tools/blob/cc05c849b1b7017fb0b519932863e206fa34077a/Libraries/SplitTools/IniData.cs#L6132-L6208)

## Reproduce

Run from the sa2 crate with the original mod-loader header and the local SA Tools installation:

```powershell
$env:SA2_PC_EXE = 'C:\path\to\sonic2app.exe'
./tests/player_physics_reference.ps1 -ToolsRoot 'C:\path\to\sa-tools-786' -GameExe $env:SA2_PC_EXE -VariablesHeader 'C:\path\to\SA2Variables.h'
cargo test --locked --test player_physics -- --include-ignored
```

The independent checker resolves the section with .NET PEReader and decodes each record using the original SplitTools.dll PlayerParameter constructor. It cross-checks all 17 configured RVAs, reflects the decoder's field names, and hashes all 627 named values in slot and ordinal field-name order. The Rust test checks the source addresses and compares its typed public output to that digest. The reference SHA-256 is e233e63003d72275a6dfc5f0e615d4f8ae72b53e3242707f3fe272a3b0c119d3.
