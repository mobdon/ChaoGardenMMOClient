param([Parameter(Mandatory)][string]$GameExe, [Parameter(Mandatory)][string]$ToolsRoot,
      [Parameter(Mandatory)][string]$RustJson)
$ErrorActionPreference = 'Stop'
Add-Type -Path (Join-Path $ToolsRoot 'tools/lib/SAModel.dll')
Add-Type -Path (Join-Path $ToolsRoot 'tools/lib/SplitTools.dll')
$ini = Get-Content (Join-Path $ToolsRoot 'GameConfig/PC_SA2/Characters.ini') -Raw
$tables = Get-Content -LiteralPath $RustJson -Raw | ConvertFrom-Json
$bytes = [IO.File]::ReadAllBytes($GameExe)
$pe = [System.Reflection.PortableExecutable.PEReader]::new([IO.MemoryStream]::new($bytes,$false))
$total = 0
$maps = [regex]::Matches($ini,'(?im)^\[([^\]]+) Animation List\]\s*\r?\ntype=animationlist\s*\r?\naddress=([0-9a-f]+)[^\[]*?\ncount=(\d+)')
if ($maps.Count -ne 16 -or $tables.Count -ne 16) { throw 'Expected 16 tables' }
foreach ($map in $maps) {
    $name = $map.Groups[1].Value.Replace(' ','')
    $table = @($tables | Where-Object character -ceq $name)
    $count = [int]$map.Groups[3].Value
    if ($table.Count -ne 1) {throw "Missing/duplicate $name"}
    $table = $table[0]
    $rva = [Convert]::ToInt32($map.Groups[2].Value,16)
    $va = $pe.PEHeaders.PEHeader.ImageBase + $rva
    if ($table.source_va -ne $va -or $table.rows.Count -ne $count) {throw "Bad extent: $name"}
    $data = [byte[]]($pe.GetSectionData($rva).GetContent(0,$count*16))
    if ($table.sha256 -ne [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($data))) {throw "Bad hash: $name"}
    for ($i=0;$i -lt $count;$i++) {
        $ref = [SplitTools.SA2AnimationInfo]::new($data,$i*16)
        $row = $table.rows[$i]
        if ($row.source_va -ne $va+$i*16 -or ($row.motion -band 65535) -ne $ref.Animation -or
            ($row.model -band 65535) -ne $ref.Model -or ($row.mode -band 65535) -ne $ref.Property -or
            ($row.next -band 65535) -ne $ref.NextAnimation -or
            [BitConverter]::SingleToInt32Bits($row.transition_speed) -ne [BitConverter]::SingleToInt32Bits($ref.TransitionSpeed) -or
            [BitConverter]::SingleToInt32Bits($row.animation_speed) -ne [BitConverter]::SingleToInt32Bits($ref.AnimationSpeed)) {throw "Mismatch: $name row $i"}
    }
    $total += $count
}
"Matched $total animation rows in 16 tables."

