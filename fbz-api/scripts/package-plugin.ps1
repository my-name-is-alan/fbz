param(
    [Parameter(Mandatory = $true)]
    [string]$PluginDir,

    [string]$OutputDir = "",

    [switch]$Force
)

$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Resolve-FullPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$BasePath
    )

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $BasePath $Path))
}

function Test-PathInside {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ParentPath,

        [Parameter(Mandatory = $true)]
        [string]$ChildPath
    )

    $comparison = if ($IsLinux -or $IsMacOS) {
        [System.StringComparison]::Ordinal
    }
    else {
        [System.StringComparison]::OrdinalIgnoreCase
    }

    $parent = [System.IO.Path]::GetFullPath($ParentPath).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar
    ) + [System.IO.Path]::DirectorySeparatorChar
    $child = [System.IO.Path]::GetFullPath($ChildPath)

    return $child.StartsWith($parent, $comparison)
}

function ConvertTo-SafePackagePart {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value
    )

    $safe = $Value.Trim() -replace '[^A-Za-z0-9._-]', '-'
    if (-not $safe) {
        throw "Plugin package file name part cannot be empty."
    }
    return $safe
}

$pluginPath = Resolve-FullPath -Path $PluginDir -BasePath $projectRoot
if (-not (Test-Path -LiteralPath $pluginPath -PathType Container)) {
    throw "PluginDir does not exist or is not a directory: $pluginPath"
}

$manifestPath = Join-Path $pluginPath "manifest.json"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Plugin package root must contain manifest.json: $manifestPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
$pluginId = [string]$manifest.id
$packageVersion = [string]$manifest.version
if (-not $pluginId.Trim()) {
    throw "manifest.json must contain a non-empty id."
}
if (-not $packageVersion.Trim()) {
    throw "manifest.json must contain a non-empty version."
}

if (-not $OutputDir) {
    $OutputDir = Join-Path $projectRoot "var/plugin-packages"
}

$outputPath = Resolve-FullPath -Path $OutputDir -BasePath $projectRoot
if (Test-PathInside -ParentPath $pluginPath -ChildPath $outputPath) {
    throw "OutputDir must be outside PluginDir so the package does not include itself."
}

New-Item -ItemType Directory -Force -Path $outputPath | Out-Null

$packageName = "{0}-{1}.zip" -f `
    (ConvertTo-SafePackagePart -Value $pluginId), `
    (ConvertTo-SafePackagePart -Value $packageVersion)
$archivePath = Join-Path $outputPath $packageName
$tempArchivePath = Join-Path $outputPath ".$packageName.tmp.zip"
$stagingPath = Join-Path $outputPath ".$packageName.stage"
$sharedHttpHelperPath = Join-Path $projectRoot "examples/plugins/_shared/fbz-plugin-http.mjs"

if ((Test-Path -LiteralPath $archivePath -PathType Leaf) -and -not $Force) {
    throw "Package already exists: $archivePath. Use -Force to replace it."
}

if (Test-Path -LiteralPath $archivePath) {
    Remove-Item -LiteralPath $archivePath -Force
}
if (Test-Path -LiteralPath $tempArchivePath) {
    Remove-Item -LiteralPath $tempArchivePath -Force
}
if (Test-Path -LiteralPath $stagingPath) {
    if (-not (Test-PathInside -ParentPath $outputPath -ChildPath $stagingPath)) {
        throw "Refusing to remove staging path outside OutputDir: $stagingPath"
    }
    Remove-Item -LiteralPath $stagingPath -Recurse -Force
}

$entries = @(Get-ChildItem -LiteralPath $pluginPath -Force)
if ($entries.Count -eq 0) {
    throw "PluginDir is empty: $pluginPath"
}

try {
    New-Item -ItemType Directory -Force -Path $stagingPath | Out-Null
    foreach ($entry in $entries) {
        Copy-Item -LiteralPath $entry.FullName -Destination $stagingPath -Recurse -Force
    }

    $serverPath = Join-Path $pluginPath "server.mjs"
    $packagedHelperPath = Join-Path $stagingPath "fbz-plugin-http.mjs"
    if (
        (Test-Path -LiteralPath $serverPath -PathType Leaf) -and
        (Test-Path -LiteralPath $sharedHttpHelperPath -PathType Leaf) -and
        -not (Test-Path -LiteralPath $packagedHelperPath)
    ) {
        Copy-Item -LiteralPath $sharedHttpHelperPath -Destination $packagedHelperPath -Force
    }

    $stagedEntries = @(Get-ChildItem -LiteralPath $stagingPath -Force)
    if ($stagedEntries.Count -eq 0) {
        throw "Plugin staging directory is empty: $stagingPath"
    }

    Compress-Archive `
        -Path ($stagedEntries | ForEach-Object { $_.FullName }) `
        -DestinationPath $tempArchivePath `
        -CompressionLevel Optimal `
        -Force
}
finally {
    if (Test-Path -LiteralPath $stagingPath) {
        if (-not (Test-PathInside -ParentPath $outputPath -ChildPath $stagingPath)) {
            throw "Refusing to remove staging path outside OutputDir: $stagingPath"
        }
        Remove-Item -LiteralPath $stagingPath -Recurse -Force
    }
}

Move-Item -LiteralPath $tempArchivePath -Destination $archivePath -Force

$checksumSha256 = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()

[ordered]@{
    packagePath = $packageName
    checksumSha256 = $checksumSha256
    archivePath = $archivePath
    manifestPath = $manifestPath
    manifest = $manifest
} | ConvertTo-Json -Depth 32
