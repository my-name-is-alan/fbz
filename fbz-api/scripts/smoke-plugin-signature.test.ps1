$ErrorActionPreference = "Stop"

$projectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$scripts = @(
    (Join-Path $projectRoot "scripts/smoke-plugin-lifecycle.ps1"),
    (Join-Path $projectRoot "scripts/smoke-plugin-runtime.ps1")
)

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

foreach ($script in $scripts) {
    $source = Get-Content -LiteralPath $script -Raw
    $name = Split-Path -Leaf $script

    Assert-Contains `
        -Text $source `
        -Pattern '\[switch\]\$SignedPackage' `
        -Message "$name must expose -SignedPackage."

    Assert-Contains `
        -Text $source `
        -Pattern 'function Invoke-PluginPackageSigner' `
        -Message "$name must sign packaged plugin zips through the shared signer."

    Assert-Contains `
        -Text $source `
        -Pattern 'PLUGIN_ALLOW_UNSIGNED = if \(\$SignedPackage\)' `
        -Message "$name must keep unsigned packages disabled when signed smoke is requested."

    Assert-Contains `
        -Text $source `
        -Pattern 'PLUGIN_TRUSTED_SIGNATURE_KEYS' `
        -Message "$name must configure the public signing key for signed smoke."

    Assert-Contains `
        -Text $source `
        -Pattern '\$installBody\["signature"\] = \$signatureInfo\.signature' `
        -Message "$name must include the generated signature in the package install request."

    Assert-Contains `
        -Text $source `
        -Pattern 'signaturePresent' `
        -Message "$name must expose signed-package evidence in its output."
}

$runtimeSource = Get-Content -LiteralPath (Join-Path $projectRoot "scripts/smoke-plugin-runtime.ps1") -Raw
$lifecycleSource = Get-Content -LiteralPath (Join-Path $projectRoot "scripts/smoke-plugin-lifecycle.ps1") -Raw

Assert-Contains `
    -Text $lifecycleSource `
    -Pattern '\[switch\]\$IncludeSchedule' `
    -Message "smoke-plugin-lifecycle.ps1 must expose -IncludeSchedule."

Assert-Contains `
    -Text $lifecycleSource `
    -Pattern 'scheduler\.register' `
    -Message "lifecycle smoke plugin must request scheduler.register when schedule coverage is enabled."

Assert-Contains `
    -Text $lifecycleSource `
    -Pattern 'packageDetail\.schedules' `
    -Message "lifecycle smoke must verify schedule definitions on package detail."

Assert-Contains `
    -Text $lifecycleSource `
    -Pattern '/api/admin/scheduled-tasks\?ownerType=plugin' `
    -Message "lifecycle smoke must query plugin-owned scheduled tasks."

Assert-Contains `
    -Text $lifecycleSource `
    -Pattern 'scheduleVisible' `
    -Message "lifecycle smoke must expose schedule visibility evidence."

Assert-Contains `
    -Text $lifecycleSource `
    -Pattern 'scheduleCount' `
    -Message "lifecycle smoke must expose schedule count evidence."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '\[switch\]\$ExhaustHostApiBudget' `
    -Message "smoke-plugin-runtime.ps1 must expose -ExhaustHostApiBudget."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '\[switch\]\$DeliverNotification' `
    -Message "smoke-plugin-runtime.ps1 must expose -DeliverNotification."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '\[switch\]\$DispatchSchedule' `
    -Message "smoke-plugin-runtime.ps1 must expose -DispatchSchedule."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'FBZ_PLUGIN_SMOKE_EXHAUST_HOST_API_BUDGET' `
    -Message "runtime smoke plugin must receive the budget exhaustion toggle."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'FBZ_PLUGIN_SMOKE_DELIVER_NOTIFICATION' `
    -Message "runtime smoke plugin must receive the notification delivery toggle."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'scheduler\.register' `
    -Message "runtime smoke plugin must request scheduler.register when schedule dispatch is enabled."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'schedules\.runtimeSmoke' `
    -Message "runtime smoke plugin must expose a schedule handler for schedule dispatch."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '/api/admin/scheduled-tasks/\$escapedScheduleKey/run' `
    -Message "runtime smoke must manually trigger the synchronized plugin schedule."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'scheduler\.tick' `
    -Message "runtime smoke must verify scheduler.tick dispatch execution."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'scheduledRunQueuedJobs' `
    -Message "runtime smoke must expose scheduled task run enqueue evidence."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'PLUGIN_HOST_API_MAX_CALLS_PER_RUN = if \(\$ExhaustHostApiBudget\)' `
    -Message "runtime smoke must lower Host API budget when budget exhaustion is requested."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'FBZ_NOTIFICATION_WORKER_ENABLED = if \(\$DeliverNotification\)' `
    -Message "runtime smoke must enable the notification worker when delivery is requested."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'notification\.send' `
    -Message "runtime smoke plugin must request notification.send when delivery is enabled."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '/api/plugin/notifications' `
    -Message "runtime smoke plugin must submit a Host API notification request."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '/api/admin/notification-targets' `
    -Message "runtime smoke must configure an administrator-managed notification target."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '/api/admin/notification-requests' `
    -Message "runtime smoke must observe notification requests through the Admin API."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern '/attempts\?status=succeeded' `
    -Message "runtime smoke must verify a succeeded notification delivery attempt."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'webhookDeliveryCount' `
    -Message "runtime smoke must expose received webhook delivery evidence."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'budgetExceededCalls' `
    -Message "runtime smoke must collect 429 Host API budget audit evidence."

Assert-Contains `
    -Text $runtimeSource `
    -Pattern 'configStatusAfterBudget' `
    -Message "runtime smoke plugin log must include the over-budget config call status."

"smoke plugin signature script checks passed"
