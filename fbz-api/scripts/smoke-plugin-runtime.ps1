param(
    [string]$BaseUrl = "http://127.0.0.1:18081",
    [int]$PluginPort = 0,
    [string]$Username = "fbz_runtime_smoke_admin",
    [string]$Password = "fbz-runtime-smoke-password-123",
    [string]$DatabaseUrl = "postgres://fbz:fbz@127.0.0.1:5432/fbz",
    [string]$RedisUrl = "redis://127.0.0.1:6379",
    [string]$PackageDir = "var/plugin-packages",
    [string]$RunId = "",
    [string]$PostgresContainer = "fbz-api-postgres",
    [string]$PostgresUser = "fbz",
    [string]$PostgresDb = "fbz",
    [int]$FailFirstAttempts = 0,
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
        Authorization = "Emby Client=`"FBZ Runtime Smoke`", Device=`"Codex Runtime Smoke`", DeviceId=`"fbz-runtime-smoke-$RunId`", Version=`"1.0.0`""
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

function Invoke-PostgresText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Sql
    )

    $output = $Sql | docker exec -i $PostgresContainer psql -U $PostgresUser -d $PostgresDb -At -v ON_ERROR_STOP=1
    if ($LASTEXITCODE -ne 0) {
        throw "psql command failed with exit code $LASTEXITCODE"
    }

    ($output -join "`n").Trim()
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse("127.0.0.1"), 0)
    try {
        $listener.Start()
        return [int]$listener.LocalEndpoint.Port
    }
    finally {
        $listener.Stop()
    }
}

function Resolve-NodeExecutable {
    try {
        $execPath = (& node -p "process.execPath" 2>$null | Select-Object -First 1)
        if ($execPath) {
            $execPath = [string]$execPath.Trim()
            if ($execPath -and (Test-Path -LiteralPath $execPath)) {
                return $execPath
            }
        }
    }
    catch {
    }

    return (Get-Command node).Source
}

function Stop-SmokeProcess {
    param(
        [System.Diagnostics.Process]$Process
    )

    if ($Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        $Process.WaitForExit(5000) | Out-Null
    }
}

function New-RuntimeSmokePlugin {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PluginId,

        [Parameter(Mandatory = $true)]
        [string]$PluginDir,

        [Parameter(Mandatory = $true)]
        [int]$Port,

        [Parameter(Mandatory = $true)]
        [int]$FailFirstAttempts
    )

    New-Item -ItemType Directory -Force -Path $PluginDir | Out-Null
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)

    $manifest = [ordered]@{
        id = $PluginId
        name = "Runtime Smoke Plugin"
        version = "0.1.0"
        apiVersion = "1"
        runtime = "http"
        entrypoint = "http://127.0.0.1:$Port/fbz-plugin"
        description = "Generated smoke plugin for validating HTTP runtime execution and Host API audit."
        permissions = @()
        hooks = @(
            [ordered]@{
                event = "user.login"
                handler = "hooks.runtimeSmoke"
            }
        )
        configSchema = @(
            [ordered]@{
                key = "channel"
                label = "Channel"
                type = "string"
                required = $false
                helpText = "Runtime smoke config value read through Host API."
            }
        )
    }

    [System.IO.File]::WriteAllText(
        (Join-Path $PluginDir "manifest.json"),
        ($manifest | ConvertTo-Json -Depth 64),
        $utf8NoBom
    )

    $serverSource = @'
import fs from 'node:fs/promises'
import http from 'node:http'

const port = Number(process.env.FBZ_PLUGIN_SMOKE_PORT || '__PLUGIN_PORT__')
const logPath = process.env.FBZ_PLUGIN_SMOKE_LOG
const failFirstAttempts = Number(process.env.FBZ_PLUGIN_SMOKE_FAIL_FIRST || '__FAIL_FIRST_ATTEMPTS__')
const attemptsByIdempotencyKey = new Map()

async function appendLog(entry) {
  if (!logPath) return
  await fs.appendFile(logPath, `${JSON.stringify(entry)}\n`, 'utf8')
}

