param(
    [string]$ApiBaseUrl = "https://localhost:18443",
    [string]$Username = $(if ($env:FMS_PERF_USERNAME) { $env:FMS_PERF_USERNAME } else { "admin" }),
    [string]$Password = $env:FMS_PERF_PASSWORD,
    [string]$OutputDir = ".tmp\host-mixed-qps\results",
    [int]$DurationSec = 30,
    [int]$Concurrency = 768,
    [int]$TargetQps = 50000,
    [double]$MaxP99Ms = 100,
    [int]$MaxStackMemoryMb = 3072,
    [double]$MaxErrorRate = 0.01,
    [switch]$ApplyTune,
    [switch]$SkipBuild,
    [switch]$Insecure,
    [switch]$NoGzip
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $repoRoot

if ([string]::IsNullOrWhiteSpace($Password)) {
    Write-Error "Password is required. Pass -Password or set FMS_PERF_PASSWORD."
    exit 1
}

$clientPath = Join-Path $repoRoot "services\api-server\target\release\mixed_qps_client.exe"
if (-not $SkipBuild -or -not (Test-Path $clientPath)) {
    Write-Host "Building mixed_qps_client (release)..."
    Push-Location (Join-Path $repoRoot "services\api-server")
    try {
        cargo build --release -p fms-api --bin mixed_qps_client
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build mixed_qps_client failed"
        }
    } finally {
        Pop-Location
    }
}

