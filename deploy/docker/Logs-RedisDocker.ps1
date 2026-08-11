[CmdletBinding()]
param(
    [int]$Tail = 200,
    [switch]$NoFollow
)

$ErrorActionPreference = "Stop"
if (Get-Variable -Name PSNativeCommandUseErrorActionPreference -ErrorAction SilentlyContinue) {
    $PSNativeCommandUseErrorActionPreference = $false
}

. (Join-Path $PSScriptRoot "RedisDocker.Common.ps1")

Write-Step "准备查看本地 Redis Docker 日志"
Ensure-DockerCli
Ensure-DockerDesktopRunning

if (-not (Test-RedisContainerExists)) {
    throw "未找到 Redis 容器，请先执行 deploy\\docker\\Start-RedisDocker.bat"
}

$containerName = Get-RedisContainerName
if ($NoFollow) {
    docker logs --tail $Tail $containerName
}
else {
    docker logs --tail $Tail -f $containerName
}

