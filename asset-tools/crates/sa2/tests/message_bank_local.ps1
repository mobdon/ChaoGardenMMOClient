param([Parameter(Mandatory)][string]$Cli,[Parameter(Mandatory)][string]$PrsDll,[Parameter(Mandatory)][string]$Game)
$ErrorActionPreference='Stop'
Add-Type -Path (Resolve-Path $PrsDll)
$folder=New-Item -ItemType Directory -Path (Join-Path .local ([guid]::NewGuid().ToString()))
$count=0
foreach($edition in 'Message','MessageK'){
  foreach($lang in 'e','f','g','i','j','s'){
    $inputFile=Join-Path $Game "resource/gd_PC/$edition/mh0020$lang.prs"
    $raw=[PSO.PRS.PRS]::Decompress([IO.File]::ReadAllBytes($inputFile))
    $outputFile=Join-Path $folder "$edition-$lang.json"
    & $Cli --source-root $Game message-bank $inputFile $outputFile
    if($LASTEXITCODE){throw 'Rust conversion failed'}
    $report=Get-Content $outputFile -Raw | ConvertFrom-Json
    if($report.groups.Count -ne 6 -or $report.decodedSha256 -ne [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($raw))){throw 'Bank mismatch'}
    for($i=0;$i -lt 6;$i++){
      $o=$i*4
      $start=([int]$raw[$o] -shl 24) -bor ([int]$raw[$o+1] -shl 16) -bor ([int]$raw[$o+2] -shl 8) -bor $raw[$o+3]
      $end=[Array]::IndexOf($raw,[byte]0,$start)
      $row=$report.groups[$i]
      if($end -lt $start -or $row.group -ne $i -or $row.offset -ne $start -or $row.payload.Count -ne $end-$start){throw 'Group extent mismatch'}
      for($j=0;$j -lt $row.payload.Count;$j++){if($row.payload[$j] -ne $raw[$start+$j]){throw 'Group byte mismatch'}}
      $count++
    }
  }
}
Write-Output "$count localized groups matched SA Tools PRS output."
