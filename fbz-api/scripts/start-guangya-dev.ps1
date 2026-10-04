param([switch]$Build)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$runtime=Join-Path $root 'var/storage'
New-Item -ItemType Directory -Force -Path $runtime | Out-Null
$keyFile=Join-Path $runtime 'keys.json'
if (!(Test-Path -LiteralPath $keyFile)) {
    @{cipher=[Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(32));plugin=[Convert]::ToHexString([Security.Cryptography.RandomNumberGenerator]::GetBytes(32))} | ConvertTo-Json | Set-Content -LiteralPath $keyFile
}
$keys=Get-Content -LiteralPath $keyFile | ConvertFrom-Json
$env:FBZ_SECRET_KEY=$keys.cipher
$env:FBZ_STORAGE_PLUGIN_KEY=$keys.plugin
$env:FBZ_GUANGYA_PLUGIN_URL='http://127.0.0.1:8098/rpc'
if($Build){Push-Location $root;try{cargo build --bin fbz-api;if($LASTEXITCODE -ne 0){throw 'Rust build failed'}}finally{Pop-Location}}
$binary=Join-Path $root 'target/debug/fbz-api.exe'
if(!(Test-Path -LiteralPath $binary)){throw 'Build fbz-api first, or pass -Build.'}
foreach($port in @(8080,8098)){if(Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue){throw "Port $port is already in use. Keep the running instance or stop it before starting another."}}
$runtimeBinary=Join-Path $runtime 'fbz-api.exe'
Copy-Item -LiteralPath $binary -Destination $runtimeBinary
$plugin=Start-Process -FilePath (Get-Command node).Source -ArgumentList @('plugins/guangya-storage/server.mjs') -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runtime 'plugin.log') -RedirectStandardError (Join-Path $runtime 'plugin-error.log')
try {
    $api=Start-Process -FilePath $runtimeBinary -WorkingDirectory $root -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $runtime 'server.log') -RedirectStandardError (Join-Path $runtime 'server-error.log')
    @{api=$api.Id;plugin=$plugin.Id}|ConvertTo-Json|Set-Content (Join-Path $runtime 'processes.json')
    Write-Host 'FBZ and Guangya plugin started. Open the frontend /admin/storage page.'
    Write-Host 'Keep var/storage/keys.json private and backed up; it is needed to decrypt saved accounts.'
}catch{Stop-Process -Id $plugin.Id -ErrorAction SilentlyContinue;throw}
