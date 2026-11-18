[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ServerRoot,
    [Parameter(Mandatory = $true)][string]$RustSetup,
    [Parameter(Mandatory = $true)][string]$GradlePath,
    [Parameter(Mandatory = $true)][string]$JavaHome,
    [Parameter(Mandatory = $true)][string]$AndroidSdk
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$clientRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$serverPath = (Resolve-Path -LiteralPath $ServerRoot).Path
$env:JAVA_HOME = (Resolve-Path -LiteralPath $JavaHome).Path
$env:ANDROID_HOME = (Resolve-Path -LiteralPath $AndroidSdk).Path
$env:ANDROID_SDK_ROOT = $env:ANDROID_HOME
$env:GRADLE_USER_HOME = Join-Path $clientRoot 'android/.gradle/user-home'
$env:ANDROID_USER_HOME = Join-Path $clientRoot 'android/.android'
. $RustSetup
$runPath = Join-Path $clientRoot ('.local/verification/rust-roundtrip/' + [guid]::NewGuid().ToString('N'))
$dataPath = Join-Path $runPath 'data'
New-Item -ItemType Directory -Path $dataPath -Force | Out-Null
$port = 4317
$portProbe = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, $port)
try { $portProbe.Start() } finally { $portProbe.Stop() }
$buildPath = Join-Path $runPath 'rust-build'
$sourceCommit = (& git -C $serverPath rev-parse HEAD).Trim()
& git -C $serverPath diff --quiet HEAD -- src crates Cargo.toml Cargo.lock
if ($LASTEXITCODE -ne 0) { throw 'Rust service source must match its recorded commit before this integration build.' }
& cargo build --offline --locked --manifest-path (Join-Path $serverPath 'Cargo.toml') --target-dir $buildPath --bin chao-game-server
if ($LASTEXITCODE -ne 0) { throw 'Isolated real Rust service build failed.' }
if ((& git -C $serverPath rev-parse HEAD).Trim() -ne $sourceCommit) { throw 'Rust source commit changed during the isolated build.' }
& git -C $serverPath diff --quiet HEAD -- src crates Cargo.toml Cargo.lock
if ($LASTEXITCODE -ne 0) { throw 'Rust service source changed during the isolated build.' }
$binaryPath = Join-Path $buildPath 'debug/chao-game-server.exe'
$certificatePath = Join-Path $runPath 'localhost-test.p12'
& (Join-Path $env:JAVA_HOME 'bin/keytool.exe') -genkeypair -alias localhost -keyalg RSA -keysize 2048 -validity 1 -dname 'CN=localhost' -ext 'SAN=dns:localhost,ip:127.0.0.1' -storetype PKCS12 -keystore $certificatePath -storepass test-only-widget-cert -keypass test-only-widget-cert -noprompt
if ($LASTEXITCODE -ne 0) { throw 'Generated localhost test certificate failed.' }
$proxyClasses = Join-Path $runPath 'proxy-classes'
New-Item -ItemType Directory -Path $proxyClasses -Force | Out-Null
& (Join-Path $env:JAVA_HOME 'bin/javac.exe') --add-modules jdk.httpserver -d $proxyClasses (Join-Path $PSScriptRoot 'LoopbackHttpsProxy.java')
if ($LASTEXITCODE -ne 0) { throw 'Test-only JDK HTTPS bridge compilation failed.' }
$configurationPath = Join-Path $runPath 'test-config.json'
$configuration = @{
    backendPort = $port
    keyStore = $certificatePath
    fixture = (Join-Path $runPath 'private-fixture.json')
    evidence = (Join-Path $runPath 'exercise-evidence.json')
    phase = 'exercise'
}
$savedConfig = $env:TINY_WIDGET_RUST_TEST_CONFIG
$env:TINY_WIDGET_RUST_TEST_CONFIG = $configurationPath
$script:serviceProcess = $null
$script:stdoutTask = $null
$script:stderrTask = $null
$script:proxyProcess = $null

function Start-TestHttpsProxy {
    $launch = [Diagnostics.ProcessStartInfo]::new()
    $launch.FileName = Join-Path $env:JAVA_HOME 'bin/java.exe'
    $launch.UseShellExecute = $false
    $launch.CreateNoWindow = $true
    $launch.RedirectStandardInput = $true
    $launch.RedirectStandardOutput = $true
    $launch.RedirectStandardError = $true
    foreach ($argument in @('--add-modules', 'jdk.httpserver', '-cp', $proxyClasses, 'LoopbackHttpsProxy', $certificatePath, [string]$port)) {
        $launch.ArgumentList.Add($argument)
    }
    $script:proxyProcess = [Diagnostics.Process]::Start($launch)
    $readiness = $script:proxyProcess.StandardOutput.ReadLineAsync()
    if (-not $readiness.Wait(10000)) { throw 'Test-only HTTPS bridge did not become ready.' }
    $line = $readiness.Result
    if ($line -notmatch '^READY ([0-9]{1,5})$') { throw 'Test-only HTTPS bridge returned invalid readiness.' }
    $configuration.proxyOrigin = 'https://localhost:' + $Matches[1]
}