async function readBody(request) {
  const chunks = []
  for await (const chunk of request) chunks.push(chunk)
  return Buffer.concat(chunks).toString('utf8')
}

const server = http.createServer(async (request, response) => {
  try {
    if (request.method === 'GET' && request.url === '/health') {
      response.writeHead(200, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ status: 'ok' }))
      return
    }

    if (request.method !== 'POST' || request.url !== '/fbz-plugin') {
      response.writeHead(404, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ error: 'not_found' }))
      return
    }

    const rawBody = await readBody(request)
    const dispatch = JSON.parse(rawBody)
    const idempotencyKey = request.headers['x-fbz-plugin-idempotency-key'] || 'missing-idempotency-key'
    const attempt = (attemptsByIdempotencyKey.get(idempotencyKey) || 0) + 1
    attemptsByIdempotencyKey.set(idempotencyKey, attempt)

    if (attempt <= failFirstAttempts) {
      await appendLog({
        pluginId: request.headers['x-fbz-plugin-id'],
        idempotencyKey,
        hookEvent: dispatch.hookEvent,
        handler: dispatch.handler,
        aggregateId: dispatch.source?.aggregateId,
        attempt,
        forcedFailure: true
      })
      response.writeHead(500, { 'content-type': 'application/json' })
      response.end(JSON.stringify({ ok: false, forcedFailure: true, attempt }))
      return
    }

    const hostBaseUrl = request.headers['x-fbz-host-base-url']
    const hostToken = request.headers['x-fbz-plugin-token']
    let configStatus = 0
    let configBody = ''

    if (hostBaseUrl && hostToken) {
      const configResponse = await fetch(`${hostBaseUrl}/api/plugin/config`, {
        headers: { 'x-fbz-plugin-token': hostToken }
      })
      configStatus = configResponse.status
      configBody = await configResponse.text()
    }

    await appendLog({
      pluginId: request.headers['x-fbz-plugin-id'],
      idempotencyKey,
      hookEvent: dispatch.hookEvent,
      handler: dispatch.handler,
      aggregateId: dispatch.source?.aggregateId,
      attempt,
      forcedFailure: false,
      configStatus,
      configBody
    })

    response.writeHead(200, { 'content-type': 'application/json' })
    response.end(JSON.stringify({ ok: true, configStatus }))
  } catch (error) {
    await appendLog({ error: String(error?.stack || error) })
    response.writeHead(500, { 'content-type': 'application/json' })
    response.end(JSON.stringify({ error: String(error?.message || error) }))
  }
})

server.listen(port, '127.0.0.1')
'@ -replace "__PLUGIN_PORT__", [string]$Port `
        -replace "__FAIL_FIRST_ATTEMPTS__", [string]$FailFirstAttempts

    [System.IO.File]::WriteAllText(
        (Join-Path $PluginDir "server.mjs"),
        $serverSource,
        $utf8NoBom
    )

    Get-Item -LiteralPath $PluginDir
}

function Wait-HttpOk {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Url,

        [int]$Attempts = 40
    )

    for ($i = 0; $i -lt $Attempts; $i++) {
        try {
            $response = Invoke-RestMethod -Uri $Url -TimeoutSec 2
            if ($response.status -eq "ok") {
                return
            }
        }
        catch {
            Start-Sleep -Milliseconds 500
        }
    }

    throw "HTTP endpoint did not become ready: $Url"
}

if (-not $RunId) {
    $RunId = (Get-Date).ToUniversalTime().ToString("yyyyMMddHHmmss")
}
if ($RunId -notmatch '^[a-z0-9._-]+$') {
    throw "RunId must contain only lowercase letters, digits, dot, underscore, or dash."
}
if ($FailFirstAttempts -lt 0 -or $FailFirstAttempts -gt 3) {
    throw "FailFirstAttempts must be between 0 and 3."
}
if ($PluginPort -le 0) {
    $PluginPort = Get-FreeTcpPort
}

