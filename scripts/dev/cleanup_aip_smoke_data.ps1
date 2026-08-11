param(
    [string]$BaseUrl = "http://localhost:8080",
    [Parameter(Mandatory = $true)]
    [string]$Token,
    [int]$OlderThanHours = 24,
    [switch]$Execute
)

$ErrorActionPreference = "Stop"

$dryRun = -not $Execute.IsPresent
$confirm = $Execute.IsPresent
$uri = "$BaseUrl/api/v2/ai/execution-readiness/cleanup-smoke?older_than_hours=$OlderThanHours&dry_run=$($dryRun.ToString().ToLowerInvariant())&confirm=$($confirm.ToString().ToLowerInvariant())"

Write-Host "Calling smoke cleanup endpoint: $uri"
$headers = @{ Authorization = "Bearer $Token" }
$response = Invoke-RestMethod -Method Post -Uri $uri -Headers $headers
$response | ConvertTo-Json -Depth 8
