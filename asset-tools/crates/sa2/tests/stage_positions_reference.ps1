param([Parameter(Mandatory)][string]$Tools,[Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference='Stop'
$cfg=Get-Content (Join-Path $Tools 'GameConfig/PC_SA2/Characters.ini') -Raw
$lib=[Reflection.Assembly]::LoadFrom((Join-Path (Resolve-Path $Tools) 'tools/lib/SplitTools.dll'))
$bytes=[IO.File]::ReadAllBytes((Resolve-Path $Exe))
$pe=[BitConverter]::ToInt32($bytes,60)
$n=[BitConverter]::ToUInt16($bytes,$pe+6)
$opt=[BitConverter]::ToUInt16($bytes,$pe+20)
$sections=for($i=0;$i -lt $n;$i++){
  $h=$pe+24+$opt+40*$i
  [pscustomobject]@{Rva=[BitConverter]::ToUInt32($bytes,$h+12);Size=[BitConverter]::ToUInt32($bytes,$h+16);Raw=[BitConverter]::ToUInt32($bytes,$h+20)}
}
$tables=foreach($match in [regex]::Matches($cfg,'(?m)^\[([^\]]+)\]\r?\ntype=(startpos|startendpos|endpos|shortpos)\r?\naddress=([0-9A-F]+)')){
  [pscustomobject]@{Kind=$match.Groups[2].Value; Rva=[Convert]::ToInt32($match.Groups[3].Value,16)}
}
$stream=[IO.MemoryStream]::new()
$writer=[IO.BinaryWriter]::new($stream)
$total=0
$city=0
foreach($tab in $tables | Sort-Object Rva){
  $sec=$sections | Where-Object {$_.Rva -le $tab.Rva -and $tab.Rva -lt $_.Rva+$_.Size}
  $at=[int]($tab.Rva-$sec.Rva+$sec.Raw)
  switch($tab.Kind){
    'endpos' {$rows=[SplitTools.SA2MiniPosList]::LoadEnd($bytes,$at);$slots=@('Mission2','Mission3')}
    'shortpos' {$rows=[SplitTools.SA2MiniPosList]::LoadMulti($bytes,$at);$slots=@('Player1','Player2')}
    default {$rows=[SplitTools.SA2StartPosList]::Load($bytes,$at);$slots=@('','P1','P2')}
  }
  foreach($level in $rows.Keys){
    $v=$rows[$level];$writer.Write([uint16]$level);$writer.Write([byte]$slots.Count)
    if($slots.Count -eq 2){$writer.Write([uint16]$v.Unknown)}
    foreach($s in $slots){
      $rot=$s+'YRotation';$pos=$s+'Position'
      $writer.Write([uint16]$v.$rot)
      foreach($axis in 'X','Y','Z'){$writer.Write([float]$v.$pos.$axis)}
    }
    $total++;if([int]$level -eq 13){$city++; Write-Verbose ("stage 13 {0:X} {1}: {2}" -f $tab.Rva,$tab.Kind,($v | Out-String).Trim())}
  }
}
"$($tables.Count) tables, $total rows, $city City Escape rows; decoded fields SHA-256 $([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream.ToArray())))"
