param([Parameter(Mandatory)][string]$Cli,[Parameter(Mandatory)][string]$PrsDll,[Parameter(Mandatory)][string]$Game)
$ErrorActionPreference='Stop'
Add-Type -Path (Resolve-Path $PrsDll)
$folder=New-Item -ItemType Directory -Path (Join-Path .local ([guid]::NewGuid().ToString()))
$count=0
$banks=0
foreach($edition in 'Message','MessageK'){
  foreach($file in (Get-ChildItem (Join-Path $Game "resource/gd_PC/$edition") -Filter *.prs | Where-Object Name -match '^(mh|eh)\d{4}[efgijs]\.prs$')){
    $banks++
    $inputFile=$file.FullName
    $raw=[PSO.PRS.PRS]::Decompress([IO.File]::ReadAllBytes($inputFile))
    $outputFile=Join-Path $folder "$edition-$($file.BaseName).json"
    & $Cli --source-root $Game message-bank $inputFile $outputFile
    if($LASTEXITCODE){throw 'Rust conversion failed'}
    $report=Get-Content $outputFile -Raw | ConvertFrom-Json

    $groups=([int]$raw[0]*16777216+[int]$raw[1]*65536+[int]$raw[2]*256+$raw[3])/4-1
    if($report.groups.Count -ne $groups -or $report.decodedSha256 -ne [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($raw))){throw 'Bank mismatch'}
    for($i=0;$i -lt $groups;$i++){
      $o=$i*4
      $start=([int]$raw[$o] -shl 24) -bor ([int]$raw[$o+1] -shl 16) -bor ([int]$raw[$o+2] -shl 8) -bor $raw[$o+3]
      $end=[Array]::IndexOf($raw,[byte]0,$start)
      $row=$report.groups[$i]
      if($end -lt $start -or $row.group -ne $i -or $row.offset -ne $start -or $row.payload.Count -ne $end-$start){throw 'Group extent mismatch'}
      for($j=0;$j -lt $row.payload.Count;$j++){if($row.payload[$j] -ne $raw[$start+$j]){throw 'Group byte mismatch'}}
      $next=if($i+1 -eq $groups){$raw.Length}else{$o+=4; ([int]$raw[$o]*16777216+[int]$raw[$o+1]*65536+[int]$raw[$o+2]*256+$raw[$o+3])}
      if($row.postTerminatorBytes.Count -ne $next-$end-1){throw 'Trailer extent mismatch'}
      for($j=0;$j -lt $row.postTerminatorBytes.Count;$j++){if($row.postTerminatorBytes[$j] -ne $raw[$end+1+$j]){throw 'Trailer byte mismatch'}}
      $count++
    }
  }
}
if($banks -ne 840){throw "Expected 840 installed numbered banks, found $banks"}
Write-Output "$banks banks and $count localized groups matched SA Tools PRS output."
