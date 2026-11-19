param([Parameter(Mandatory)][string]$Cli,[Parameter(Mandatory)][string]$ToolLib,[Parameter(Mandatory)][string]$Game)
$ErrorActionPreference='Stop'
Add-Type -Path (Join-Path $ToolLib SAModel.dll)
Add-Type -Path (Join-Path $ToolLib SA2LightFogEditor.dll)
[SAModel.ByteConverter]::BigEndian=$true
$dir=New-Item -ItemType Directory -Path (Join-Path .local ([guid]::NewGuid().ToString()))
$counts=@{16=0;528=0}
foreach($file in Get-ChildItem (Join-Path $Game 'resource/gd_PC') -Filter '*fog*.bin'){
    $raw=[IO.File]::ReadAllBytes($file.FullName)
    $long=$raw.Length -eq 528
    $ref=[SA2LightFogEditor.FogData]::new($raw,$true,$long)
    $out=Join-Path $dir ($file.Name+'.json')
    & $Cli --source-root $Game stage-fog $file.FullName $out
    if($LASTEXITCODE){throw "Conversion failed: $($file.Name)"}
    $v=Get-Content $out -Raw | ConvertFrom-Json
    if($v.type -ne $ref.Type -or $v.unknown -ne $ref.Unk -or ($v.argb -join ',') -ne (@($ref.A,$ref.R,$ref.G,$ref.B) -join ',') -or
       $v.maxDistanceBits -ne [BitConverter]::SingleToUInt32Bits($ref.MaxDist) -or $v.minDistanceBits -ne [BitConverter]::SingleToUInt32Bits($ref.MinDist)){throw "Header mismatch: $($file.Name)"}
    if($long){
        if($v.tableBits.Count -ne 128){throw 'Table extent mismatch'}
        for($i=0;$i -lt 128;$i++){if($v.tableBits[$i] -ne [BitConverter]::SingleToUInt32Bits($ref.FogTable[$i])){throw "Table mismatch at $($file.Name) sample $i"}}
    }elseif($null -ne $v.tableBits){throw 'Unexpected table in short file'}
    $counts[$raw.Length]++
}
if($counts[16] -ne 19 -or $counts[528] -ne 19){throw 'Installed fog inventory mismatch'}
Write-Output '38 installed headers and 2432 table samples matched SA Tools 786.'
