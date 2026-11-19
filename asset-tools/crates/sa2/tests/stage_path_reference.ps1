param(
    [Parameter(Mandatory)][string]$ToolsRoot,
    [Parameter(Mandatory)][string]$GameExe,
    [Parameter(Mandatory)][string]$PrivateOutput
)
$ErrorActionPreference='Stop'
$ToolsRoot=(Resolve-Path -LiteralPath $ToolsRoot).Path
$GameExe=(Resolve-Path -LiteralPath $GameExe).Path
$PrivateOutput=[IO.Path]::GetFullPath($PrivateOutput)
if (Test-Path -LiteralPath $PrivateOutput) {throw 'Choose a new private output directory.'}
New-Item -ItemType Directory -Path $PrivateOutput | Out-Null
$config=@('game=SA2B','datafile=sonic2app.exe')
$tables=foreach($f in Get-ChildItem (Join-Path $ToolsRoot 'GameConfig/PC_SA2/*.ini')){
    foreach($m in [regex]::Matches((Get-Content -LiteralPath $f.FullName -Raw),'(?im)^\[[^\]]+\]\s*\r?\ntype=pathlist\b[^\[]*')){
        $config+=$m.Value.TrimEnd()
        $rva=[regex]::Match($m.Value,'(?im)^address=([a-f0-9]+)').Groups[1].Value
        $path=[regex]::Match($m.Value,'(?im)^filename=([^\r\n]+)').Groups[1].Value
        $count=[regex]::Match($m.Value,'(?im)^count=(\d+)').Groups[1].Value
        [pscustomobject]@{Rva=[Convert]::ToUInt32($rva,16); Path=$path; Count=$count}
    }
}
$cfg=Join-Path $PrivateOutput 'paths.ini'
Set-Content -LiteralPath $cfg -Value $config
$out=Join-Path $PrivateOutput 'split'
& (Join-Path $ToolsRoot 'bin/split.exe') binary $GameExe $cfg $out | Out-Null
if($LASTEXITCODE){throw 'SA Tools split failed.'}
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SAModel.dll')
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SplitTools.dll')
$bytes=[IO.File]::ReadAllBytes($GameExe)
$stream=[IO.MemoryStream]::new($bytes,$false)
$pe=[System.Reflection.PortableExecutable.PEReader]::new($stream)
$data=$pe.PEHeaders.SectionHeaders|Where-Object Name -eq '.data'
$delta=$data.VirtualAddress-$data.PointerToRawData
$base=[uint32]($pe.PEHeaders.PEHeader.ImageBase+$delta)
foreach($kind in @('executable','split INI')){
    $ms=[IO.MemoryStream]::new()
    $w=[IO.BinaryWriter]::new($ms)
    $pathCount=0
    $pointCount=0
    foreach($table in ($tables|Sort-Object Rva)){
        if($kind -eq 'split INI'){
            $paths=[SplitTools.PathList]::Load((Join-Path $out $table.Path))
        } elseif($table.Count -ne '') {
            $paths=[SplitTools.PathList]::LoadCount($bytes,[int]($table.Rva-$delta),$base,[int]$table.Count)
        } else {
            $paths=[SplitTools.PathList]::Load($bytes,[int]($table.Rva-$delta),$base)
        }
        $w.Write([uint32]$table.Rva)
        $w.Write([uint32]$paths.Count)
        foreach($p in $paths){
            $w.Write([int16]$p.Unknown)
            $w.Write([uint16]$p.Path.Count)
            $w.Write([single]$p.TotalDistance)
            $w.Write([uint32]$p.Code)
            foreach($point in $p.Path){$w.Write([byte[]]$point.GetBytes())}
            $pathCount++
            $pointCount+=$p.Path.Count
        }
    }
    $w.Flush()
    $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($ms.ToArray())).ToLowerInvariant()
    Write-Output "$kind - $($tables.Count) lists, $pathCount paths, $pointCount points, SHA-256 $hash"
}
