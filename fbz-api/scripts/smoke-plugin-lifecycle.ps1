param(
    [string]$BaseUrl = "http://127.0.0.1:18080",
    [string]$Username = "fbz_smoke_admin",
    [string]$Password = "fbz-smoke-password-123",
    [string]$DatabaseUrl = "postgres://fbz:fbz@127.0.0.1:5432/fbz",
    [string]$RedisUrl = "redis://127.0.0.1:6379",
    [string]$PackageDir = "var/plugin-packages",
    [string]$RunId = "",
    [switch]$StartServer
)

$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$packageScript = Join-Path $PSScriptRoot "package-plugin.ps1"

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

function Invoke-FbzJson {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Method,

        [Parameter(Mandatory = $true)]
        [string]$Path,

        [object]$Body = $null,

        [string]$AccessToken = ""
    )

    $headers = @{
        Authorization = "Emby Client=`"FBZ Smoke`", Device=`"Codex Smoke`", DeviceId=`"fbz-smoke-$RunId`", Version=`"1.0.0`""
    }
    if ($AccessToken) {
        $headers["x-emby-token"] = $AccessToken
    }

    $request = @{
        Method = $Method
        Uri = "$BaseUrl$Path"
        Headers = $headers
        TimeoutSec = 15
        ContentType = "application/json"
    }

    if ($null -ne $Body) {
        $request.Body = ($Body | ConvertTo-Json -Depth 64)
    }

    Invoke-RestMethod @request
}

function New-SmokePlugin {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PluginId,

        [Parameter(Mandatory = $true)]
        [string]$PluginDir
    )

    New-Item -ItemType Directory -Force -Path $PluginDir | Out-Null

    $manifest = [ordered]@{
        id = $PluginId
        name = "Smoke Lifecycle Plugin"
        version = "0.1.0"
        apiVersion = "1"
        runtime = "http"
        entrypoint = "http://127.0.0.1:19093/fbz-plugin"
        description = "Generated smoke plugin for validating package approval, enablement, config, and menu boundaries."
        permissions = @(
            [ordered]@{
                key = "admin.menu"
                reason = "Expose a smoke-only admin menu item."
            }
        )
        hooks = @(
            [ordered]@{
                event = "user.login"
                handler = "hooks.observe"
            }
        )
        menu = @(
            [ordered]@{
                key = "$PluginId.root"
                label = "Smoke Plugin"
                path = "/admin/plugins/$PluginId"
                requiredPermission = "admin.menu"
                weight = 9000
            }
        )
        configSchema = @(
            [ordered]@{
                key = "enabled"
                label = "Enabled"
                type = "boolean"
                required = $false
                helpText = "Smoke config flag."
            },
            [ordered]@{
                key = "channel"
                label = "Channel"
                type = "string"
                required = $false
                helpText = "Smoke notification channel."
            }
        )
    }

    $manifestPath = Join-Path $PluginDir "manifest.json"
    $serverPath = Join-Path $PluginDir "server.mjs"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)

    [System.IO.File]::WriteAllText(
        $manifestPath,
        ($manifest | ConvertTo-Json -Depth 64),
        $utf8NoBom
    )
    [System.IO.File]::WriteAllText(
        $serverPath,
        @"
import http from 'node:http'

const server = http.createServer((request, response) => {
  if (request.method !== 'POST' || request.url !== '/fbz-plugin') {
    response.writeHead(404, { 'content-type': 'application/json' })
    response.end(JSON.stringify({ error: 'not_found' }))
    return
  }

  request.resume()
  response.writeHead(200, { 'content-type': 'application/json' })
  response.end(JSON.stringify({ ok: true }))
})

server.listen(19093, '127.0.0.1')
"@,
        $utf8NoBom
    )

    Get-Item -LiteralPath $PluginDir
}

function Test-JsonTrue {
    param([object]$Value)

    if ($Value -eq $true) {
        return $true
    }

    return [string]::Equals(
        [string]$Value,
        "true",
        [System.StringComparison]::OrdinalIgnoreCase
    )
}

if (-not $RunId) {
    $RunId = (Get-Date).ToUniversalTime().ToString("yyyyMMddHHmmss")
}
if ($RunId -notmatch '^[a-z0-9._-]+$') {
    throw "RunId must contain only lowercase letters, digits, dot, underscore, or dash."
}

$baseUri = [System.Uri]$BaseUrl
$port = $baseUri.Port
$pluginId = "dev.fbz.smoke.lifecycle.$RunId"
if ($pluginId.Length -gt 128) {
    throw "Generated plugin id is longer than 128 characters: $pluginId"
}

$packageDirPath = Resolve-FullPath -Path $PackageDir -BasePath $projectRoot
$pluginDir = Join-Path $projectRoot "var/plugin-smoke-src/$RunId"
$serverProcess = $null

try {
    New-SmokePlugin -PluginId $pluginId -PluginDir $pluginDir | Out-Null
    $packageInfo = & $packageScript -PluginDir $pluginDir -OutputDir $packageDirPath -Force | ConvertFrom-Json

    if ($StartServer) {
        $env:FBZ_API_PORT = [string]$port
        $env:DATABASE_URL = $DatabaseUrl
        $env:REDIS_URL = $RedisUrl
        $env:PLUGIN_PACKAGE_DIR = $packageDirPath
        $env:PLUGIN_ALLOW_UNSIGNED = "true"
        $env:FBZ_BOOTSTRAP_ADMIN_USERNAME = $Username
        $env:FBZ_BOOTSTRAP_ADMIN_PASSWORD = $Password
        $env:REDIS_EVENT_STREAMS_ENABLED = "false"
        $env:FBZ_SCAN_WORKER_ENABLED = "false"
        $env:FBZ_SCHEDULER_ENABLED = "false"
        $env:FBZ_TRANSCODE_WORKER_ENABLED = "false"
        $env:FBZ_PROBE_WORKER_ENABLED = "false"
        $env:FBZ_METADATA_WORKER_ENABLED = "false"
        $env:FBZ_PLUGIN_WORKER_ENABLED = "false"
        $env:FBZ_NOTIFICATION_WORKER_ENABLED = "false"

        $outLog = Join-Path $env:TEMP "fbz-api-plugin-smoke-out.log"
        $errLog = Join-Path $env:TEMP "fbz-api-plugin-smoke-err.log"
        Remove-Item -LiteralPath $outLog, $errLog -Force -ErrorAction SilentlyContinue

        $exe = Resolve-Path (Join-Path $projectRoot "target/debug/fbz-api.exe")
        $serverProcess = Start-Process `
            -FilePath $exe.Path `
            -PassThru `
            -WindowStyle Hidden `
            -RedirectStandardOutput $outLog `
            -RedirectStandardError $errLog
    }

    $ready = $null
    for ($i = 0; $i -lt 40; $i++) {
        if ($serverProcess -and $serverProcess.HasExited) {
            $stdout = if (Test-Path -LiteralPath $outLog) { Get-Content -LiteralPath $outLog -Raw } else { "" }
            $stderr = if (Test-Path -LiteralPath $errLog) { Get-Content -LiteralPath $errLog -Raw } else { "" }
            throw "fbz-api exited early with code $($serverProcess.ExitCode)`nSTDOUT:`n$stdout`nSTDERR:`n$stderr"
        }
        try {
            $ready = Invoke-FbzJson -Method "GET" -Path "/ready"
            if ($ready.status -eq "ok") {
                break
            }
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }
    if ($null -eq $ready -or $ready.status -ne "ok") {
        throw "ready endpoint did not become ok."
    }

    $login = Invoke-FbzJson `
        -Method "POST" `
        -Path "/emby/Users/AuthenticateByName" `
        -Body ([ordered]@{
            Username = $Username
            Pw = $Password
        })
    $accessToken = [string]$login.AccessToken
    if (-not $accessToken) {
        throw "AuthenticateByName did not return AccessToken."
    }

    $install = Invoke-FbzJson `
        -Method "POST" `
        -Path "/api/admin/plugins/packages" `
        -AccessToken $accessToken `
        -Body ([ordered]@{
            packagePath = $packageInfo.packagePath
            checksumSha256 = $packageInfo.checksumSha256
            manifest = $packageInfo.manifest
        })
    $installedPluginId = [string]$install.pluginId

    $approve = Invoke-FbzJson `
        -Method "POST" `
        -Path "/api/admin/plugins/packages/$($install.packageId)/approve" `
        -AccessToken $accessToken

    $enable = Invoke-FbzJson `
        -Method "POST" `
        -Path "/api/admin/plugins/$pluginId/enable" `
        -AccessToken $accessToken

    $config = Invoke-FbzJson `
        -Method "PUT" `
        -Path "/api/admin/plugins/$pluginId/config" `
        -AccessToken $accessToken `
        -Body ([ordered]@{
            values = [ordered]@{
                enabled = $true
                channel = "smoke"
            }
        })

    $packageDetail = Invoke-FbzJson `
        -Method "GET" `
        -Path "/api/admin/plugins/packages/$($install.packageId)" `
        -AccessToken $accessToken

    $plugins = @()
    $menuItems = @()
    $listedVisible = $false
    $menuVisible = $false
    $expectedMenuPath = "/admin/plugins/$installedPluginId"
    for ($i = 0; $i -lt 10; $i++) {
        $plugins = @(Invoke-FbzJson -Method "GET" -Path "/api/admin/plugins?limit=500" -AccessToken $accessToken)
        $menuItems = @(Invoke-FbzJson -Method "GET" -Path "/api/admin/plugins/menu-items" -AccessToken $accessToken)
        $seenPluginIds = @($plugins | ForEach-Object { ([string]$_.pluginId).Trim() })
        $seenMenuIds = @($menuItems | ForEach-Object { ([string]$_.pluginId).Trim() })
        $listedVisible = ($seenPluginIds -join "`n").Contains($installedPluginId.Trim())
        $menuVisible = ($seenMenuIds -join "`n").Contains($installedPluginId.Trim())

        if ($listedVisible -and $menuVisible) {
            break
        }

        Start-Sleep -Milliseconds 500
    }

    if ($installedPluginId -ne $pluginId) {
        throw "installed plugin id mismatch."
    }
    if ($approve.approvalStatus -ne "approved") {
        throw "plugin approval did not become approved."
    }
    if (-not (Test-JsonTrue $enable.enabled)) {
        throw "plugin did not become enabled."
    }
    if ($config.values.enabled -ne $true -or $config.values.channel -ne "smoke") {
        throw "plugin config was not persisted."
    }
    if (-not $listedVisible) {
        $seenPluginIds = ($plugins | ForEach-Object { $_.pluginId }) -join ", "
        throw "enabled plugin was not visible in plugin list. Installed plugin id: $installedPluginId. Seen plugin ids: $seenPluginIds"
    }
    if (-not $menuVisible) {
        $seenMenuPluginIds = ($menuItems | ForEach-Object { $_.pluginId }) -join ", "
        throw "active plugin menu item was not visible. Seen menu plugin ids: $seenMenuPluginIds"
    }
    $detailMenuPaths = @($packageDetail.menu | ForEach-Object { ([string]$_.path).Trim() })
    if (-not (($detailMenuPaths -join "`n").Contains($expectedMenuPath))) {
        $seenDetailMenuPaths = $detailMenuPaths -join ", "
        throw "package detail did not expose expected menu path. Seen paths: $seenDetailMenuPaths"
    }
    if ($packageDetail.hooks.Count -lt 1 -or $packageDetail.permissions.Count -lt 1) {
        throw "package detail did not expose normalized hooks and permissions."
    }

    [ordered]@{
        status = "ok"
        pluginId = $pluginId
        packageId = $install.packageId
        packagePath = $packageInfo.packagePath
        approvalStatus = $approve.approvalStatus
        enabled = $enable.enabled
        menuPath = $expectedMenuPath
        configChannel = $config.values.channel
        hookCount = $packageDetail.hooks.Count
        permissionCount = $packageDetail.permissions.Count
    } | ConvertTo-Json -Depth 8
}
finally {
    if ($serverProcess -and -not $serverProcess.HasExited) {
        Stop-Process -Id $serverProcess.Id -Force
        $serverProcess.WaitForExit()
    }
}
