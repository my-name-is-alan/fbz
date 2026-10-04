param(
    [string]$BaseUrl = "http://127.0.0.1:8080",
    [Parameter(Mandatory = $true)][string]$MediaDirectory,
    [string]$LibraryId = ""
)

$ErrorActionPreference = "Stop"
if (-not $env:FBZ_SMOKE_USERNAME -or -not $env:FBZ_SMOKE_PASSWORD) {
    throw "Set FBZ_SMOKE_USERNAME and FBZ_SMOKE_PASSWORD to a local test account."
}
$root = (Resolve-Path -LiteralPath $MediaDirectory).Path
$device = "fbz-playback-smoke-" + [guid]::NewGuid().ToString("N")
$headers = @{ Authorization = "MediaBrowser Client=`"FBZ Smoke`", Device=`"Test`", DeviceId=`"$device`", Version=`"1.0`"" }
function Call-Api([string]$Path, $Body = $null) {
    $options = @{ Uri = "$BaseUrl$Path"; Headers = $headers; TimeoutSec = 20 }
    if ($null -ne $Body) {
        $options.Method = "POST"
        $options.ContentType = "application/json"
        $options.Body = ConvertTo-Json -InputObject $Body -Depth 8 -Compress
    }
    Invoke-RestMethod @options
}

$auth = Call-Api "/emby/Users/AuthenticateByName" @{ Username = $env:FBZ_SMOKE_USERNAME; Pw = $env:FBZ_SMOKE_PASSWORD }
$headers["X-Emby-Token"] = $auth.AccessToken
$playback = $null
$item = $null
try {
    if (-not $LibraryId) {
        $library = Call-Api "/api/admin/libraries" @{ name = "Playback smoke $(Get-Date -Format yyyyMMdd-HHmmss)"; libraryType = "movies"; paths = @($root) }
        $LibraryId = $library.id
    }
    $job = Call-Api "/api/admin/libraries/$LibraryId/scan" @{ reason = "playback_smoke" }
    $deadline = (Get-Date).AddSeconds(90)
    do {
        $detail = Call-Api "/api/admin/jobs/$($job.id)"
        if ($detail.job.status -in @("failed", "cancelled")) { throw "Scan failed: $($detail.job.lastError)" }
        if ($detail.job.status -eq "succeeded") { break }
        if ((Get-Date) -gt $deadline) { throw "Scan timed out. Enable FBZ_SCAN_WORKER_ENABLED." }
        Start-Sleep -Seconds 1
    } while ($true)
    $list = Call-Api "/emby/Users/$($auth.User.Id)/Items?ParentId=$LibraryId&Recursive=true&IncludeItemTypes=Movie&Limit=10"
    $item = $list.Items | Select-Object -First 1
    if (-not $item) { throw "No scanned movie found." }
    $playback = Call-Api "/emby/Items/$($item.Id)/PlaybackInfo" @{ UserId = $auth.User.Id; EnableTranscoding = $false; EnableDirectPlay = $true }
    $source = $playback.MediaSources[0]
    $rangeHeaders = $headers.Clone()
    $rangeHeaders.Range = "bytes=0-1023"
    $response = Invoke-WebRequest -Uri ([uri]::new([uri]$BaseUrl, $source.DirectStreamUrl)) -Headers $rangeHeaders -TimeoutSec 20
    if ($response.StatusCode -ne 206 -or $response.RawContentLength -ne 1024) { throw "Range request failed." }
    if ([string]$response.Headers['Content-Type'] -notmatch 'video/mp4') { throw "Use an MP4 fixture; expected video/mp4." }
    $report = @{ ItemId = $item.Id; MediaSourceId = $source.Id; PlaySessionId = $playback.PlaySessionId; PlayMethod = "DirectPlay"; PositionTicks = 0; IsPaused = $false }
    Call-Api "/emby/Sessions/Playing" $report | Out-Null
    $report.PositionTicks = 250000000
    $report.IsPaused = $true
    Call-Api "/emby/Sessions/Playing/Progress" $report | Out-Null
    $active = @(Call-Api "/api/admin/playback")
    if (-not ($active | Where-Object { $_.Id -eq $item.Id -and $_.PositionTicks -eq 250000000 })) { throw "Playback dashboard did not receive progress." }
    Call-Api "/emby/Sessions/Playing/Stopped" $report | Out-Null
    $saved = Call-Api "/emby/Users/$($auth.User.Id)/Items/$($item.Id)"
    if ($saved.UserData.PlaybackPositionTicks -ne 250000000) { throw "Saved progress differs from report." }
    $resume = Call-Api "/emby/Users/$($auth.User.Id)/Items/Resume?Limit=100"
    if (-not ($resume.Items | Where-Object Id -eq $item.Id)) { throw "Item missing from resume list." }
    [pscustomobject]@{ LibraryId = $LibraryId; Scan = $detail.job.status; Range = $response.StatusCode; ContentType = [string]$response.Headers['Content-Type']; SavedSeconds = 25; Resume = "passed" }
} finally {
    if ($playback -and $item) {
        try { Call-Api "/emby/Sessions/Playing/Stopped" @{ ItemId = $item.Id; MediaSourceId = $playback.MediaSources[0].Id; PlaySessionId = $playback.PlaySessionId; PlayMethod = "DirectPlay"; PositionTicks = 250000000; IsPaused = $true } | Out-Null } catch { Write-Warning "Could not close test playback." }
    }
    try { Call-Api "/emby/Sessions/Logout" @{} | Out-Null } catch { Write-Warning "Could not close test login session." }
}
