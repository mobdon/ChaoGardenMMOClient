param(
    [Parameter(Mandatory=$true)][string]$GradlePath,
    [Parameter(Mandatory=$true)][string]$JavaHome,
    [Parameter(Mandatory=$true)][string]$AndroidSdk,
    [switch]$Connected
)
$ErrorActionPreference = 'Stop'
$env:JAVA_HOME = (Resolve-Path -LiteralPath $JavaHome).Path
$env:ANDROID_HOME = (Resolve-Path -LiteralPath $AndroidSdk).Path
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:GRADLE_USER_HOME = Join-Path $PSScriptRoot '.gradle/user-home'
$env:ANDROID_USER_HOME = Join-Path $PSScriptRoot '.android'
$tasks = @('assembleDebug', 'lintDebug', 'testDebugUnitTest', 'assembleDebugAndroidTest')
if ($Connected) { $tasks += 'connectedDebugAndroidTest' }
Push-Location -LiteralPath $PSScriptRoot
try {
    & $GradlePath --no-daemon --console=plain @tasks
    if ($LASTEXITCODE -ne 0) { throw "Android verification failed with exit $LASTEXITCODE" }
    Get-FileHash -LiteralPath '.\app\build\outputs\apk\debug\app-debug.apk' -Algorithm SHA256
} finally { Pop-Location }
