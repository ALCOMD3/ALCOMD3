$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "Desktop E2E currently requires Windows WebView2."
}

$node = (Get-Command node.exe -ErrorAction Stop).Source
$wdio = Join-Path $PSScriptRoot "..\node_modules\@wdio\cli\bin\wdio.js"
$configuration = Join-Path $PSScriptRoot "..\test\e2e\wdio.conf.mjs"
if (-not (Test-Path -LiteralPath $wdio -PathType Leaf)) {
    throw "WebdriverIO is not installed. Run npm ci first."
}

$processJobSource = Join-Path $PSScriptRoot "test-process-job.cs"
Add-Type -Path $processJobSource -ErrorAction Stop
$desktopE2eTimeoutMinutes = 15
$resultFile = Join-Path $env:TEMP "alcomd3-wdio-result-$PID.json"
$deepLinkRegistryPath = "Software\Classes\vcc"
$env:ALCOMD3_E2E_RESULT_FILE = $resultFile
if (Test-Path -LiteralPath $resultFile) {
    Remove-Item -LiteralPath $resultFile -Force
}

function Get-RegistryKeySnapshot {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryKey] $Key
    )

    $values = @(
        foreach ($valueName in @($Key.GetValueNames() | Sort-Object)) {
            [pscustomobject] @{
                Name = $valueName
                Kind = [int] $Key.GetValueKind($valueName)
                Data = $Key.GetValue(
                    $valueName,
                    $null,
                    [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
                )
            }
        }
    )
    $subKeys = @(
        foreach ($subKeyName in @($Key.GetSubKeyNames() | Sort-Object)) {
            $subKey = $Key.OpenSubKey($subKeyName)
            if ($null -eq $subKey) {
                throw "Failed to read registry key '$($Key.Name)\$subKeyName'."
            }
            try {
                [pscustomobject] @{
                    Name = $subKeyName
                    Tree = Get-RegistryKeySnapshot -Key $subKey
                }
            }
            finally {
                $subKey.Dispose()
            }
        }
    )

    return [pscustomobject] @{
        Values = $values
        SubKeys = $subKeys
    }
}

function Get-DeepLinkAssociationSnapshot {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($deepLinkRegistryPath)
    if ($null -eq $key) {
        return [pscustomobject] @{
            Exists = $false
            Tree = $null
        }
    }

    try {
        return [pscustomobject] @{
            Exists = $true
            Tree = Get-RegistryKeySnapshot -Key $key
        }
    }
    finally {
        $key.Dispose()
    }
}

function Restore-RegistryKeySnapshot {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryKey] $Key,
        [Parameter(Mandatory)]
        [pscustomobject] $Snapshot
    )

    foreach ($value in $Snapshot.Values) {
        $Key.SetValue(
            $value.Name,
            $value.Data,
            [Microsoft.Win32.RegistryValueKind] $value.Kind
        )
    }
    foreach ($subKeySnapshot in $Snapshot.SubKeys) {
        $subKey = $Key.CreateSubKey($subKeySnapshot.Name)
        if ($null -eq $subKey) {
            throw "Failed to restore registry key '$($Key.Name)\$($subKeySnapshot.Name)'."
        }
        try {
            Restore-RegistryKeySnapshot -Key $subKey -Snapshot $subKeySnapshot.Tree
        }
        finally {
            $subKey.Dispose()
        }
    }
}

function Restore-DeepLinkAssociationSnapshot {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Snapshot
    )

    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($deepLinkRegistryPath, $false)
    if (-not $Snapshot.Exists) {
        return
    }

    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($deepLinkRegistryPath)
    if ($null -eq $key) {
        throw "Failed to restore registry key 'HKCU\$deepLinkRegistryPath'."
    }
    try {
        Restore-RegistryKeySnapshot -Key $key -Snapshot $Snapshot.Tree
    }
    finally {
        $key.Dispose()
    }
}

function Convert-DeepLinkAssociationSnapshotToJson {
    param(
        [Parameter(Mandatory)]
        [pscustomobject] $Snapshot
    )

    return $Snapshot | ConvertTo-Json -Depth 100 -Compress
}

$runner = $null
$processJob = $null
$exitCode = 1
$associationBefore = Get-DeepLinkAssociationSnapshot
$associationBeforeJson = Convert-DeepLinkAssociationSnapshotToJson -Snapshot $associationBefore
$associationChanged = $false
try {
    $processJob = [Alcomd3.E2E.TestProcessJob]::new()
    $runner = $processJob.Start(
        $node,
        @($wdio, 'run', $configuration),
        (Get-Location).Path
    )
    if ($null -eq $runner) {
        throw 'Failed to start WebdriverIO.'
    }
    if (-not $runner.WaitForExit($desktopE2eTimeoutMinutes * 60 * 1000)) {
        $processJob.Dispose()
        $processJob = $null
        throw "Desktop E2E exceeded its $desktopE2eTimeoutMinutes-minute timeout."
    }
    $exitCode = $runner.ExitCode
    if (-not (Test-Path -LiteralPath $resultFile -PathType Leaf)) {
        throw 'WebdriverIO did not write a completion result.'
    }
    $result = Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
    if (
        $result.exitCode -ne 0 -or
        $result.failed -ne 0 -or
        $null -eq $result.passed -or
        $result.passed -lt 1
    ) {
        $exitCode = 1
    }
}
finally {
    try {
        if ($null -ne $processJob) {
            $processJob.Dispose()
        }
        if ($null -ne $runner) {
            if (-not $runner.HasExited) {
                [void] $runner.WaitForExit(10000)
            }
            $runner.Dispose()
        }
        if (Test-Path -LiteralPath $resultFile) {
            Remove-Item -LiteralPath $resultFile -Force -ErrorAction SilentlyContinue
        }
    }
    finally {
        $associationAfter = Get-DeepLinkAssociationSnapshot
        $associationAfterJson = Convert-DeepLinkAssociationSnapshotToJson -Snapshot $associationAfter
        if ($associationAfterJson -cne $associationBeforeJson) {
            $associationChanged = $true
            Restore-DeepLinkAssociationSnapshot -Snapshot $associationBefore
            $associationRestored = Get-DeepLinkAssociationSnapshot
            $associationRestoredJson = Convert-DeepLinkAssociationSnapshotToJson -Snapshot $associationRestored
            if ($associationRestoredJson -cne $associationBeforeJson) {
                throw "Desktop E2E changed the current user's vcc:// association and restoration failed."
            }
        }
    }
}

if ($associationChanged) {
    throw "Desktop E2E changed the current user's vcc:// association; the original registration was restored."
}

exit $exitCode
