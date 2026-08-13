# Orchestrate offline_sync_test.dart with adb airplane-mode toggles.
# Requires: emulator online, host API on :8000, Flutter on PATH.
param(
    [string]$Device = "emulator-5554",
    [string]$BaseUrl = "http://10.0.2.2:8000"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
if (-not (Test-Path "$repo\mobile\flutter-app")) {
    $repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
}
$app = Join-Path $repo "mobile\flutter-app"
$adb = Join-Path $env:LOCALAPPDATA "Android\Sdk\platform-tools\adb.exe"
if (-not (Test-Path $adb)) { $adb = "adb" }

Write-Host "repo=$repo device=$Device"

# Clear logcat and start watcher
& $adb -s $Device logcat -c | Out-Null
$logFile = Join-Path $env:TEMP "offline_sync_logcat.txt"
if (Test-Path $logFile) { Remove-Item $logFile -Force }

$watcher = Start-Process -FilePath $adb -ArgumentList @(
    "-s", $Device, "logcat", "-v", "time", "flutter:I", "*:S"
) -RedirectStandardOutput $logFile -PassThru -WindowStyle Hidden

$airplaneOn = $false
$airplaneOff = $false
$done = $false

$poll = {
    if (-not (Test-Path $logFile)) { return }
    $text = Get-Content $logFile -Raw -ErrorAction SilentlyContinue
    if (-not $text) { return }
    if (-not $script:airplaneOn -and $text -match "OFFLINE_READY_FOR_AIRPLANE") {
        Write-Host ">>> enable airplane mode"
        & $adb -s $Device shell cmd connectivity airplane-mode enable
        $script:airplaneOn = $true
    }
    if ($script:airplaneOn -and -not $script:airplaneOff -and $text -match "OFFLINE_QUEUED") {
        Write-Host ">>> disable airplane mode"
        Start-Sleep -Seconds 2
        & $adb -s $Device shell cmd connectivity airplane-mode disable
        $script:airplaneOff = $true
    }
    if ($text -match "OFFLINE_SYNC_RESULT") {
        $script:done = $true
    }
}

$env:PATH = "C:\flutter\bin;$env:PATH"
$env:ANDROID_HOME = "$env:LOCALAPPDATA\Android\Sdk"
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\29.0.14206865"

$testJob = Start-Job -ScriptBlock {
    param($app, $Device, $BaseUrl)
    Set-Location $app
    flutter test integration_test/offline_sync_test.dart -d $Device `
        --dart-define=FMS_TEST_BASE_URL=$BaseUrl 2>&1 | Out-String
} -ArgumentList $app, $Device, $BaseUrl

$deadline = (Get-Date).AddMinutes(25)
try {
    while ((Get-Date) -lt $deadline) {
        & $poll
        if ($testJob.State -ne "Running") { break }
        Start-Sleep -Seconds 2
    }
    & $poll
    $output = Receive-Job $testJob -Wait -AutoRemoveJob
    Write-Host $output
    if ($output -match "All tests passed") {
        Write-Host "OFFLINE_ACCEPTANCE_OK"
        exit 0
    }
    Write-Host "OFFLINE_ACCEPTANCE_FAILED"
    exit 1
}
finally {
    if (-not $airplaneOff) {
        & $adb -s $Device shell cmd connectivity airplane-mode disable 2>$null
    }
    if ($watcher -and -not $watcher.HasExited) {
        Stop-Process -Id $watcher.Id -Force -ErrorAction SilentlyContinue
    }
}
