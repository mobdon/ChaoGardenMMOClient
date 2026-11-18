param(
    [Parameter(Mandatory = $true)][string[]]$InputFile,
    [switch]$ExtractLocal
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
$taskOutputRoot = Join-Path $taskProjectRoot '.local/gba'
New-Item -ItemType Directory -Force -Path $taskOutputRoot | Out-Null
$taskMaxRomBytes = 32MB
$taskKnownAdvance2Sha1 = '7bcd6a07af7c894746fa28073fe0c0e34408022d'

function Get-TaskHash([byte[]]$Bytes, [string]$Algorithm) {
    $taskHasher = [System.Security.Cryptography.HashAlgorithm]::Create($Algorithm)
    try { return ([BitConverter]::ToString($taskHasher.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $taskHasher.Dispose() }
}

function Read-TaskBounded([System.IO.Stream]$Stream) {
    $taskBuffer = New-Object byte[] 65536
    $taskMemory = New-Object System.IO.MemoryStream
    try {
        while (($taskRead = $Stream.Read($taskBuffer, 0, $taskBuffer.Length)) -gt 0) {
            if ($taskMemory.Length + $taskRead -gt $taskMaxRomBytes) { throw 'ROM exceeds the 32 MiB input limit.' }
            $taskMemory.Write($taskBuffer, 0, $taskRead)
        }
        return ,$taskMemory.ToArray()
    } finally { $taskMemory.Dispose() }
}

function Get-TaskRom([byte[]]$Bytes, [string]$SourcePath, [string]$EntryName) {
    if ($Bytes.Length -lt 192) { throw "Truncated GBA header: $EntryName" }
    $taskHeaderSum = 0
    for ($taskIndex = 0xA0; $taskIndex -le 0xBC; $taskIndex++) { $taskHeaderSum += $Bytes[$taskIndex] }
    $taskExpectedChecksum = (-$taskHeaderSum - 0x19) -band 0xFF
    $taskValidHeader = $Bytes[0xB2] -eq 0x96 -and $Bytes[0xBD] -eq $taskExpectedChecksum
    if (-not $taskValidHeader) { throw "Invalid GBA fixed/header checksum bytes: $EntryName" }
    $taskSha1 = Get-TaskHash $Bytes 'SHA1'
    $taskSha256 = Get-TaskHash $Bytes 'SHA256'
    $taskTitle = [Text.Encoding]::ASCII.GetString($Bytes, 0xA0, 12).Trim([char]0, [char]32)
    $taskCode = [Text.Encoding]::ASCII.GetString($Bytes, 0xAC, 4)
    if ($taskCode -notmatch '^[A-Z0-9]{4}$') { throw "Unsupported game code: $EntryName" }
    $taskRelativeOutput = $null
    if ($ExtractLocal) {
        # Generated names never use the untrusted ZIP path; nothing is extracted into the source tree.
        $taskFilename = $taskCode + '-' + $taskSha256.Substring(0, 12) + '.gba'
        [IO.File]::WriteAllBytes((Join-Path $taskOutputRoot $taskFilename), $Bytes)
        $taskRelativeOutput = '.local/gba/' + $taskFilename
    }
    [ordered]@{
        sourcePath = $SourcePath
        archiveEntry = $EntryName
        bytes = $Bytes.Length
        title = $taskTitle
        gameCode = $taskCode
        makerCode = [Text.Encoding]::ASCII.GetString($Bytes, 0xB0, 2)
        revision = [int]$Bytes[0xBC]
        headerChecksumValid = $taskValidHeader
        sha1 = $taskSha1
        sha256 = $taskSha256
        referenceMatch = $(if ($taskSha1 -eq $taskKnownAdvance2Sha1) { 'SAT-R/sa2 USA retail SHA-1' } else { $null })
        localRom = $taskRelativeOutput
    }
}

$taskRoms = @()
$taskSources = @()
foreach ($taskInput in $InputFile) {
    $taskPath = (Resolve-Path -LiteralPath $taskInput).ProviderPath
    $taskInfo = Get-Item -LiteralPath $taskPath
    if ($taskInfo.Length -gt 64MB) { throw "Input exceeds 64 MiB: $taskPath" }
    $taskSources += [ordered]@{ path = $taskPath; bytes = $taskInfo.Length; sha256 = (Get-FileHash -LiteralPath $taskPath -Algorithm SHA256).Hash.ToLowerInvariant() }
    $taskStream = [IO.File]::OpenRead($taskPath)
    try {
        if ($taskInfo.Extension -ieq '.zip') {
            $taskZip = [IO.Compression.ZipArchive]::new($taskStream, [IO.Compression.ZipArchiveMode]::Read, $true)
            try {
                if ($taskZip.Entries.Count -gt 64) { throw 'ZIP has too many entries.' }
                $taskCandidates = @($taskZip.Entries | Where-Object { $_.Name -match '(?i)\.gba$' })
                if ($taskCandidates.Count -eq 0 -or $taskCandidates.Count -gt 8) { throw 'ZIP must contain between one and eight GBA files.' }
                foreach ($taskEntry in $taskCandidates) {
                    if ($taskEntry.Length -gt $taskMaxRomBytes) { throw 'ZIP ROM exceeds 32 MiB.' }
                    $taskEntryStream = $taskEntry.Open()
                    try { $taskBytes = Read-TaskBounded $taskEntryStream }
                    finally { $taskEntryStream.Dispose() }
                    if ($taskBytes.Length -ne $taskEntry.Length) { throw 'ZIP entry size does not match decoded data.' }
                    $taskRoms += Get-TaskRom $taskBytes $taskPath $taskEntry.FullName
                }
            } finally { $taskZip.Dispose() }
        } elseif ($taskInfo.Extension -ieq '.gba') {
            $taskRoms += Get-TaskRom (Read-TaskBounded $taskStream) $taskPath $taskInfo.Name
        } else { throw 'Supported inputs: .zip and .gba.' }
    } finally { $taskStream.Dispose() }
}

$taskReport = [ordered]@{
    schemaVersion = 1
    createdUtc = [DateTime]::UtcNow.ToString('o')
    purpose = 'Local supplied-ROM identification only; no engine port or entitlement claim.'
    sources = $taskSources
    roms = $taskRoms
}
$taskReport | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $taskOutputRoot 'inventory.json') -Encoding UTF8
$taskRoms | ForEach-Object { [pscustomobject]$_ } | Select-Object title, gameCode, bytes, headerChecksumValid, sha1, referenceMatch, localRom
