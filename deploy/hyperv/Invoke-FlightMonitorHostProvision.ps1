[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$VmRootPath = "D:\HyperV\FlightMonitor",
    [string]$IsoPath = "C:\Users\shezh\Downloads\ubuntu-24.04.3-live-server-amd64.iso",
    [string]$SwitchName = "vSwitch-FlightMonitor",
    [ValidateSet("Internal", "External")]
    [string]$SwitchType = "Internal",
    [string]$NetAdapterName = "以太网",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

function Test-IsAdministrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

if (-not (Test-IsAdministrator)) {
    throw "Please run this script from an elevated PowerShell session."
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$planScript = Join-Path $scriptRoot "Test-FlightMonitorHostCapacity.ps1"
$createScript = Join-Path $scriptRoot "New-FlightMonitorDistributedLab.ps1"

Write-Host "Step 1/4: validating host capacity..."
& $planScript

Write-Host "Step 2/4: ensuring Hyper-V switch $SwitchName exists..."
$existingSwitch = Get-VMSwitch -Name $SwitchName -ErrorAction SilentlyContinue
if (-not $existingSwitch) {
    if ($SwitchType -eq "External") {
        if (-not (Get-NetAdapter -Name $NetAdapterName -ErrorAction SilentlyContinue)) {
            throw "Physical adapter not found: $NetAdapterName"
        }
        if ($PSCmdlet.ShouldProcess($SwitchName, "Create external switch bound to $NetAdapterName")) {
            New-VMSwitch -Name $SwitchName -NetAdapterName $NetAdapterName -AllowManagementOS $true | Out-Null
        }
    }
    else {
        if ($PSCmdlet.ShouldProcess($SwitchName, "Create internal switch")) {
            New-VMSwitch -Name $SwitchName -SwitchType Internal | Out-Null
        }
    }
}
else {
    Write-Host "Switch already exists: $SwitchName"
}

Write-Host "Step 3/4: checking Ubuntu ISO..."
if (-not (Test-Path $IsoPath)) {
    throw "Ubuntu ISO not found: $IsoPath"
}

Write-Host "Step 4/4: creating VM shells..."
& $createScript -SwitchName $SwitchName -VmRootPath $VmRootPath -IsoPath $IsoPath -Force:$Force

Write-Host ""
Write-Host "Host-side provisioning completed."
Write-Host "Next: install Ubuntu on each VM and follow docs/HYPERV_DISTRIBUTED_RUNBOOK.md"
