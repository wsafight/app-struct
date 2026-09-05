param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$BinDir = (Join-Path $env:LOCALAPPDATA 'Programs\AppStruct'),
    [string]$ArchiveDir = ''
)
$ErrorActionPreference = 'Stop'
$Version = $Version -replace '^v', ''
if ($Version -notmatch '^\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$') { throw 'A pinned release version is required' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'This installer requires x64 Windows' }
$stem = "appstruct-$Version-x86_64-pc-windows-msvc"
$archive = "$stem.zip"
$temporary = Join-Path ([IO.Path]::GetTempPath()) ("appstruct-install-" + [guid]::NewGuid())
$staged = $null
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    foreach ($name in @($archive, "$archive.sha256")) {
        $destination = Join-Path $temporary $name
        if ($ArchiveDir) { Copy-Item -LiteralPath (Join-Path $ArchiveDir $name) -Destination $destination }
        else {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            Invoke-WebRequest -UseBasicParsing -TimeoutSec 300 -Uri "https://github.com/wsafight/app-struct/releases/download/v$Version/$name" -OutFile $destination
        }
    }
    $expected = ((Get-Content -LiteralPath (Join-Path $temporary "$archive.sha256") -TotalCount 1) -split '\s+')[0]
    if ($expected -notmatch '^[a-fA-F0-9]{64}$') { throw 'Invalid release checksum' }
    $package = Join-Path $temporary $archive
    if ((Get-FileHash -LiteralPath $package -Algorithm SHA256).Hash -ine $expected) { throw 'Release checksum mismatch' }
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    $BinDir = (Resolve-Path -LiteralPath $BinDir).Path
    $staged = Join-Path $BinDir ('.appstruct-install-' + [guid]::NewGuid())
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($package)
    try {
        $entry = $zip.GetEntry("$stem/appstruct.exe")
        if ($null -eq $entry -or $entry.Length -eq 0) { throw 'Release archive has no binary' }
        $source = $entry.Open()
        try {
            $file = [IO.File]::Open($staged, [IO.FileMode]::CreateNew)
            try { $source.CopyTo($file) } finally { $file.Dispose() }
        } finally { $source.Dispose() }
    } finally { $zip.Dispose() }
    $installed = Join-Path $BinDir 'appstruct.exe'
    if ([IO.File]::Exists($installed)) { [IO.File]::Replace($staged, $installed, $null) }
    else { [IO.File]::Move($staged, $installed) }
    $staged = $null
    Write-Output "Installed AppStruct $Version to $installed"
    Write-Output "Ensure $BinDir is on PATH."
} finally {
    if ($staged -and (Test-Path -LiteralPath $staged)) { Remove-Item -LiteralPath $staged -Force }
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
