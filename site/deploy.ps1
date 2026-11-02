# Publishes the usage guide to http://o-server.main.jp/avese/ over FTPS.
# Usage: powershell -File site\deploy.ps1 -OServer <path to the o-server repository>
param(
    [Parameter(Mandatory = $true)][string]$OServer
)

$ErrorActionPreference = 'Stop'
$envFile = Join-Path $OServer '.env'
$publisherDir = Join-Path $OServer 'hacker-news'

$credentials = @{}
foreach ($line in Get-Content -LiteralPath $envFile) {
    $trimmed = $line.Trim()
    if (-not $trimmed -or $trimmed.StartsWith('#') -or $trimmed -notmatch '=') {
        continue
    }
    $key, $value = $trimmed -split '=', 2
    $credentials[$key.Trim()] = $value.Trim().Trim('"').Trim("'")
}
# The FTPS account is the same as the WebDAV one; deploy_ftps.py reads either name.
$env:WEBDAV_USER = if ($credentials['WEBDAV_USER']) { $credentials['WEBDAV_USER'] } else { $credentials['WEBDAV_USR'] }
$env:WEBDAV_PASS = $credentials['WEBDAV_PASS']
if (-not $env:WEBDAV_USER -or -not $env:WEBDAV_PASS) {
    throw 'Set WEBDAV_USER (or WEBDAV_USR) and WEBDAV_PASS in the o-server .env file.'
}

# deploy_ftps.py writes its manifest next to the local folder, so the published files are assembled in a temp folder.
$work = Join-Path $env:TEMP 'avese-site-deploy'
$local = Join-Path $work 'avese'
if (Test-Path -LiteralPath $work) {
    Remove-Item -LiteralPath $work -Recurse -Force
}
New-Item -ItemType Directory -Force (Join-Path $local 'samples') | Out-Null

& python -I (Join-Path $PSScriptRoot 'samples\make_samples.py')
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
Copy-Item (Join-Path $PSScriptRoot 'index.html'), (Join-Path $PSScriptRoot 'style.css') $local
Copy-Item (Join-Path $PSScriptRoot 'ja') $local -Recurse
Copy-Item (Join-Path $PSScriptRoot 'media') $local -Recurse
Copy-Item (Join-Path $PSScriptRoot 'samples\samples.zip'), (Join-Path $PSScriptRoot 'samples\make_samples.py') (Join-Path $local 'samples')

$runner = Join-Path $work 'publish.py'
Set-Content -LiteralPath $runner -Encoding utf8 -Value @'
import sys
sys.path.insert(0, sys.argv[1])
from deploy_ftps import deploy
deploy(sys.argv[2], "/avese")
'@
& python -I $runner $publisherDir $local
exit $LASTEXITCODE
