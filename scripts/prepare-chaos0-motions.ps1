# splitMTN guesses endian from the first ID. Chaos starts at 0x0100, so
# put ID 242 first in a private working copy. Data and offsets stay intact.
$root=Split-Path -Parent $PSScriptRoot
$base=Join-Path $root '.local/extracted/dev-roster'
$lib=Join-Path $root '.local/tools/sa-tools-786/bin/lib'
[void][reflection.assembly]::LoadFrom((Join-Path $lib 'PRS.dll'))
$bytes=[PSO.PRS.PRS]::Decompress([io.file]::ReadAllBytes((Join-Path $base 'chaos0mtn.prs')))
if([Convert]::ToHexString($bytes[0..3]) -ne '0100003E' -or [Convert]::ToHexString($bytes[24..27]) -ne '00F2003E'){throw 'Unexpected Chaos motion table'}
$tmp=$bytes[0..7]
[Array]::Copy($bytes,24,$bytes,0,8)
[Array]::Copy($tmp,0,$bytes,24,8)
$out=Join-Path $base 'chaos0mtn.bin'
[io.file]::WriteAllBytes($out,$bytes)
$env:DOTNET_ROLL_FORWARD='Major'
dotnet (Join-Path $lib 'splitMTN.dll') $out
if($LASTEXITCODE -ne 0){throw 'Chaos motion extraction failed'}
