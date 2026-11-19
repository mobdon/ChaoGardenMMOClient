param(
    [Parameter(Mandatory)][string]$ToolsRoot,
    [Parameter(Mandatory)][string]$GameExe,
    [Parameter(Mandatory)][string]$VariablesHeader
)
$ErrorActionPreference='Stop'
$ToolsRoot=(Resolve-Path -LiteralPath $ToolsRoot).Path
$bytes=[IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $GameExe).Path)
$decl=[regex]::Match((Get-Content -LiteralPath $VariablesHeader -Raw),'DataArray\(PhysicsData,\s*PhysicsArray,\s*0x([A-Fa-f0-9]+),\s*(\d+)\)')
if(!$decl.Success){throw 'PhysicsArray declaration not found'}
$va=[Convert]::ToUInt32($decl.Groups[1].Value,16)
$count=[int]$decl.Groups[2].Value
$ini=Get-Content -LiteralPath (Join-Path $ToolsRoot 'GameConfig/PC_SA2/Characters.ini') -Raw
$mapped=[regex]::Matches($ini,'(?im)^type=physicsdata\s*\r?\naddress=([0-9a-f]+)')
$pe=[System.Reflection.PortableExecutable.PEReader]::new([IO.MemoryStream]::new($bytes,$false))
$rva=$va-$pe.PEHeaders.PEHeader.ImageBase
$stride=[Convert]::ToUInt32($mapped[1].Groups[1].Value,16)-[Convert]::ToUInt32($mapped[0].Groups[1].Value,16)
for($i=0;$i -lt $mapped.Count;$i++){
    if([Convert]::ToUInt32($mapped[$i].Groups[1].Value,16) -ne $rva+$i*$stride){throw 'Independent character map disagrees.'}
}
$sec=$pe.PEHeaders.SectionHeaders|Where-Object {$rva -ge $_.VirtualAddress -and $rva+$count*$stride -le $_.VirtualAddress+$_.SizeOfRawData}
if($null -eq $sec){throw 'Physics table not file-backed.'}
$start=$rva-$sec.VirtualAddress+$sec.PointerToRawData
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SAModel.dll')
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SplitTools.dll')
$stream=[IO.MemoryStream]::new()
$w=[IO.BinaryWriter]::new($stream)
$names=[SplitTools.PlayerParameter].GetProperties().Name
[Array]::Sort($names,[StringComparer]::Ordinal)
for($i=0;$i -lt $count;$i++){
    $p=[SplitTools.PlayerParameter]::new($bytes,$start+$i*$stride)
    $w.Write([uint32]$i)
    foreach($key in $names){
        $w.Write([Text.Encoding]::ASCII.GetBytes($key))
        $w.Write([byte]0)
        if($key -eq 'jump2_timer'){$w.Write([int]$p.$key)}else{$w.Write([single]$p.$key)}
    }
    if($i -eq 0){Write-Output "Sonic: jump2_timer=$($p.jump2_timer) jmp_y_spd=$($p.jmp_y_spd) run_accel=$($p.run_accel) run_speed=$($p.run_speed) rad=$($p.rad) height=$($p.height)"}
}
$w.Flush()
$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream.ToArray())).ToLowerInvariant()
Write-Output "$count slots, $($names.Count) named fields, $($mapped.Count) SA Tools config entries. SHA-256 $hash"