$baseUri = [System.Uri]$BaseUrl
$apiPort = $baseUri.Port
$pluginId = "dev.fbz.smoke.runtime.$RunId"
if ($pluginId.Length -gt 128) {
    throw "Generated plugin id is longer than 128 characters: $pluginId"
}

$packageDirPath = Resolve-FullPath -Path $PackageDir -BasePath $projectRoot
$pluginDir = Join-Path $projectRoot "var/plugin-runtime-smoke-src/$RunId"
$pluginLog = Join-Path $projectRoot "var/plugin-runtime-smoke-src/$RunId/plugin-requests.jsonl"
$apiProcess = $null
$pluginProcess = $null

try {
    New-RuntimeSmokePlugin -PluginId $pluginId -PluginDir $pluginDir -Port $PluginPort -FailFirstAttempts $FailFirstAttempts | Out-Null
    $packageInfo = & $packageScript -PluginDir $pluginDir -OutputDir $packageDirPath -Force | ConvertFrom-Json

    $env:FBZ_PLUGIN_SMOKE_PORT = [string]$PluginPort
    $env:FBZ_PLUGIN_SMOKE_LOG = $pluginLog
    $env:FBZ_PLUGIN_SMOKE_FAIL_FIRST = [string]$FailFirstAttempts
    $pluginOutLog = Join-Path $env:TEMP "fbz-plugin-runtime-smoke-out.log"
    $pluginErrLog = Join-Path $env:TEMP "fbz-plugin-runtime-smoke-err.log"
    Remove-Item -LiteralPath $pluginOutLog, $pluginErrLog -Force -ErrorAction SilentlyContinue
    $nodeExe = Resolve-NodeExecutable
    $pluginProcess = Start-Process `
        -FilePath $nodeExe `
        -ArgumentList @((Join-Path $pluginDir "server.mjs")) `
        -PassThru `
        -WindowStyle Hidden `
        -RedirectStandardOutput $pluginOutLog `
        -RedirectStandardError $pluginErrLog

    Wait-HttpOk -Url "http://127.0.0.1:$PluginPort/health"

    if ($StartServer) {
        $env:FBZ_API_PORT = [string]$apiPort
        $env:PUBLIC_BASE_URL = $BaseUrl
        $env:DATABASE_URL = $DatabaseUrl
        $env:REDIS_URL = $RedisUrl
        $env:PLUGIN_PACKAGE_DIR = $packageDirPath
        $env:PLUGIN_ALLOW_UNSIGNED = "true"
        $env:PLUGIN_HTTP_ALLOWED_HOSTS = "127.0.0.1,localhost"
        $env:PLUGIN_MAX_CONCURRENCY = "8"
        $env:FBZ_BOOTSTRAP_ADMIN_USERNAME = $Username
        $env:FBZ_BOOTSTRAP_ADMIN_PASSWORD = $Password
        $env:REDIS_EVENT_STREAMS_ENABLED = "false"
        $env:FBZ_SCAN_WORKER_ENABLED = "false"
        $env:FBZ_SCHEDULER_ENABLED = "false"
        $env:FBZ_TRANSCODE_WORKER_ENABLED = "false"
        $env:FBZ_PROBE_WORKER_ENABLED = "false"
        $env:FBZ_METADATA_WORKER_ENABLED = "false"
        $env:FBZ_PLUGIN_WORKER_ENABLED = "true"
        $env:FBZ_PLUGIN_WORKER_INTERVAL_SECONDS = "1"
        $env:FBZ_NOTIFICATION_WORKER_ENABLED = "false"

        $apiOutLog = Join-Path $env:TEMP "fbz-api-runtime-smoke-out.log"
        $apiErrLog = Join-Path $env:TEMP "fbz-api-runtime-smoke-err.log"
        Remove-Item -LiteralPath $apiOutLog, $apiErrLog -Force -ErrorAction SilentlyContinue

        $exe = Resolve-Path (Join-Path $projectRoot "target/debug/fbz-api.exe")
        $apiProcess = Start-Process `
            -FilePath $exe.Path `
            -PassThru `
            -WindowStyle Hidden `
            -RedirectStandardOutput $apiOutLog `
            -RedirectStandardError $apiErrLog
    }

    Wait-HttpOk -Url "$BaseUrl/ready"

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
        -Path "/api/admin/plugins/$installedPluginId/enable" `
        -AccessToken $accessToken

    $config = Invoke-FbzJson `
        -Method "PUT" `
        -Path "/api/admin/plugins/$installedPluginId/config" `
        -AccessToken $accessToken `
        -Body ([ordered]@{
            values = [ordered]@{
                channel = "runtime-smoke"
            }
        })

    if ($installedPluginId -ne $pluginId) {
        throw "installed plugin id mismatch."
    }
    if ($approve.approvalStatus -ne "approved") {
        throw "plugin approval did not become approved."
    }
    if ($enable.enabled -ne $true -and [string]$enable.enabled -ne "true") {
        throw "plugin did not become enabled."
    }
    if ($config.values.channel -ne "runtime-smoke") {
        throw "plugin config was not persisted."
    }

    $dispatchSql = @"
with inserted_dispatch as (
insert into event_outbox (
    event_type,
    aggregate_type,
    aggregate_id,
    payload,
    available_at
)
select
    'plugin.hook.dispatch',
    'plugin',
    '$pluginId',
    jsonb_build_object(
        'pluginId', pi.plugin_id,
        'packageId', pkg.public_id::text,
        'hookId', h.id,
        'handler', h.handler,
        'hookEvent', h.event_key,
        'source', jsonb_build_object(
            'aggregateType', 'runtime-smoke',
            'aggregateId', '$RunId',
            'payload', jsonb_build_object('runId', '$RunId')
        )
    ),
    timestamp with time zone '1900-01-01 00:00:00+00'
from plugin_installations pi
join plugin_packages pkg on pkg.id = pi.active_package_id
join plugin_hooks h on h.package_id = pkg.id
where pi.plugin_id = '$pluginId'
  and h.event_key = 'user.login'
  and pi.enabled = true
  and pi.approval_status = 'approved'
  and pkg.package_status = 'approved'
limit 1
returning public_id::text
)
select public_id from inserted_dispatch;
"@
    $dispatchId = Invoke-PostgresText -Sql $dispatchSql
    if (-not $dispatchId) {
        throw "failed to enqueue plugin runtime smoke dispatch."
    }

    $runEvidence = $null
    $expectedAttempts = $FailFirstAttempts + 1
    for ($i = 0; $i -lt 60; $i++) {
        $evidenceSql = @"
select coalesce((
    select jsonb_build_object(
        'runStatus', (
            select latest.status
            from plugin_execution_runs latest
            where latest.outbox_event_public_id = outbox.public_id::text
            order by latest.started_at desc, latest.id desc
            limit 1
        ),
        'responseStatus', (
            select latest.response_status
            from plugin_execution_runs latest
            where latest.outbox_event_public_id = outbox.public_id::text
            order by latest.started_at desc, latest.id desc
            limit 1
        ),
        'responseBody', (
            select latest.response_body
            from plugin_execution_runs latest
            where latest.outbox_event_public_id = outbox.public_id::text
            order by latest.started_at desc, latest.id desc
            limit 1
        ),
        'outboxStatus', outbox.status,
        'outboxAttempts', outbox.attempts,
        'runCount', (
            select count(*)
            from plugin_execution_runs counted
            where counted.outbox_event_public_id = outbox.public_id::text
        ),
        'failedRuns', (
            select count(*)
            from plugin_execution_runs failed
            where failed.outbox_event_public_id = outbox.public_id::text
              and failed.status = 'failed'
        ),
        'succeededRuns', (
            select count(*)
            from plugin_execution_runs succeeded
            where succeeded.outbox_event_public_id = outbox.public_id::text
              and succeeded.status = 'succeeded'
        ),
        'hostApiCalls', (
            select count(*)
            from plugin_host_api_calls calls
            join plugin_execution_runs audited on audited.id = calls.execution_run_id
            where audited.outbox_event_public_id = outbox.public_id::text
              and calls.path = '/api/plugin/config'
              and calls.status_code = 200
        )
    )::text
    from event_outbox outbox
    where outbox.public_id::text = '$dispatchId'
), '{}');
"@
        $rawEvidence = Invoke-PostgresText -Sql $evidenceSql
        if ($rawEvidence -and $rawEvidence -ne "{}") {
            $runEvidence = $rawEvidence | ConvertFrom-Json
            $retryEvidenceSatisfied = $true
            if ($FailFirstAttempts -gt 0) {
                $retryEvidenceSatisfied = `
                    [int]$runEvidence.runCount -ge $expectedAttempts `
                    -and [int]$runEvidence.failedRuns -ge $FailFirstAttempts `
                    -and [int]$runEvidence.outboxAttempts -ge $expectedAttempts
            }
            if (
                $runEvidence.runStatus -eq "succeeded" `
                    -and $runEvidence.outboxStatus -eq "delivered" `
                    -and [int]$runEvidence.hostApiCalls -ge 1 `
                    -and $retryEvidenceSatisfied
            ) {
                break
            }
        }
        Start-Sleep -Milliseconds 1000
    }

    if ($null -eq $runEvidence) {
        throw "plugin runtime dispatch was not processed."
    }
    if ($runEvidence.runStatus -ne "succeeded") {
        throw "plugin runtime run did not succeed: $($runEvidence | ConvertTo-Json -Compress)"
    }
    if ($runEvidence.outboxStatus -ne "delivered") {
        throw "plugin runtime outbox was not delivered: $($runEvidence | ConvertTo-Json -Compress)"
    }
    if ([int]$runEvidence.hostApiCalls -lt 1) {
        throw "plugin runtime did not record Host API config audit: $($runEvidence | ConvertTo-Json -Compress)"
    }
    if ([int]$runEvidence.runCount -lt $expectedAttempts) {
        throw "plugin runtime did not create expected execution runs: $($runEvidence | ConvertTo-Json -Compress)"
    }
    if ($FailFirstAttempts -gt 0) {
        if ([int]$runEvidence.failedRuns -lt $FailFirstAttempts) {
            throw "plugin runtime did not record expected failed runs: $($runEvidence | ConvertTo-Json -Compress)"
        }
        if ([int]$runEvidence.outboxAttempts -lt $expectedAttempts) {
            throw "plugin runtime did not retry the outbox event enough times: $($runEvidence | ConvertTo-Json -Compress)"
        }
    }

    $serverLog = if (Test-Path -LiteralPath $pluginLog) {
        Get-Content -LiteralPath $pluginLog -Raw
    }
    else {
        ""
    }
    if (-not $serverLog.Contains('"hookEvent":"user.login"')) {
        throw "plugin HTTP server did not log the user.login dispatch."
    }
    if (-not $serverLog.Contains('"configStatus":200')) {
        throw "plugin HTTP server did not log a successful Host API config call."
    }
    if ($FailFirstAttempts -gt 0 -and -not $serverLog.Contains('"forcedFailure":true')) {
        throw "plugin HTTP server did not log the forced failure attempt."
    }

    [ordered]@{
        status = "ok"
        pluginId = $pluginId
        packageId = $install.packageId
        dispatchId = $dispatchId
        runStatus = $runEvidence.runStatus
        outboxStatus = $runEvidence.outboxStatus
        outboxAttempts = $runEvidence.outboxAttempts
        runCount = $runEvidence.runCount
        failedRuns = $runEvidence.failedRuns
        succeededRuns = $runEvidence.succeededRuns
        responseStatus = $runEvidence.responseStatus
        hostApiCalls = $runEvidence.hostApiCalls
        pluginLog = $pluginLog
    } | ConvertTo-Json -Depth 8
}
finally {
    Stop-SmokeProcess -Process $apiProcess
    Stop-SmokeProcess -Process $pluginProcess
}
