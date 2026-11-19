param(
    [Parameter(Mandatory)][string]$ToolsRoot,
    [Parameter(Mandatory)][string]$EventDirectory,
    [Parameter(Mandatory)][string]$PrivateOutput
)
$ErrorActionPreference='Stop'
$ToolsRoot=(Resolve-Path -LiteralPath $ToolsRoot).Path
$EventDirectory=(Resolve-Path -LiteralPath $EventDirectory).Path
$PrivateOutput=[IO.Path]::GetFullPath($PrivateOutput)
if(Test-Path -LiteralPath $PrivateOutput){throw 'Choose a new private directory.'}
New-Item -ItemType Directory -Path $PrivateOutput | Out-Null
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SAModel.dll')
Add-Type -Path (Join-Path $ToolsRoot 'bin/lib/SplitTools.dll')
$oracle=Join-Path $PrivateOutput 'camera-frames.bin'
$w=[IO.BinaryWriter]::new([IO.File]::OpenWrite($oracle))
$tracks=0
$samples=0
try{
    foreach($id in @(0,100,200)){
        $name='e{0:D4}' -f $id
        $out=Join-Path $PrivateOutput $name
        & (Join-Path $ToolsRoot 'bin/splitEvent.exe') (Join-Path $EventDirectory "$name.prs") $out | Out-Null
        if($LASTEXITCODE){throw 'SA Tools splitEvent failed.'}
        $dir=Join-Path $out $name
        $manifest=Get-Content (Join-Path $dir "$name.json") -Raw|ConvertFrom-Json -AsHashtable
        $indices=@{}
        foreach($p in $manifest.Motions.GetEnumerator()){if($p.Value){$indices[$p.Value]=[int]$p.Key}}
        for($s=0;$s -lt $manifest.Scenes.Count;$s++){
            $motions=$manifest.Scenes[$s].CameraMotions
            for($cam=0;$cam -lt $motions.Count;$cam++){
                $track=Join-Path $dir ("Scene {0}/Camera Motion {1}" -f $s,($cam+1))
                $motion=[SAModel.NJS_MOTION]::Load("$track.saanim",1)
                $attr=[SplitTools.SAArc.NJS_CAMERA]::Load("$track Attributes.ini")
                $w.Write([uint32]$id)
                $w.Write([int]$indices[$motions[$cam]])
                $w.Write([uint32]$motion.Frames)
                $w.Write([byte[]]$attr.NinjaCameraData.GetBytes())
                $model=$motion.Models[0]
                for($half=0;$half -lt 2*$motion.Frames;$half++){
                    $frame=[single]($half*0.5)
                    $w.Write([byte[]]$model.GetPosition($frame).GetBytes())
                    $w.Write([byte[]]$model.GetTarget($frame).GetBytes())
                    $w.Write([int]$model.GetRoll($frame))
                    $w.Write([int]$model.GetAngle($frame))
                    $samples++
                }
                $tracks++
            }
        }
    }
} finally {$w.Dispose()}
Write-Output "$tracks camera tracks, $samples half-frame evaluations. $oracle"
