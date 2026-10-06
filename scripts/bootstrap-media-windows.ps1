param([Parameter(Mandatory=$true)][string]$MediaRoot)
$ErrorActionPreference='Stop'
New-Item -ItemType Directory -Force $MediaRoot | Out-Null
$MediaRoot=(Resolve-Path $MediaRoot).Path
$vcpkg=Join-Path $MediaRoot 'vcpkg'
if (!(Test-Path "$vcpkg/.git")) { git clone --depth 1 --branch 2026.07.29 https://github.com/microsoft/vcpkg.git $vcpkg; if ($LASTEXITCODE) { throw 'vcpkg checkout failed' } }
& "$vcpkg/bootstrap-vcpkg.bat" -disableMetrics
if ($LASTEXITCODE) { throw 'vcpkg bootstrap failed' }
# core suppresses default GPL x265 encoder; libde265 is an unconditional dependency.
& "$vcpkg/vcpkg.exe" install 'libheif[core]:x64-windows' --recurse
if ($LASTEXITCODE) { throw 'HEIC decoder dependencies failed' }
$env:VCPKG_ROOT=$vcpkg
$env:VCPKGRS_DYNAMIC='1'
$env:PATH="$vcpkg/installed/x64-windows/bin;$env:PATH"
if ($env:GITHUB_ENV) {
  "VCPKG_ROOT=$vcpkg" | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
  'VCPKGRS_DYNAMIC=1' | Out-File -Append -Encoding utf8 $env:GITHUB_ENV
  "$vcpkg/installed/x64-windows/bin" | Out-File -Append -Encoding utf8 $env:GITHUB_PATH
}
