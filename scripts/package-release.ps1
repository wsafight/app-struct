param(
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Binary,
    [Parameter(Mandatory = $true)][string]$OutputDir
)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$') { throw 'Invalid release version' }
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'Release binary is missing' }
if ((& $Binary --version) -ne "appstruct $Version") { throw 'Release binary version does not match' }

$target = 'x86_64-pc-windows-msvc'
$stem = "appstruct-$Version-$target"
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$OutputDir = (Resolve-Path -LiteralPath $OutputDir).Path
$archive = Join-Path $OutputDir "$stem.zip"
$checksum = "$archive.sha256"
if ((Test-Path -LiteralPath $archive) -or (Test-Path -LiteralPath $checksum)) { throw 'Release archive already exists' }
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('appstruct-package-' + [guid]::NewGuid())
$staged = Join-Path $temporary $stem
New-Item -ItemType Directory -Path $staged -Force | Out-Null
try {
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $staged 'appstruct.exe')
    foreach ($name in @('README.md', 'LICENSE-MIT', 'LICENSE-APACHE')) {
        Copy-Item -LiteralPath (Join-Path (Split-Path $PSScriptRoot -Parent) $name) -Destination $staged
    }
    Compress-Archive -LiteralPath $staged -DestinationPath $archive -CompressionLevel Optimal
    $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath $checksum -Encoding ascii -Value "$hash  $stem.zip"
    Write-Output "Packaged $archive"
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
