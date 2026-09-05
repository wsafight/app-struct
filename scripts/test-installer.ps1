param([string]$ArchiveDir, [string]$Version)
$ErrorActionPreference = 'Stop'
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('appstruct-installer-test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $bin = Join-Path $temporary 'bin with spaces'
    $installer = Join-Path $PSScriptRoot 'install.ps1'
    & $installer -Version $Version -ArchiveDir $ArchiveDir -BinDir $bin
    $binary = Join-Path $bin 'appstruct.exe'
    if ((& $binary --version) -ne "appstruct $Version") { throw 'Installed binary version mismatch' }
    $original = (Get-FileHash $binary).Hash
    & $installer -Version $Version -ArchiveDir $ArchiveDir -BinDir $bin
    if ((Get-FileHash $binary).Hash -ne $original) { throw 'Replacement changed the binary' }
    $corrupt = Join-Path $temporary 'corrupt'
    New-Item -ItemType Directory -Path $corrupt | Out-Null
    Copy-Item (Join-Path $ArchiveDir "appstruct-$Version-*.zip*") $corrupt
    $zip = Get-ChildItem $corrupt -Filter '*.zip' | Select-Object -First 1
    [IO.File]::AppendAllText($zip.FullName, 'corrupt')
    $rejected = $false
    try { & $installer -Version $Version -ArchiveDir $corrupt -BinDir $bin }
    catch { if ($_.Exception.Message -ne 'Release checksum mismatch') { throw }; $rejected = $true }
    if (-not $rejected -or (Get-FileHash $binary).Hash -ne $original) { throw 'Corrupt archive was not rejected safely' }
    Write-Output 'Windows installer checks passed'
} finally { Remove-Item $temporary -Recurse -Force }
