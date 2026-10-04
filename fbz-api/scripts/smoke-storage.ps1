param([string]$BaseUrl = "http://127.0.0.1:8081")
$ErrorActionPreference = 'Stop'
if (!$env:FBZ_SMOKE_USERNAME -or !$env:FBZ_SMOKE_PASSWORD) { throw 'Set local test account credentials in FBZ_SMOKE_USERNAME/PASSWORD' }
$headers = @{ Authorization = 'Emby Client="Storage Smoke", Device="Test", DeviceId="storage-smoke", Version="1"' }
function Api([string]$Path, $Body = $null) {
    $options = @{ Uri = "$BaseUrl$Path"; Headers = $headers; TimeoutSec = 65 }
    if ($null -ne $Body) { $options.Method='POST'; $options.ContentType='application/json'; $options.Body=ConvertTo-Json -InputObject $Body -Depth 8 -Compress }
    Invoke-RestMethod @options
}
$auth = Api '/emby/Users/AuthenticateByName' @{Username=$env:FBZ_SMOKE_USERNAME;Pw=$env:FBZ_SMOKE_PASSWORD}
$headers['X-Emby-Token']=$auth.AccessToken
$account=Api '/api/admin/storage/accounts' @{name='Offline fixture';qps=5}
$login=Api "/api/admin/storage/accounts/$($account.id)/login" @{}
if (!$login.attemptId -or !$login.url -or $login.device_code -or $login.access_token) { throw 'Login attempt leaked or omitted fields' }
Start-Sleep -Seconds 3
$poll=Api "/api/admin/storage/accounts/$($account.id)/poll" @{attemptId=$login.attemptId}
if (!$poll.authenticated) { throw 'Fixture did not authenticate' }
$directory=Api "/api/admin/storage/accounts/$($account.id)/directories?parentId=&page=0"
if ($directory.entries[0].id -ne 'movies') { throw 'Directory mapping failed' }
$mount=Api '/api/admin/storage/mounts' @{accountId=$account.id;rootId='movies';name="Storage fixture $([guid]::NewGuid().ToString('N').Substring(0,8))";displayPath='/已刮削电影';libraryType='movies'}
Api "/api/admin/storage/mounts/$($mount.id)/scan" @{} | Out-Null
$deadline=(Get-Date).AddSeconds(120)
do {
    $overview=Api '/api/admin/storage'
    $state=$overview.mounts | Where-Object id -eq $mount.id
    if ($state.status -eq 'failed') { throw "Scan failed: $($state.lastError)" }
    if ($state.status -eq 'idle') { break }
    if ((Get-Date) -ge $deadline) { throw 'Scan timed out' }
    Start-Sleep -Seconds 2
} while ($true)
$items=Api "/emby/Users/$($auth.User.Id)/Items?ParentId=$($state.libraryId)&Recursive=true"
$item=$items.Items | Where-Object Name -eq '光鸭离线验证影片' | Select-Object -First 1
if (!$item -or $item.RunTimeTicks -ne 1200000000) { throw 'NFO title or runtime was not imported' }
$info=Api "/emby/Items/$($item.Id)/PlaybackInfo" @{EnableDirectPlay=$true;EnableTranscoding=$false}
$source=$info.MediaSources[0]
$handler=[Net.Http.HttpClientHandler]::new();$handler.AllowAutoRedirect=$false
$client=[Net.Http.HttpClient]::new($handler)
try {
    $stream=[uri]::new([uri]$BaseUrl,$source.DirectStreamUrl)
    $response=$client.GetAsync($stream).GetAwaiter().GetResult()
    if ([int]$response.StatusCode -ne 302 -or $response.Headers.Location.Host -ne 'fixture.invalid') { throw 'Expected authenticated CDN redirect' }
    $denied=$client.GetAsync("$BaseUrl/emby/Videos/$($item.Id)/stream").GetAwaiter().GetResult()
    if ([int]$denied.StatusCode -ne 401) { throw 'Unauthenticated stream was not denied' }
} finally { $client.Dispose();$handler.Dispose() }
$image=Invoke-WebRequest -Uri "$BaseUrl/emby/Items/$($item.Id)/Images/Primary" -Headers $headers
if ($image.StatusCode -ne 200) { throw 'Artwork cache not served' }
$subtitle=Invoke-WebRequest -Uri "$BaseUrl/emby/Videos/$($item.Id)/$($source.Id)/Subtitles/1000/Stream.srt" -Headers $headers
$subtitleText=if($subtitle.Content -is [byte[]]){[Text.Encoding]::UTF8.GetString($subtitle.Content)}else{[string]$subtitle.Content}
if ($subtitle.StatusCode -ne 200 -or $subtitleText -notmatch '00:00:01') { throw 'Cloud subtitle not served' }
$report=@{ItemId=$item.Id;MediaSourceId=$source.Id;PlaySessionId='storage-smoke-play';PositionTicks=0;PlayMethod='DirectPlay'}
Api '/emby/Sessions/Playing' $report | Out-Null
$report.PositionTicks=250000000
Api '/emby/Sessions/Playing/Progress' $report | Out-Null
Api '/emby/Sessions/Playing/Stopped' $report | Out-Null
$saved=Api "/emby/Users/$($auth.User.Id)/Items/$($item.Id)"
if ($saved.UserData.PlaybackPositionTicks -ne 250000000) { throw 'Cloud playback progress not persisted' }
[pscustomobject]@{Account=$account.id;Mount=$mount.id;Item=$item.Id;MediaSource=$source.Id;Scanned=$state.scanned;Imported=$state.imported;Nfo='passed';Artwork='passed';Subtitle='passed';Redirect=302;Unauthorized=401;ProgressSeconds=25}
