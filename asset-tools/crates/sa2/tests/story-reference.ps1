param(
    [Parameter(Mandatory=$true)][string]$SplitTools,
    [Parameter(Mandatory=$true)][string]$StoryMappings,
    [Parameter(Mandatory=$true)][string]$SplitDirectory
)
$ErrorActionPreference='Stop'
[void][Reflection.Assembly]::LoadFrom((Resolve-Path -LiteralPath $SplitTools).Path)
$sections=[Collections.Generic.SortedDictionary[string,string]]::new([StringComparer]::Ordinal)
$section=$null
$type=$null
foreach($line in (Get-Content -LiteralPath $StoryMappings)){
    if($line -match '^\[(.+)\]$'){ $section=$Matches[1]; $type=$null }
    elseif($line -match '^type=(.+)$'){ $type=$Matches[1] }
    elseif($line -match '^filename=(.+)$' -and $type -eq 'storysequence'){ $sections.Add($section,$Matches[1]) }
}
$records=0
$actions=0
$text=[Text.StringBuilder]::new()
foreach($item in $sections.GetEnumerator()){
    [void]$text.Append($item.Key).Append([char]10)
    $rows=[SplitTools.SA2StoryList]::Load((Resolve-Path -LiteralPath (Join-Path $SplitDirectory $item.Value)).Path)
    foreach($entry in $rows){
        $type=[int]$entry.Type
        $character=[int]$entry.Character
        $level=[int]$entry.Level
        $events=$entry.Events -join ','
        [void]$text.Append("$type|$character|$level|$events").Append([char]10)
        $records++
        if($type -eq 0){$actions+=$entry.Events.Count} else {$actions++}
    }
}
$hash=[Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($text.ToString()))
Write-Output ("sections={0} entries={1} dispatches={2} sha256={3}" -f $sections.Count,$records,$actions,[Convert]::ToHexString($hash).ToLowerInvariant())
