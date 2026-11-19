param(
    [Parameter(Mandatory)][string]$ToolsRoot,
    [Parameter(Mandatory)][string]$GameExe,
    [Parameter(Mandatory)][string]$PrivateOutput
)
$ErrorActionPreference = 'Stop'
$ToolsRoot = (Resolve-Path -LiteralPath $ToolsRoot).Path
$GameExe = (Resolve-Path -LiteralPath $GameExe).Path
$PrivateOutput = [IO.Path]::GetFullPath($PrivateOutput)
if (Test-Path -LiteralPath $PrivateOutput) { throw 'Choose a new private output directory.' }
New-Item -ItemType Directory -Path $PrivateOutput | Out-Null
$cfg = Join-Path $PrivateOutput 'deathzones.ini'
$configLines = @('game=SA2B','datafile=sonic2app.exe')
$tables = foreach ($file in Get-ChildItem (Join-Path $ToolsRoot 'GameConfig/PC_SA2/*.ini')) {
    $ini = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($m in [regex]::Matches($ini,'(?im)^\[([^\]]+)\]\s*\r?\ntype=deathzone\s*\r?\naddress=([a-f0-9]+)\s*\r?\nfilename=([^\r\n]+)')) {
        $configLines += $m.Value
        [pscustomobject]@{Rva=[Convert]::ToUInt32($m.Groups[2].Value,16); Filename=$m.Groups[3].Value}
    }
}
Set-Content -LiteralPath $cfg -Value $configLines
$dest = Join-Path $PrivateOutput 'split'
& (Join-Path $ToolsRoot 'bin/split.exe') binary $GameExe $cfg $dest | Out-Null
if ($LASTEXITCODE) { throw 'SA Tools split failed.' }
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SAModel.dll')
$stream = [IO.MemoryStream]::new()
$w = [IO.BinaryWriter]::new($stream)
function Write-Vector($v) {
    foreach ($f in @($v.X,$v.Y,$v.Z)) { if ($f -eq 0) {$w.Write([single]0)} else {$w.Write([single]$f)} }
}
$zoneCount = 0
foreach ($table in ($tables | Sort-Object Rva)) {
    $w.Write([uint32]$table.Rva)
    $zoneList = Join-Path $dest $table.Filename
    $data = Get-Content -LiteralPath $zoneList -Raw
    $entries = [regex]::Matches($data,'(?ms)^\[\d+\]\s*\n(.*?)(?=^\[|\z)')
    $w.Write([uint32]$entries.Count)
    foreach ($entry in $entries) {
        $props = ConvertFrom-StringData $entry.Groups[1].Value
        if ($props.Flags -ne 'Sonic, Shadow, Tails, Eggman, Knuckles, Rouge, MechTails, MechEggman') { throw 'Unexpected character flags.' }
        $w.Write([byte]255)
        foreach ($v in @($props.Constant1,$props.Constant2,$props.DeathFlag)) {$w.Write([byte]$v)}
        $model = [SAModel.ModelFile]::new((Join-Path (Split-Path $zoneList) $props.Filename),$false).Model
        $w.Write([uint32]$model.Flags)
        Write-Vector $model.Position
        $w.Write([byte[]]$model.Rotation.GetBytes())
        Write-Vector $model.Scale
        Write-Vector $model.Attach.Bounds.Center
        $w.Write([single]$model.Attach.Bounds.Radius)
        $w.Write([uint32]$model.Attach.Vertex.Count)
        foreach ($array in @('Vertex','Normal')) {
            foreach ($vertex in $model.Attach.$array) {Write-Vector $vertex}
        }
        $w.Write([byte[]]$model.Attach.Material[0].GetBytes())
        $mesh = $model.Attach.Mesh[0]
        $w.Write([uint16]$mesh.PolyType)
        $w.Write([uint16]$mesh.MaterialID)
        $w.Write([uint32]$mesh.Poly.Count)
        foreach ($p in $mesh.Poly) {
            $w.Write([byte][bool]$p.Reversed)
            $w.Write([uint32]$p.Indexes.Length)
            foreach ($ix in $p.Indexes) {$w.Write([uint16]$ix)}
        }
        $zoneCount++
    }
}
$w.Flush()
$hash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream.ToArray())).ToLowerInvariant()
Write-Output "$($tables.Count) tables, $zoneCount zones, SHA-256 $hash"
