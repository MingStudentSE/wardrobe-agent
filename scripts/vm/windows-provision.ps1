# wardrobe-agent Windows ARM64 build VM provisioning (ticket #35).
# Run INSIDE the guest as the build user, AFTER first-logon (network + sshd ready).
# Installs: VS 2022 Build Tools (ARM64 toolsets + SDK + CMake), Rust MSVC, Node 22,
# Python, vcpkg 2026.07.29 with libheif:arm64-windows. Everything user-local or
# under C:\ — nothing outside the VM.
$ErrorActionPreference = 'Continue'
Start-Transcript -Path 'C:\wardrobe\provision.log' -Force | Out-Null
Set-ExecutionPolicy -Scope Process Bypass -Force

$ProgressPreference = 'SilentlyContinue'

function Get-File($url, $out) {
  Write-Output "== GET $url"
  Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
  Write-Output ("   {0:N0} bytes" -f (Get-Item $out).Length)
}

# --- 1. Rust (MSVC host aarch64) ---------------------------------------------
if (-not (Test-Path "$env:USERPROFILE\.cargo\bin\rustc.exe")) {
  Get-File 'https://static.rust-lang.org/rustup/dist/aarch64-pc-windows-msvc/rustup-init.exe' 'C:\wardrobe\rustup-init.exe'
  Start-Process -Wait -FilePath 'C:\wardrobe\rustup-init.exe' -ArgumentList '-y','--default-toolchain','stable','--profile','minimal'
}
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
rustc --version

# --- 2. Python 3 (ARM64) ------------------------------------------------------
if (-not (Test-Path 'C:\Python313\python.exe')) {
  Get-File 'https://www.python.org/ftp/python/3.13.1/python-3.13.1-arm64.exe' 'C:\wardrobe\python-arm64.exe'
  Start-Process -Wait -FilePath 'C:\wardrobe\python-arm64.exe' -ArgumentList '/quiet','InstallAllUsers=1','PrependPath=1','TargetDir=C:\Python313'
}
$env:Path = "C:\Python313;$env:Path"
python --version

# --- 3. Node.js 22 (ARM64) ----------------------------------------------------
if (-not (Test-Path 'C:\Program Files\nodejs\node.exe')) {
  Get-File 'https://nodejs.org/dist/v22.11.0/node-v22.11.0-arm64.msi' 'C:\wardrobe\node-arm64.msi'
  Start-Process -Wait -FilePath msiexec.exe -ArgumentList '/i','C:\wardrobe\node-arm64.msi','/qn'
}
$env:Path = "C:\Program Files\nodejs;$env:Path"
node --version

# --- 4. VS 2022 Build Tools (ARM64-hosted MSVC + SDK + CMake) ------------------
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$haveVS = $false
if (Test-Path $vsWhere) {
  $inst = & $vsWhere -products 'Microsoft.VisualStudio.Product.BuildTools' -property installationPath
  if ($inst) { $haveVS = $true; Write-Output "BuildTools at $inst" }
}
if (-not $haveVS) {
  Get-File 'https://aka.ms/vs/17/release/vs_BuildTools.exe' 'C:\wardrobe\vs_BuildTools.exe'
  $args = @('--quiet','--wait','--norestart','--nocache',`
    '--add','Microsoft.VisualStudio.Workload.VCTools',`
    '--add','Microsoft.VisualStudio.Component.VC.Tools.ARM64',`
    '--add','Microsoft.VisualStudio.Component.VC.Tools.x86.x64',`
    '--add','Microsoft.VisualStudio.Component.Windows11SDK.26100',`
    '--add','Microsoft.VisualStudio.Component.VC.Redist.14.Latest',`
    '--add','Microsoft.VisualStudio.Component.VC.CMake.Project',`
    '--includeRecommended')
  $p = Start-Process -PassThru -FilePath 'C:\wardrobe\vs_BuildTools.exe' -ArgumentList $args
  $p.WaitForExit()
  Write-Output "vs_BuildTools exit: $($p.ExitCode)"
}

# --- 5. vcpkg pinned 2026.07.29 + libheif:arm64-windows -----------------------
if (-not (Test-Path 'C:\wardrobe\media\vcpkg\.git')) {
  git --version 2>$null
  if ($LASTEXITCODE) {
    Write-Output 'git missing: fetching portable git'
    Get-File 'https://github.com/git-for-windows/git/releases/download/v2.47.0.windows.1/MinGit-2.47.0-arm64.zip' 'C:\wardrobe\mingit.zip'
    Expand-Archive -Force -Path 'C:\wardrobe\mingit.zip' -DestinationPath 'C:\wardrobe\mingit'
    $env:Path = "C:\wardrobe\mingit\cmd;$env:Path"
  }
  New-Item -ItemType Directory -Force 'C:\wardrobe\media' | Out-Null
  git clone --depth 1 --branch 2026.07.29 https://github.com/microsoft/vcpkg.git C:\wardrobe\media\vcpkg
}
& C:\wardrobe\media\vcpkg\bootstrap-vcpkg.bat -disableMetrics
$env:VCPKG_ROOT = 'C:\wardrobe\media\vcpkg'
& "$env:VCPKG_ROOT\vcpkg.exe" install 'libheif[core]:arm64-windows' --recurse
if ($LASTEXITCODE) { Write-Output "VCPKG-FAILED exit $LASTEXITCODE" } else { Write-Output 'VCPKG-OK' }

Write-Output 'PROVISION-DONE'
Stop-Transcript | Out-Null
