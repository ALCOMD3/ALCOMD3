param(
    [Parameter(Mandatory)]
    [string] $CurrentVersion,

    [Parameter(Mandatory)]
    [string] $OutputDirectory
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($env:GITHUB_REPOSITORY)) {
    throw 'GITHUB_REPOSITORY is required to resolve the migration installer.'
}

$config = Get-Content alcomd3.config.json -Raw | ConvertFrom-Json
$migrationReleaseTag = [string] $config.legacyWindowsMigrationReleaseTag
if ([string]::IsNullOrWhiteSpace($migrationReleaseTag)) {
    throw 'legacyWindowsMigrationReleaseTag is required during the Windows identity migration.'
}
if ($migrationReleaseTag -notmatch '^v(.+)$') {
    throw "Legacy Windows migration release tag must start with v: $migrationReleaseTag"
}

$currentSemanticVersion = [semver] $CurrentVersion
$migrationSemanticVersion = [semver] $Matches[1]
if ($migrationSemanticVersion -ge $currentSemanticVersion) {
    throw "Legacy Windows migration release must be older than the current version: $migrationReleaseTag"
}

$encodedTag = [uri]::EscapeDataString($migrationReleaseTag)
$releaseResult = @(
    gh api "repos/$env:GITHUB_REPOSITORY/releases/tags/$encodedTag" 2>&1
)
if ($LASTEXITCODE -ne 0) {
    $releaseError = $releaseResult -join [Environment]::NewLine
    if ($releaseError -match 'HTTP 404|Not Found') {
        $global:LASTEXITCODE = 0
        Write-Warning "Migration baseline $migrationReleaseTag is not published in $env:GITHUB_REPOSITORY; running installer smoke without a previous installer."
        return
    }
    throw "Failed to resolve migration release $migrationReleaseTag`: $releaseError"
}

$previousRelease = ($releaseResult -join [Environment]::NewLine) | ConvertFrom-Json
if ($previousRelease.tag_name -cne $migrationReleaseTag) {
    throw "Migration release tag mismatch: expected $migrationReleaseTag, got $($previousRelease.tag_name)"
}
if ($previousRelease.draft -or $previousRelease.prerelease) {
    throw "Legacy Windows migration release must be a published stable release: $migrationReleaseTag"
}

$installerAssets = @(
    $previousRelease.assets | Where-Object {
        $_.name.EndsWith(
            '.exe',
            [System.StringComparison]::OrdinalIgnoreCase
        )
    }
)
if ($installerAssets.Count -ne 1) {
    throw "Previous stable release $migrationReleaseTag must contain exactly one Windows installer EXE; found $($installerAssets.Count)"
}

$previousName = [string] $installerAssets[0].name
if ([System.IO.Path]::GetFileName($previousName) -cne $previousName) {
    throw "Previous stable release has an unsafe installer asset name: $previousName"
}
if (Test-Path -LiteralPath $OutputDirectory) {
    throw "Migration installer output directory already exists: $OutputDirectory"
}

New-Item -ItemType Directory -Path $OutputDirectory -ErrorAction Stop | Out-Null
$null = gh release download $migrationReleaseTag `
    --repo $env:GITHUB_REPOSITORY `
    --pattern $previousName `
    --dir $OutputDirectory
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

$previousInstaller = Join-Path $OutputDirectory $previousName
if (-not (Test-Path -LiteralPath $previousInstaller -PathType Leaf)) {
    throw "Downloaded migration installer is missing: $previousInstaller"
}

Write-Output $previousInstaller