if ($ApplyTune) {
    & (Join-Path $repoRoot "scripts\perf\apply_host_perf_profile.ps1") -StackMemoryMb $MaxStackMemoryMb
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$runDir = Join-Path $repoRoot (Join-Path $OutputDir $timestamp)
New-Item -ItemType Directory -Path $runDir -Force | Out-Null

$loginUrl = "$ApiBaseUrl/api/v2/auth/login"
$loginBody = @{ username = $Username; password = $Password } | ConvertTo-Json -Compress
$loginHeaders = @{
    "User-Agent"        = "fms-mixed-qps-client"
    "X-Client-Surface"  = "native"
}
$loginParams = @{
    Uri             = $loginUrl
    Method          = "Post"
    Headers         = $loginHeaders
    ContentType     = "application/json"
    Body            = $loginBody
    TimeoutSec      = 15
}
if ($Insecure -or $ApiBaseUrl.StartsWith("https://")) {
    if ($PSVersionTable.PSVersion.Major -ge 6) {
        $loginParams.SkipCertificateCheck = $true
    }
}

try {
    $loginResponse = Invoke-RestMethod @loginParams
} catch {
    Write-Error "Login failed against $loginUrl : $_"
    exit 1
}

$token = if ($loginResponse.data -and $loginResponse.data.access_token) {
    $loginResponse.data.access_token
} else {
    $loginResponse.access_token
}
$sessionSecret = if ($loginResponse.data -and $loginResponse.data.session_secret) {
    $loginResponse.data.session_secret
} else {
    $loginResponse.session_secret
}
if (-not $token -or -not $sessionSecret) {
    Write-Error "Login response missing access_token or session_secret"
    exit 1
}

$flightIds = @()
try {
    $flightsUrl = "$ApiBaseUrl/api/v2/flights?page=1&page_size=50"
    $flightsParams = @{
        Uri        = $flightsUrl
        Method     = "Get"
        Headers    = @{
            "User-Agent"    = "fms-mixed-qps-client"
            "Authorization" = "Bearer $token"
        }
        TimeoutSec = 15
    }
    if ($Insecure -or $ApiBaseUrl.StartsWith("https://")) {
        if ($PSVersionTable.PSVersion.Major -ge 6) {
            $flightsParams.SkipCertificateCheck = $true
        }
    }
    $flights = Invoke-RestMethod @flightsParams
    $items = @()
    if ($flights.data -and $flights.data.items) {
        $items = $flights.data.items
    } elseif ($flights.data -is [System.Array]) {
        $items = $flights.data
    }
    foreach ($item in $items) {
        if ($item.flight_id) { $flightIds += [string]$item.flight_id }
        elseif ($item.inbound_flight_id) { $flightIds += [string]$item.inbound_flight_id }
        elseif ($item.outbound_flight_id) { $flightIds += [string]$item.outbound_flight_id }
    }
} catch {
    Write-Warning "Could not pre-load flight ids: $_"
}

$beforeFile = Join-Path $runDir "stack-memory-before.json"
$afterFile = Join-Path $runDir "stack-memory-after.json"
& (Join-Path $repoRoot "scripts\perf\collect_host_stack_memory.ps1") -OutputFile $beforeFile | Out-Null

$stdoutFile = Join-Path $runDir "mixed-stdout.txt"
$stderrFile = Join-Path $runDir "mixed-stderr.txt"
$scenario = Join-Path $repoRoot "scripts\perf\scenarios\airport_ops.json"

$argumentList = @(
    "--base-url", $ApiBaseUrl,
    "--scenario", $scenario,
    "--concurrency", "$Concurrency",
    "--duration-sec", "$DurationSec",
    "--timeout-ms", "5000",
    "--token", $token,
    "--anti-replay-secret", $sessionSecret
)
if ($Insecure -or $ApiBaseUrl.StartsWith("https://")) {
    $argumentList += @("--insecure", "true")
}
$argumentList += @("--gzip", $(if ($NoGzip) { "false" } else { "true" }))
foreach ($id in $flightIds) {
    $argumentList += @("--flight-id", $id)
}

Write-Host "Running mixed airport_ops: concurrency=$Concurrency duration=${DurationSec}s url=$ApiBaseUrl"
$previousError = $ErrorActionPreference
$ErrorActionPreference = "Continue"
try {
    & $clientPath @argumentList 1> $stdoutFile 2> $stderrFile
    $exitCode = $LASTEXITCODE
} finally {
    $ErrorActionPreference = $previousError
}

& (Join-Path $repoRoot "scripts\perf\collect_host_stack_memory.ps1") -OutputFile $afterFile | Out-Null

$stdout = Get-Content $stdoutFile -Raw -ErrorAction SilentlyContinue
$jsonlLine = ($stdout -split "`n") | Where-Object { $_ -match '^jsonl=' } | Select-Object -First 1
if (-not $jsonlLine) {
    Write-Error "mixed_qps_client produced no jsonl summary. See $stderrFile"
    exit 1
}
$summary = ($jsonlLine -replace '^jsonl=', '') | ConvertFrom-Json
$summary | ConvertTo-Json -Depth 8 | Out-File (Join-Path $runDir "summary.json") -Encoding utf8

$after = Get-Content $afterFile -Raw | ConvertFrom-Json
$stackMb = [double]$after.total_working_set_mb
$total = [double]$summary.total
$errors = [double]$summary.errors + [double]$summary.non_success
$errorRate = if ($total -gt 0) { $errors / $total } else { 1 }
$qps = [double]$summary.qps
$p99 = [double]$summary.p99_ms

$passQps = $qps -ge $TargetQps
$passP99 = $p99 -le $MaxP99Ms
$passMem = $stackMb -le $MaxStackMemoryMb
$passErr = $errorRate -le $MaxErrorRate
$passed = $passQps -and $passP99 -and $passMem -and $passErr -and ($exitCode -eq 0)

$gate = [ordered]@{
    passed              = $passed
    qps                 = $qps
    target_qps          = $TargetQps
    p99_ms              = $p99
    max_p99_ms          = $MaxP99Ms
    stack_working_set_mb = $stackMb
    max_stack_memory_mb = $MaxStackMemoryMb
    error_rate          = $errorRate
    max_error_rate      = $MaxErrorRate
    gzip                = -not $NoGzip
    gzip_responses      = [double]$summary.gzip_responses
    avg_bytes           = [double]$summary.avg_bytes
    mbps                = [double]$summary.mbps
    client_exit_code    = $exitCode
    run_dir             = $runDir
}
$gate | ConvertTo-Json -Depth 6 | Out-File (Join-Path $runDir "gate.json") -Encoding utf8

Write-Host ""
Write-Host ("QPS {0:N0} / {1}  p99 {2:N1}ms / {3}ms  RSS {4:N0}MB / {5}MB  err {6:P2} / {7:P2}" -f `
    $qps, $TargetQps, $p99, $MaxP99Ms, $stackMb, $MaxStackMemoryMb, $errorRate, $MaxErrorRate)
if ($passed) {
    Write-Host "GATE PASS" -ForegroundColor Green
    exit 0
}
Write-Host "GATE FAIL" -ForegroundColor Red
exit 1
