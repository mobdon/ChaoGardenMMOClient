# PC stage SET to object-table binding

`stage_catalog::bind(set, exe, level_config, object_config, level_section)` takes an explicit level section name from SA Tools 786's PC_SA2 `sa2lvl.ini`. It resolves that section's `ObjectList` to the matching `type=objlist` filename in `ObjectLists.ini`, verifies the Steam executable SHA-256, and reads its 16-byte object records through the file-backed PE section map. Casing and slash direction in object-list paths are normalized. Section names are literal, including the two consecutive backslashes used by these INI files. For example, the PowerShell argument for City Escape is `'Hero Story\\City Escape'`. `Catalog::new` verifies the executable once for batch reads.

Each placement retains its input order, byte offset, clip level, all rotation words, position and three object-specific parameter floats. The lower twelve bits select a zero-based entry in the chosen table. The report carries the table once and a nullable definition index per placement. Unresolved IDs are counted by value. The report also lists every section sharing the selected LevelID or table address, the declared SET name when present, and any absent list mapping. An unknown section is an error. A known level lacking a list returns its placements with an explicit gap. Object names and function addresses are executable data; this operation does not start actors, simulate bosses, select variants from filenames, or confirm that the caller chose the right section for an arbitrary SET. The SET header has no stage identity.

## Verified installed corpus

An independent Python pass read the INIs with ConfigParser, mapped PE sections directly from the executable file, decoded the SET IDs, and computed a digest over all placements and their resolved record bytes and names. The opt-in Rust test pins the section selection, source/configuration hashes, independent digest, counts and known unresolved IDs. For the November 18, 2026 installation:

| Input | Files | Placements | Matched | Unmatched |
| --- | ---: | ---: | ---: | ---: |
| Numbered SETs | 209 | 58,592 | 58,498 | 94 |
| Named boss SETs | 14 | 156 | 156 | 0 |
| Named garden SETs | 9 | 272 | 272 | 0 |
| Total | 232 | 59,020 | 58,926 | 94 |

* Stage 0001 has two numbered SET files containing 53 placements but no level section. No other stage table is substituted.
* Iron Gate's 92-entry table cannot resolve 38 instances of ID 92 in `set0011_2p_s.bin`. The configured 2P Dry Lagoon table has 89 entries and cannot resolve IDs 89, 92 and 94 in `set0046_2p_s.bin`.
* 2P Dry Lagoon and Pyramid Race reference the same header RVA, `0x11A76B0`, through different config filenames. The second Tails vs Eggman boss and 2P Eternal Engine reference `0x8873A8` through the same filename. The report exposes the alias without assuming why it occurs.
* Nineteen Chao World sections share LevelID 0090. The three named garden SET families use distinct selected tables. Three race sections share `0xF533F8`; thirteen other 0090 sections declare no object list. A bare numeric selector cannot identify a Chao section.
* Final Hazard declares `BossLast2/objList.ini`, which has no object-list mapping in this PC configuration. It has no matching named SET in this checked installation. Eight kart files use a separate input family and are outside this binding test. A resolved entry alone provides no evidence its runtime callback executes.

Run from the crate with `SA2_GAME_PATH` pointing to the installation containing `sonic2app.exe` and `SA2_PC_CONFIG` pointing to the SA Tools `GameConfig/PC_SA2` directory:

```powershell
cargo test --locked --test stage_catalog_local -- --ignored --nocapture
```

The executable SHA-256 is `4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16`. The fingerprint of the 232 sorted lowercase filenames and their source hashes is `f35a34578d4ca308558f7302cd3ac57c5c3296b45cfc0f98829a96356596bdc9`. The independently constructed placement/definition fingerprint is `78be8cf2eefe3875b191a97b4b97a162bfe9ea9782ac132ed251ba4c84ce9b65`. The committed test contains the exact config hashes; executable, SETs, INI files and generated definitions stay local.
