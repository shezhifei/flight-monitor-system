[CmdletBinding()]
param(
    [switch]$RemoveVolume
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

. (Join-Path $PSScriptRoot "RedisDocker.Common.ps1")

Write-Step "准备停止本地 Redis Docker 容器"
Ensure-DockerCli
Ensure-DockerCompose
Ensure-DockerDesktopRunning

if (-not (Test-RedisContainerExists)) {
    Write-WarnLine "未发现 Redis 容器，无需停止。"
    exit 0
}

$composeArgs = @("down")
if ($RemoveVolume) {
    $composeArgs += "-v"
}

Invoke-RedisCompose -ComposeArguments $composeArgs | Out-Null

if ($RemoveVolume) {
    Write-Step "Redis 已停止，数据卷已删除。"
}
else {
    Write-Step "Redis 已停止，数据卷已保留。"
}

