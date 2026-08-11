[CmdletBinding()]
param(
    [string]$PlanPath = "deploy/hyperv/flight-monitor-hyperv-plan.json",
    [string]$DriveLetter = "C"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $PlanPath)) {
    throw "Plan file not found: $PlanPath"
}

$plan = Get-Content $PlanPath -Raw | ConvertFrom-Json

$computer = Get-CimInstance Win32_ComputerSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$volume = Get-Volume -DriveLetter $DriveLetter

$hostLogical = [int]$computer.NumberOfLogicalProcessors
$hostMemoryGb = [math]::Floor([double]$computer.TotalPhysicalMemory / 1GB)
$freeDiskGb = [math]::Floor([double]$volume.SizeRemaining / 1GB)

$requiredLogical = [int]$plan.totals.allocated_vcpu + [int]$plan.planning_assumptions.host_reserve_logical_processors
$requiredMemoryGb = [int]$plan.totals.allocated_memory_gb + [int]$plan.planning_assumptions.host_reserve_memory_gb
$requiredDiskGb = [int]$plan.totals.allocated_disk_gb

$cpuOk = $hostLogical -ge $requiredLogical
$memoryOk = $hostMemoryGb -ge $requiredMemoryGb
$diskOk = $freeDiskGb -ge $requiredDiskGb

[pscustomobject]@{
    HostCpuModel = $cpu.Name
    HostLogicalProcessors = $hostLogical
    RequiredLogicalProcessors = $requiredLogical
    CpuCapacityOk = $cpuOk
    HostMemoryGb = $hostMemoryGb
    RequiredMemoryGb = $requiredMemoryGb
    MemoryCapacityOk = $memoryOk
    FreeDiskGb = $freeDiskGb
    RequiredDiskGb = $requiredDiskGb
    DiskCapacityOk = $diskOk
    Recommended = ($cpuOk -and $memoryOk -and $diskOk)
} | Format-List