function Stop-TestHttpsProxy {
    if ($null -ne $script:proxyProcess) {
        if (-not $script:proxyProcess.HasExited) {
            $script:proxyProcess.StandardInput.WriteLine('shutdown')
            $script:proxyProcess.StandardInput.Flush()
            if (-not $script:proxyProcess.WaitForExit(10000)) { $script:proxyProcess.Kill($true) }
        }
        $script:proxyProcess.Dispose()
        $script:proxyProcess = $null
    }
}

function Start-TestRustService {
    $launch = [Diagnostics.ProcessStartInfo]::new()
    $launch.FileName = $binaryPath
    $launch.WorkingDirectory = $runPath
    $launch.UseShellExecute = $false
    $launch.CreateNoWindow = $true
    $launch.RedirectStandardInput = $true
    $launch.RedirectStandardOutput = $true
    $launch.RedirectStandardError = $true
    $launch.Environment['CHAO_DATA_DIR'] = $dataPath
    $launch.Environment['GAME_HOST'] = '127.0.0.1'
    $launch.Environment['GAME_PORT'] = [string]$port
    $launch.Environment['CHAO_OWNER_BOOTSTRAP'] = '0'
    $launch.Environment['CHAO_ALLOWED_ORIGINS'] = ''
    $launch.Environment['CHAO_SHUTDOWN_STDIN'] = '1'
    $script:serviceProcess = [Diagnostics.Process]::Start($launch)
    $script:stdoutTask = $script:serviceProcess.StandardOutput.ReadToEndAsync()
    $script:stderrTask = $script:serviceProcess.StandardError.ReadToEndAsync()
    $ready = $false
    for ($attempt = 0; $attempt -lt 80; $attempt++) {
        if ($script:serviceProcess.HasExited) { throw 'Isolated Rust service exited before readiness.' }
        try {
            $status = Invoke-RestMethod -Uri "http://127.0.0.1:$port/api/status" -TimeoutSec 1 -NoProxy
            if ($status.accounts.version -eq 2) { $ready = $true; break }
        } catch { }
        Start-Sleep -Milliseconds 100
    }
    if (-not $ready) { throw 'Isolated Rust accounts-v2 service did not become ready.' }
}

function Stop-TestRustService {
    if ($null -ne $script:serviceProcess) {
        if (-not $script:serviceProcess.HasExited) {
            $script:serviceProcess.StandardInput.WriteLine('shutdown')
            $script:serviceProcess.StandardInput.Flush()
            if (-not $script:serviceProcess.WaitForExit(10000)) {
                $script:serviceProcess.Kill($true)
                throw 'Isolated Rust service did not shut down cleanly.'
            }
        }
        if ($script:serviceProcess.ExitCode -ne 0) { throw 'Isolated Rust service returned a failure exit code.' }
        $script:serviceProcess.Dispose()
        $script:serviceProcess = $null
    }
}

function Run-NativeGatewayTest {
    [IO.File]::WriteAllText($configurationPath, ($configuration | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    Push-Location -LiteralPath (Join-Path $clientRoot 'android')
    try {
        & $GradlePath --no-daemon --console=plain testDebugUnitTest --tests garden.tiny.widget.RustServiceIntegrationTest --rerun-tasks
        if ($LASTEXITCODE -ne 0) { throw 'Actual Java Gateway to Rust service test failed.' }
    } finally { Pop-Location }
}

try {
    Start-TestRustService
    Start-TestHttpsProxy
    Run-NativeGatewayTest
    Stop-TestRustService
    $configuration.phase = 'restart'
    $configuration.evidence = Join-Path $runPath 'restart-evidence.json'
    Start-TestRustService
    Run-NativeGatewayTest
    Stop-TestRustService
    $receipt = @{
        sourceCommit = $sourceCommit
        binarySha256 = (Get-FileHash -LiteralPath $binaryPath -Algorithm SHA256).Hash.ToLowerInvariant()
        backend = 'real Rust accounts-v2 service, isolated loopback port 4317'
        transport = 'production Android Gateway via test-only localhost HTTPS proxy; default hostname verification retained'
        exercise = (Get-Content -LiteralPath (Join-Path $runPath 'exercise-evidence.json') -Raw | ConvertFrom-Json)
        restart = (Get-Content -LiteralPath (Join-Path $runPath 'restart-evidence.json') -Raw | ConvertFrom-Json)
    }
    $receiptPath = Join-Path $runPath 'roundtrip-evidence.json'
    [IO.File]::WriteAllText($receiptPath, ($receipt | ConvertTo-Json -Depth 10), [Text.UTF8Encoding]::new($false))
    Write-Output "Real native adapter roundtrip passed. Sanitized evidence: $receiptPath"
} finally {
    try { Stop-TestRustService } finally {
        try { Stop-TestHttpsProxy } finally { $env:TINY_WIDGET_RUST_TEST_CONFIG = $savedConfig }
    }
}
