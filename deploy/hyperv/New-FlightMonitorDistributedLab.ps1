[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)]
    [string]$SwitchName,
    [Parameter(Mandatory = $true)]
    [string]$VmRootPath,
    [string]$PlanPath = "deploy/hyperv/flight-monitor-hyperv-plan.json",
    [string]$IsoPath = "",
    [switch]$Force
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $PlanPath)) {
    throw "Plan file not found: $PlanPath"
}

$plan = Get-Content $PlanPath -Raw | ConvertFrom-Json

if (-not (Get-VMSwitch -Name $SwitchName -ErrorAction SilentlyContinue)) {
    throw "Hyper-V switch not found: $SwitchName"
}

$resolvedVmRoot = Resolve-Path -LiteralPath $VmRootPath -ErrorAction SilentlyContinue
if (-not $resolvedVmRoot) {
    New-Item -ItemType Directory -Path $VmRootPath -Force | Out-Null
    $resolvedVmRoot = Resolve-Path -LiteralPath $VmRootPath
}

foreach ($vm in $plan.vms) {
    $vmName = [string]$vm.name
    $vmPath = Join-Path $resolvedVmRoot $vmName
    $vhdPath = Join-Path $vmPath "$vmName.vhdx"
    $memoryBytes = [int64]$vm.memory_gb * 1GB
    $diskBytes = [int64]$vm.disk_gb * 1GB
    $vcpu = [int]$vm.vcpu

    $existingVm = Get-VM -Name $vmName -ErrorAction SilentlyContinue
    if ($existingVm) {
        if (-not $Force) {
            Write-Host "Skip existing VM: $vmName"
            continue
        }
        if ($PSCmdlet.ShouldProcess($vmName, "Remove existing VM and recreate")) {
            Stop-VM -Name $vmName -TurnOff -Force -ErrorAction SilentlyContinue | Out-Null
            Remove-VM -Name $vmName -Force
        }
    }

    if (Test-Path $vmPath) {
        if (-not $Force) {
            throw "VM path already exists for $vmName: $vmPath. Use -Force to recreate."
        }
        Remove-Item -LiteralPath $vmPath -Recurse -Force
    }

    if ($PSCmdlet.ShouldProcess($vmName, "Create VM")) {
        New-Item -ItemType Directory -Path $vmPath -Force | Out-Null
        New-VHD -Path $vhdPath -SizeBytes $diskBytes -Dynamic | Out-Null
        New-VM `
            -Name $vmName `
            -Generation 2 `
            -MemoryStartupBytes $memoryBytes `
            -VHDPath $vhdPath `
            -Path $vmPath `
            -SwitchName $SwitchName | Out-Null

        Set-VMProcessor -VMName $vmName -Count $vcpu
        Set-VMMemory -VMName $vmName -DynamicMemoryEnabled $false
        Set-VM -Name $vmName -AutomaticStartAction StartIfRunning -AutomaticStopAction ShutDown -CheckpointType Disabled
        Set-VMFirmware -VMName $vmName -EnableSecureBoot On -SecureBootTemplate "MicrosoftUEFICertificateAuthority"

        if ($IsoPath) {
            Add-VMDvdDrive -VMName $vmName -Path $IsoPath | Out-Null
        }

        Write-Host "Created $vmName role=$($vm.role) vcpu=$vcpu memory_gb=$($vm.memory_gb) disk_gb=$($vm.disk_gb) ip=$($vm.ip)"
    }
}
