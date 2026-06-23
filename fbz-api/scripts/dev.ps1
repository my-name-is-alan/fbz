param(
    [string]$HostName = "",
    [int]$Port = 8080
)

$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

if (-not $HostName -and $env:FBZ_API_HOST) {
    $HostName = $env:FBZ_API_HOST
}

if (-not $HostName) {
    $HostName = "127.0.0.1"
}

if ($env:FBZ_API_PORT) {
    $Port = [int]$env:FBZ_API_PORT
}

$env:FBZ_API_HOST = $HostName
$env:FBZ_API_PORT = [string]$Port

$cargoWatch = Get-Command "cargo-watch" -ErrorAction SilentlyContinue
if ($cargoWatch) {
    Write-Host "Using cargo-watch for hot reload."
    cargo watch -c -w src -w Cargo.toml -x run
    exit $LASTEXITCODE
}

Write-Host "cargo-watch not found; using PowerShell polling hot reload."

$script:apiProcess = $null

function Get-SourceStamp {
    $files = @()
    $files += Get-Item -LiteralPath (Join-Path $projectRoot "Cargo.toml")
    $files += Get-ChildItem -LiteralPath (Join-Path $projectRoot "src") -Recurse -File

    return ($files | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1).LastWriteTimeUtc.Ticks
}

function Stop-Api {
    if ($null -ne $script:apiProcess -and -not $script:apiProcess.HasExited) {
        Stop-Process -Id $script:apiProcess.Id -Force
        $script:apiProcess.WaitForExit()
    }
}

function Start-Api {
    Write-Host "Starting fbz-api at http://$env:FBZ_API_HOST`:$env:FBZ_API_PORT"
    $script:apiProcess = Start-Process `
        -FilePath "cargo" `
        -ArgumentList @("run") `
        -WorkingDirectory $projectRoot `
        -NoNewWindow `
        -PassThru
}

function Restart-Api {
    Write-Host "Change detected; restarting fbz-api."
    Stop-Api
    Start-Api
}

$lastStamp = Get-SourceStamp
Start-Api

try {
    while ($true) {
        Start-Sleep -Milliseconds 800

        $currentStamp = Get-SourceStamp
        if ($currentStamp -ne $lastStamp) {
            $lastStamp = $currentStamp
            Restart-Api
            continue
        }

        if ($null -ne $script:apiProcess -and $script:apiProcess.HasExited) {
            Write-Host "fbz-api exited with code $($script:apiProcess.ExitCode). Waiting for changes..."
            $script:apiProcess = $null
        }
    }
}
finally {
    Stop-Api
}
