# Orchestrate p2_sse_reconnect_test.dart with a 30s airplane window.
# Poll flutter test stdout (not adb logcat) — redirected logcat is fully buffered on Windows.
param(
    [string]$Device = "emulator-5554",
    [string]$BaseUrl = "http://10.0.2.2:8000"
)

$ErrorActionPreference = "Continue"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$app = Join-Path $repo "mobile\flutter-app"
$adb = Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe"
if (-not (Test-Path $adb)) { $adb = "adb" }
$flutter = "C:\flutter\bin\flutter.bat"
if (-not (Test-Path $flutter)) { $flutter = "flutter" }

$env:PATH = "C:\flutter\bin;$env:PATH"
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\29.0.14206865"

$outFile = Join-Path $env:TEMP "p2_sse_reconn_flutter.out.txt"
$errFile = Join-Path $env:TEMP "p2_sse_reconn_flutter.err.txt"
foreach ($f in @($outFile, $errFile)) {
    if (Test-Path $f) { Remove-Item $f -Force }
}

Write-Host "repo=$repo device=$Device"
& $adb -s $Device shell cmd connectivity airplane-mode disable 2>$null
& $adb -s $Device logcat -c | Out-Null

$proc = Start-Process -FilePath $flutter -WorkingDirectory $app -ArgumentList @(
    "test", "integration_test/p2_sse_reconnect_test.dart",
    "-d", $Device,
    "--dart-define=FMS_TEST_BASE_URL=$BaseUrl"
) -RedirectStandardOutput $outFile -RedirectStandardError $errFile -PassThru -NoNewWindow

$airplaneOn = $false
$airplaneOff = $false

function Combined-Text {
    $a = if (Test-Path $outFile) { Get-Content $outFile -Raw -ErrorAction SilentlyContinue } else { "" }
    $b = if (Test-Path $errFile) { Get-Content $errFile -Raw -ErrorAction SilentlyContinue } else { "" }
    $c = & $adb -s $Device logcat -d -s flutter:I 2>$null | Out-String
    return "$a`n$b`n$c"
}

$deadline = (Get-Date).AddMinutes(25)
try {
    while ((Get-Date) -lt $deadline -and -not $proc.HasExited) {
        $text = Combined-Text
        if (-not $airplaneOn -and $text -match "P2_READY_FOR_AIRPLANE") {
            Write-Host ">>> enable airplane mode (hold 30s)"
            & $adb -s $Device shell cmd connectivity airplane-mode enable
            $airplaneOn = $true
        }
        if ($airplaneOn -and -not $airplaneOff -and $text -match "P2_READY_FOR_RESTORE") {
            Write-Host ">>> disable airplane mode"
            & $adb -s $Device shell cmd connectivity airplane-mode disable
            $airplaneOff = $true
        }
        Start-Sleep -Seconds 1
    }
    if (-not $proc.HasExited) {
        Wait-Process -Id $proc.Id -Timeout 30 -ErrorAction SilentlyContinue
    }
    $output = Combined-Text
    Write-Host $output
    if ($output -match "All tests passed") {
        Write-Host "P2_SSE_RECONNECT_OK"
        exit 0
    }
    Write-Host "P2_SSE_RECONNECT_FAILED"
    exit 1
}
finally {
    if (-not $airplaneOff) {
        & $adb -s $Device shell cmd connectivity airplane-mode disable 2>$null
    }
}
