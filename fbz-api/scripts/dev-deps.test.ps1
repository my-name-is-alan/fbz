$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$scriptPath = Join-Path $projectRoot "scripts/dev-deps.ps1"

function Assert-True {
    param(
        [Parameter(Mandatory = $true)]
        [bool]$Condition,

        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Contains {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Text,

        [Parameter(Mandatory = $true)]
        [string]$Pattern,

        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    if ($Text -notmatch $Pattern) {
        throw $Message
    }
}

Assert-True `
    -Condition (Test-Path -LiteralPath $scriptPath -PathType Leaf) `
    -Message "scripts/dev-deps.ps1 must exist so a fresh machine can start PostgreSQL and Redis without remembering compose flags."

$source = Get-Content -LiteralPath $scriptPath -Raw

Assert-Contains `
    -Text $source `
    -Pattern 'ValidateSet\("start", "stop", "restart", "status"\)' `
    -Message "dev-deps.ps1 must expose start, stop, restart, and status actions."

Assert-Contains `
    -Text $source `
    -Pattern 'docker-compose\.dev\.yml' `
    -Message "dev-deps.ps1 must use the repository development compose file."

Assert-Contains `
    -Text $source `
    -Pattern 'up", "-d", "postgres", "redis"' `
    -Message "dev-deps.ps1 must start both PostgreSQL and Redis services explicitly."

Assert-Contains `
    -Text $source `
    -Pattern 'fbz-api-postgres' `
    -Message "dev-deps.ps1 must wait on the PostgreSQL container health status."

Assert-Contains `
    -Text $source `
    -Pattern 'fbz-api-redis' `
    -Message "dev-deps.ps1 must wait on the Redis container health status."

Assert-Contains `
    -Text $source `
    -Pattern 'docker", "inspect"' `
    -Message "dev-deps.ps1 must inspect Docker health status instead of assuming compose start means ready."

Assert-Contains `
    -Text $source `
    -Pattern 'TimeoutSeconds' `
    -Message "dev-deps.ps1 must let callers bound health waiting on fresh machines."

"dev deps script checks passed"
