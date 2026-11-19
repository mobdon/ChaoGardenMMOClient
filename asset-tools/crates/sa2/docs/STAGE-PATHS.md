# Native stage and enemy path resources

The reader loads PC PATH_TAG lists directly from the supported Steam sonic2app.exe (SHA-256 4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16). Each SA Tools pathlist section supplies the table RVA. A list terminates with a null pointer unless the configuration specifies its count. The four counted enemy lists contain 12, 2, 12 and 12 entries respectively. Each pointer and point array must occupy a file-backed PE span.

Each path retains its tag VA, path-type byte, second header byte, authored total length, point-array VA and task callback VA. Each point retains the two signed rotation words (X, Z), authored length, and XYZ position in native units. The [SAMT native declarations](https://github.com/roeming/SA2B/blob/874a480379260dce01e8988d6ab13c0deddcee4c/include/samt/sonic/path.h) and [SA Tools PathList/PathData implementation](https://github.com/X-Hax/sa_tools/blob/cc05c849b1b7017fb0b519932863e206fa34077a/Libraries/SplitTools/IniData.cs#L3635-L3806) give the field layouts. SA Tools exposes the first two header bytes together as Unknown. The reader preserves both. Path type and callback address alone do not establish how a player acquires a rail, how points are interpolated, how velocity changes, or when a player detaches.

The mapped PC stage, boss and enemy configurations contain 22 lists with 228 paths and 10,360 points. City Escape's table at RVA 0xCDB35C has 13 paths with 200 points. Global safeguards limit configuration bytes to 1 MiB, lists to 128, paths per list to 4,096 and total output points to 1,000,000.

## Independent comparison

Run the script from the sa2 crate directory with SA Tools 786. Choose a new, private output directory for extracted native data.

```powershell
$env:SA2_PC_EXE = 'C:\path\to\sonic2app.exe'
$pathOutput = 'C:\path\to\private\path-verification'
./tests/stage_path_reference.ps1 -ToolsRoot 'C:\path\to\sa-tools-786' -GameExe $env:SA2_PC_EXE -PrivateOutput $pathOutput
$env:SA2_PATH_CONFIG = Join-Path $pathOutput 'paths.ini'
cargo test --locked --test stage_paths -- --include-ignored
```

The oracle reads each list twice using original SA Tools code. The first pass loads binary arrays directly with SplitTools.PathList.Load or LoadCount. The second runs split.exe and reloads each exported directory with SplitTools.dll. Both produced SHA-256 83fbc1034753bbf00e35679ab73335a8fc98f21b67903289bca1ca11fea3b96d for the 210,112-byte decoded sequence. The stream includes the table RVAs, list/point counts, both header bytes, total distances, callback addresses and all point fields in file order. Its only differences from the Rust raw stream were 21 signed floating-point zeros normalized by SA Tools. The test checks this comparator digest and the Rust source-bit digest 101004e33922f51ac8df76447a36b3fe5f53149446b9c175afd636a4c815bb39. Verification on this host used DOTNET_ROLL_FORWARD=Major to run the original utilities under the installed Windows Desktop runtime.
