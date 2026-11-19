param([Parameter(Mandatory)][string]$GamePath,[Parameter(Mandatory)][string]$ToolsRoot,
      [Parameter(Mandatory)][string]$PrivateOutput)
$ErrorActionPreference='Stop'
$GamePath=(Resolve-Path -LiteralPath $GamePath).Path
$ToolsRoot=(Resolve-Path -LiteralPath $ToolsRoot).Path
$PrivateOutput=[IO.Path]::GetFullPath($PrivateOutput)
if(Test-Path -LiteralPath $PrivateOutput){throw 'Choose a new private output directory.'}
$ancestor=[IO.DirectoryInfo]::new($PrivateOutput)
$checkout=$null
while($null -ne $ancestor){
    if($ancestor.Exists -and ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Private output may not traverse a link.'}
    if((Test-Path -LiteralPath (Join-Path $ancestor.FullName '.git')) -and !$checkout){$checkout=$ancestor.FullName}
    $ancestor=$ancestor.Parent
}
if(!$checkout){throw 'Choose a directory inside the checkout-local .local folder.'}
$relative=[IO.Path]::GetRelativePath($checkout,$PrivateOutput)
if(($relative -split '[/\\]')[0] -ne '.local'){throw 'Reference files belong in the checkout-local .local folder.'}
New-Item -ItemType Directory -Path $PrivateOutput | Out-Null
$source=Get-Content -Raw -LiteralPath (Join-Path $ToolsRoot 'GameConfig/PC_SA2/STG13.ini')
$config=@('game=SA2B','datafile=sonic2app.exe')
$blocks=[regex]::Matches($source,'(?ms)^\[([^\]]+)\]\r?\n([^\[]*)')
$count=0
foreach($block in $blocks){
  $s=$block.Value
  if($s -notmatch '(?im)^filename=stg13_cityescape/(?:tls|models)/(?:Streetboard|ShadowBoard|AmyBoard|MetalSonicBoard|SonicBoard|Board_Shadow|Board_Modifier)\.(?:satex|sa2mdl)\s*$'){continue}
  $config+=($s -replace '(?m)^animations=[^\r\n]*','')
  $count++
}
if($count -ne 12){throw 'Expected seven Chunk models and five texture lists.'}
$ini=Join-Path $PrivateOutput 'board.ini'
Set-Content -LiteralPath $ini -Value $config
& (Join-Path $ToolsRoot 'bin/split.exe') binary (Join-Path $GamePath 'sonic2app.exe') $ini (Join-Path $PrivateOutput 'split')
if($LASTEXITCODE){throw 'Model split failed.'}
$copy=Join-Path $PrivateOutput 'objtex_stg13.prs'
Copy-Item -LiteralPath (Join-Path $GamePath 'resource/gd_PC/objtex_stg13.prs') -Destination $copy
& (Join-Path $ToolsRoot 'bin/ArchiveTool.exe') -png $copy
if($LASTEXITCODE){throw 'Archive PNG conversion failed.'}
