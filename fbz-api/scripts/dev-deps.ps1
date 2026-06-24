param(
    [ValidateSet("start", "stop", "restart", "status")]
    [string]$Action = "start",

    [ValidateRange(1, 3600)]
    [int]$TimeoutSeconds = 90,

    [ValidateRange(1, 60)]
    [int]$PollSeconds = 2,

    [switch]$NoWait
)

$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$composeFile = Join-Path $projectRoot "docker-compose.dev.yml"
$containers = @(
    @{ Name = "PostgreSQL"; Container = "fbz-api-postgres" },
    @{ Name = "Redis"; Container = "fbz-api-redis" }
)

function Invoke-Compose {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $composeArgs = @("compose", "-f", $composeFile) + $Arguments
    Write-Host ("docker " + ($composeArgs -join " "))
    & "docker" @composeArgs
    if ($LASTEXITCODE -ne 0) {
        throw "docker compose command failed with exit code $LASTEXITCODE."
    }
}

function Get-ContainerHealth {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ContainerName
    )

    $dockerInspectPrefix = @("docker", "inspect")
    $dockerCommand = $dockerInspectPrefix[0]
    $inspectCommand = $dockerInspectPrefix[1]
    $output = & $dockerCommand $inspectCommand "-f" "{{.State.Health.Status}}" $ContainerName 2>$null
    if ($LASTEXITCODE -ne 0) {
        return "missing"
    }

    $status = ($output | Select-Object -First 1)
    if (-not $status) {
        return "unknown"
    }

    return $status.Trim()
}

function Wait-DevDepsHealthy {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)

    while ($true) {
        $statuses = foreach ($container in $containers) {
            $health = Get-ContainerHealth -ContainerName $container.Container
            [pscustomobject]@{
                Name = $container.Name
                Container = $container.Container
                Health = $health
            }
        }

        $notHealthy = @($statuses | Where-Object { $_.Health -ne "healthy" })
        if ($notHealthy.Count -eq 0) {
            Write-Host "Development dependencies are healthy."
            return
        }

        if ((Get-Date) -ge $deadline) {
            $summary = ($statuses | ForEach-Object { "$($_.Container)=$($_.Health)" }) -join ", "
            throw "Timed out waiting for development dependencies to become healthy: $summary"
        }

        $summary = ($statuses | ForEach-Object { "$($_.Container)=$($_.Health)" }) -join ", "
        Write-Host "Waiting for development dependencies: $summary"
        Start-Sleep -Seconds $PollSeconds
    }
}

switch ($Action) {
    "start" {
        Invoke-Compose -Arguments @("up", "-d", "postgres", "redis")
        if (-not $NoWait) {
            Wait-DevDepsHealthy
        }
        Invoke-Compose -Arguments @("ps")
    }
    "restart" {
        Invoke-Compose -Arguments @("stop", "postgres", "redis")
        Invoke-Compose -Arguments @("up", "-d", "postgres", "redis")
        if (-not $NoWait) {
            Wait-DevDepsHealthy
        }
        Invoke-Compose -Arguments @("ps")
    }
    "status" {
        Invoke-Compose -Arguments @("ps")
    }
    "stop" {
        Invoke-Compose -Arguments @("stop", "postgres", "redis")
        Invoke-Compose -Arguments @("ps")
    }
}
