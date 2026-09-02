param(
    [int]$HttpsPort = 18443,
    [int]$ApiPort = 18080
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = (Resolve-Path (Join-Path $scriptDir "..\..")).Path
$caddyExe = Join-Path $repoRoot ".tools\caddy\caddy.exe"

if (-not (Test-Path $caddyExe)) {
    throw "caddy.exe not found: $caddyExe"
}

$existing = Get-Process -Name "caddy" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($existing) {
    Write-Host "[INFO] Caddy already running (PID: $($existing.Id))"
    exit 0
}

$runtimeDir = Join-Path $repoRoot ".runtime\host-services\caddy"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null

$caddyFile = Join-Path $runtimeDir "Caddyfile"
$stdout = Join-Path $runtimeDir "caddy.stdout.log"
$stderr = Join-Path $runtimeDir "caddy.stderr.log"

@"
https://localhost:$HttpsPort {
    tls internal
    handle /api/* {
        encode {
            gzip 1
            minimum_length 256
        }
        reverse_proxy 127.0.0.1:$ApiPort {
            transport http {
                versions 1.1 2
                keepalive 2m
                keepalive_idle_conns 256
            }
        }
    }
    handle {
        encode zstd gzip
        reverse_proxy 127.0.0.1:$ApiPort
    }
}
"@ | Out-File -LiteralPath $caddyFile -Encoding ascii

$process = Start-Process -FilePath $caddyExe `
    -ArgumentList @("run", "--config", $caddyFile, "--adapter", "caddyfile") `
    -WorkingDirectory $runtimeDir `
    -WindowStyle Hidden `
    -PassThru `
    -RedirectStandardOutput $stdout `
    -RedirectStandardError $stderr

Start-Sleep -Seconds 2

if ($process.HasExited) {
    $tail = if (Test-Path $stderr) { Get-Content -LiteralPath $stderr -Tail 20 | Out-String } else { "" }
    throw "Caddy exited during startup. Log: $tail"
}

Write-Host "[INFO] Caddy started (PID: $($process.Id), https://localhost:$HttpsPort -> http://127.0.0.1:$ApiPort)"
