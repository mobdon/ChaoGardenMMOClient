param(
    [Parameter(Mandatory)][string]$ToolsRoot,
    [Parameter(Mandatory)][string]$GameData,
    [Parameter(Mandatory)][string]$PrivateOutput,
    [string]$GameExe,
    [string]$Overrides=(Join-Path $PSScriptRoot 'stage_land_offsets.tsv')
)
# Run in PowerShell 7 with SA Tools 786 and a new private output directory.
$ErrorActionPreference='Stop'
$ToolsRoot=(Resolve-Path -LiteralPath $ToolsRoot).Path
$GameData=(Resolve-Path -LiteralPath $GameData).Path
$PrivateOutput=[IO.Path]::GetFullPath($PrivateOutput)
if(Test-Path -LiteralPath $PrivateOutput){throw 'Choose a new private output directory'}
New-Item -ItemType Directory -Path $PrivateOutput|Out-Null
$split=Join-Path $ToolsRoot 'bin/split.exe'
if((Get-FileHash $split).Hash -ne '4A367B8832AB66BD12703383EBA7AF18B329B329580549D7BFAC05B308F9179C'){throw 'Unexpected split.exe revision'}
$corrected=@{}
if($Overrides){
    foreach($r in (Import-Csv -Delimiter "`t" -LiteralPath $Overrides)){
        if($r.address -notmatch '^[0-9A-Fa-f]+$' -or !$r.sha256){throw 'Invalid private override'}
        $corrected[($r.filename+'|'+$r.symbol)]=@{addr=$r.address;hash=$r.sha256}
    }
}
[void][Reflection.Assembly]::LoadFrom((Join-Path $ToolsRoot 'bin/lib/PRS.dll'))
$rows=@(foreach($ini in (Get-ChildItem (Join-Path $ToolsRoot 'GameConfig/GC_SA2B/*.ini') | Sort-Object Name)){
    $s=Get-Content -Raw -LiteralPath $ini.FullName
    $header=($s -split '(?m)^\[',2)[0]
    $relName=[regex]::Match($header,'(?im)^datafile=([^\r\n]+)').Groups[1].Value.Trim()
    if($ini.Name -ieq 'stg58D.ini'){$relName='stg58D.rel'}
    if($relName -notmatch '^[\w-]+\.(?:rel|prs)$'){continue}
    $rel=Join-Path $GameData $relName
    if(!(Test-Path -LiteralPath $rel -PathType Leaf)){continue}
    $hash=(Get-FileHash -LiteralPath $rel).Hash.ToLowerInvariant()
    $inputPath=$rel
    if($relName -like '*.prs'){
        $header=$header -replace '(?im)^compressedrel=[^\r\n]+',''
        $inputPath=Join-Path $PrivateOutput ($relName+'.rel')
        [IO.File]::WriteAllBytes($inputPath,[PSO.PRS.PRS]::Decompress([IO.File]::ReadAllBytes($rel)))
    }
    if($ini.Name -ieq 'stg32D.ini'){
        # PC_SA2/LeftoverDLL.ini names this SA2-format animated table absent
        # from the GC config. The actual Steam REL header is file offset 0x90D14.
        if($hash -ne '64d55b75fb8d0382c0ca4c0ad468af4ead93f0817ebb280d3435c85a3eed506d'){throw 'Unexpected Meteor Herd REL'}
        $s += "`n[objLandTable0032_uv]`ntype=landtable`naddress=90D14`nfilename=terrain.lvl`nformat=SA2`n"
    }
    foreach($m in [regex]::Matches($s,'(?im)^\[([^\]\r\n]+)\][^\[]*')){
        $body=$m.Value
        if($body -notmatch '(?im)^type=landtable\s*$'){continue}
        $sym=$m.Groups[1].Value
        if($sym -notmatch '^[\w-]+$'){throw 'Unexpected stage symbol'}
        $dir=Join-Path $PrivateOutput ($relName+"-"+$sym)
        New-Item -ItemType Directory -Path $dir|Out-Null
        $original=[regex]::Match($body,'(?im)^address=([a-f0-9]+)').Groups[1].Value
        $address=$original
        $key=$relName+'|'+$sym
        if($corrected.ContainsKey($key)){
            if($corrected[$key].hash -ne $hash){throw "Override SHA mismatch: $relName"}
            $address=$corrected[$key].addr
            $body=$body -replace '(?im)^address=[a-f0-9]+',("address="+$address)
        }
        $cfg=Join-Path $dir 'reference.ini'
        Set-Content -LiteralPath $cfg ($header+($body -replace '(?im)^filename=[^\r\n]+','filename=terrain.lvl'))
        $out=Join-Path $dir 'split'
        & $split binary $inputPath $cfg $out *> (Join-Path $dir 'split.log')
        $oracle=Join-Path $out 'terrain.lvl'
        [pscustomobject]@{filename=$relName;symbol=$sym;sha256=$hash;original_address=$original;address=$address;oracle=$oracle;generated=(Test-Path -LiteralPath $oracle -PathType Leaf)}
    }
})
if($rows.Count -ne 102){throw "Expected 102 mapped installed tables, got $($rows.Count)"}
$rows|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $PrivateOutput 'rows.json')
"$(($rows|Where-Object generated).Count)/$($rows.Count) oracle files materialized; the Rust comparison verifies their contents"

if($GameExe){
    $exe=(Resolve-Path -LiteralPath $GameExe).Path
    if((Get-FileHash -LiteralPath $exe).Hash -ne '4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16'){throw 'Unexpected Steam executable'}
    $s=Get-Content -LiteralPath (Join-Path $ToolsRoot 'GameConfig/PC_SA2/STG42.ini') -Raw
    $section=[regex]::Match($s,'(?im)^\[Sonic vs Shadow 2 Platform Collision\][^\[]*')
    if(!$section.Success){throw 'Missing platform model mapping'}
    $cfg=Join-Path $PrivateOutput 'platform.ini'
    Set-Content -LiteralPath $cfg ("game=SA2B`ndatafile=sonic2app.exe`n"+($section.Value -replace '(?im)^filename=[^\r\n]+','filename=platform.sa1mdl'))
    & $split binary $exe $cfg $PrivateOutput *> (Join-Path $PrivateOutput 'platform.log')
    if(!(Test-Path (Join-Path $PrivateOutput 'platform.sa1mdl'))){throw 'Missing platform model output'}
}
