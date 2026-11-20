param([Parameter(Mandatory)][string]$ToolsRoot,[Parameter(Mandatory)][string]$SplitDirectory)
$ErrorActionPreference='Stop'
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SAModel.dll')
foreach($pair in 'ssonic:106','ssonic:108','sshadow:111','sshadow:115'){
    $stem,$slot=$pair.Split(':')
    $m=[SAModel.NJS_MOTION]::Load((Join-Path $SplitDirectory "${stem}mtn/$slot.saanim"),1)
    $stream=[IO.MemoryStream]::new()
    $w=[IO.BinaryWriter]::new($stream)
    try {
        for($i=0;$i -lt 2*$m.Frames;$i++){
            $t=[single]($i*0.5)
            $w.Write([byte[]]$m.Models[0].GetPosition($t).GetBytes())
            $w.Write([byte[]]$m.Models[0].GetTarget($t).GetBytes())
            $w.Write([int]$m.Models[0].GetRoll($t))
            $w.Write([int]$m.Models[0].GetAngle($t))
        }
        $w.Flush()
        "$pair $($m.Frames) $([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream.ToArray())).ToLowerInvariant())"
    } finally {$w.Dispose();$stream.Dispose()}
}
