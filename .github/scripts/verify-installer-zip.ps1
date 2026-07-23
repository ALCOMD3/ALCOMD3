param(
    [Parameter(Mandatory)]
    [string] $Installer,

    [Parameter(Mandatory)]
    [string] $Archive
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

foreach ($path in @($Installer, $Archive)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Expected installer artifact is missing: $path"
    }
}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::OpenRead($Archive)
try {
    if ($zip.Entries.Count -ne 1) {
        throw 'Installer ZIP must contain exactly one entry'
    }
    $entry = $zip.Entries[0]
    if ($entry.Name -cne [System.IO.Path]::GetFileName($Installer)) {
        throw "Installer ZIP entry name mismatch: $($entry.Name)"
    }
    $stream = $entry.Open()
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $zipHash = [System.BitConverter]::ToString(
            $hasher.ComputeHash($stream)
        ).Replace('-', '')
    }
    finally {
        $hasher.Dispose()
        $stream.Dispose()
    }
}
finally {
    $zip.Dispose()
}

$installerHash = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash
if ($zipHash -cne $installerHash) {
    throw 'Installer ZIP content hash does not match the standalone installer'
}
