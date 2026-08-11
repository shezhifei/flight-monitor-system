param(
    [string]$ApiBaseUrl = "http://127.0.0.1:18080",
    [string]$Username = $(if ($env:FMS_PERF_USERNAME) { $env:FMS_PERF_USERNAME } else { "admin" }),
    [string]$Password = $env:FMS_PERF_PASSWORD,
    [string]$OutputDir = ".tmp\host-qps-full\results",
    [int]$DurationSec = 20,
    [int[]]$Concurrencies = @(10, 50, 100),
    [string]$UserAgent = "fms-qps-client",
    [switch]$PersistAuthArtifacts
)

$ErrorActionPreference = "Stop"

$clientPath = "C:\flight-monitor-system\services\api-server\target\release\qps_load_client.exe"
if (-not (Test-Path $clientPath)) {
    Write-Error "qps_load_client not found at $clientPath. Build first: cargo build --release -p fms-api --bin qps_load_client"
    exit 1
}

if ([string]::IsNullOrWhiteSpace($Password)) {
    Write-Error "Password is required. Pass -Password or set FMS_PERF_PASSWORD; no default credential is used."
    exit 1
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$runDir = Join-Path $OutputDir $timestamp
New-Item -ItemType Directory -Path $runDir -Force | Out-Null

# Login and get token / session secret
$loginUrl = "$ApiBaseUrl/api/v2/auth/login"
$loginBody = @{ username = $Username; password = $Password } | ConvertTo-Json -Compress

try {
    $loginResponse = Invoke-RestMethod -Uri $loginUrl -Method Post `
        -Headers @{ "User-Agent" = $UserAgent } `
        -ContentType "application/json" -Body $loginBody `
        -TimeoutSec 10
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
} catch {
    Write-Error "Login failed: $_"
    exit 1
}

if (-not $token) {
    Write-Error "Login response did not contain access_token"
    exit 1
}

if ($PersistAuthArtifacts) {
    Write-Warning "Persisting bearer token and session secret under $runDir. Protect or delete these files after use."
    $token | Out-File (Join-Path $runDir "admin-token.txt") -Encoding utf8 -NoNewline
    $sessionSecret | Out-File (Join-Path $runDir "admin-session-secret.txt") -Encoding utf8 -NoNewline
}

function Invoke-QpsTest {
    param(
        [string]$Url,
        [string]$Method = "GET",
        [string]$Token,
        [string]$Secret,
        [int]$Concurrency,
        [int]$DurationSec,
        [string]$Label,
        [bool]$Protected = $true
    )

    Write-Host "`n=== $Label | concurrency=$Concurrency | $Url ===" -ForegroundColor Cyan

    # Collect resources before
    $beforeFile = Join-Path $runDir "$Label-c$Concurrency-before.json"
    & "C:\flight-monitor-system\scripts\perf\collect_host_qps_resources.ps1" -OutputFile $beforeFile

    $stdoutFile = Join-Path $runDir "$Label-c$Concurrency-stdout.txt"
    $stderrFile = Join-Path $runDir "$Label-c$Concurrency-stderr.txt"
    $env:RUST_LOG = "warn"

    $argumentList = @(
        "--url", $Url,
        "--method", $Method,
        "--concurrency", $Concurrency,
        "--duration-sec", $DurationSec,
        "--timeout-ms", "5000",
        "--header", "User-Agent: $UserAgent",
        "--header", "Accept: application/json"
    )
    if ($Protected) {
        if (-not $Token) {
            throw "Protected endpoint $Label requires a token"
        }
        if (-not $Secret) {
            throw "Protected endpoint $Label requires a session secret for anti-replay signing"
        }
        $argumentList += @(
            "--header", "Authorization: Bearer $Token",
            "--anti-replay-secret", $Secret
        )
    }

    $previousErrorActionPreference = $ErrorActionPreference
    $previousNativeErrorActionPreference = $null
    $hasNativeErrorActionPreference = Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue
    if ($hasNativeErrorActionPreference) {
        $previousNativeErrorActionPreference = $PSNativeCommandUseErrorActionPreference
        $PSNativeCommandUseErrorActionPreference = $false
    }
    try {
        $ErrorActionPreference = "Continue"
        & $clientPath @argumentList 1> $stdoutFile 2> $stderrFile
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
        if ($hasNativeErrorActionPreference) {
            $PSNativeCommandUseErrorActionPreference = $previousNativeErrorActionPreference
        }
    }

    if ($exitCode -ne 0) {
        Write-Warning "qps_load_client exited with code $exitCode"
    }

    # Extract JSONL line from stdout
    $stdout = Get-Content $stdoutFile -Raw
    $jsonlLine = ($stdout -split "`n") | Where-Object { $_ -match '^jsonl=' } | Select-Object -First 1
    if ($jsonlLine) {
        $jsonPayload = $jsonlLine -replace '^jsonl=', ''
        $jsonPayload | Out-File (Join-Path $runDir "summary.jsonl") -Encoding utf8 -Append
    }

    # Collect resources after
    $afterFile = Join-Path $runDir "$Label-c$Concurrency-after.json"
    & "C:\flight-monitor-system\scripts\perf\collect_host_qps_resources.ps1" -OutputFile $afterFile

    return $stdout
}

# Test matrix
$endpoints = @(
    @{ Url = "$ApiBaseUrl/api/v2/health/ping"; Label = "ping"; Protected = $false }
    @{ Url = "$ApiBaseUrl/api/v2/auth/me"; Label = "auth-me"; Protected = $true }
    @{ Url = "$ApiBaseUrl/api/v2/flights?page=1&page_size=20"; Label = "flights"; Protected = $true }
    @{ Url = "$ApiBaseUrl/api/v2/todos?page=1&size=20"; Label = "todos"; Protected = $true }
    @{ Url = "$ApiBaseUrl/api/v2/notifications/unread-count"; Label = "notifications-unread"; Protected = $true }
)

foreach ($ep in $endpoints) {
    foreach ($c in $Concurrencies) {
        $result = Invoke-QpsTest `
            -Url $ep.Url `
            -Token $token `
            -Secret $sessionSecret `
            -Concurrency $c `
            -DurationSec $DurationSec `
            -Label $ep.Label `
            -Protected $ep.Protected
    }
}

Write-Host "`n=== All tests completed ===" -ForegroundColor Green
Write-Host "Results in: $runDir"
