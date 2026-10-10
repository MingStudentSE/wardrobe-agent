# wardrobe-agent Windows ARM64 release build (ticket #35).
# Run INSIDE the guest after windows-provision.ps1, with source at C:\wardrobe\src.
# Mirrors the Windows leg of .github/workflows/media-bundles.yml with
# -Triplet arm64-windows (local ARM64-hosted build; CI builds x64 on x64 runners).
$ErrorActionPreference = 'Stop'
Start-Transcript -Path 'C:\wardrobe\build.log' -Force | Out-Null

$env:VCPKG_ROOT = 'C:\wardrobe\media\vcpkg'
$env:VCPKGRS_DYNAMIC = '1'
$env:Path = "$env:VCPKG_ROOT\installed\arm64-windows\bin;$env:USERPROFILE\.cargo\bin;C:\Program Files\nodejs;C:\Python313;C:\wardrobe\mingit\cmd;$env:Path"

Set-Location C:\wardrobe\src

Write-Output '== [1/5] native HEIC decoder (vcpkg libheif:arm64-windows, pinned vcpkg 2026.07.29)'
& .\scripts\bootstrap-media-windows.ps1 -MediaRoot C:\wardrobe\media -Triplet arm64-windows

Write-Output '== [2/5] cargo build --release -p wardrobe-cli (aarch64-pc-windows-msvc)'
cargo build --release -p wardrobe-cli
if ($LASTEXITCODE) { throw "cargo build failed: $LASTEXITCODE" }

Write-Output '== [3/5] stage distribution (portable CLI + decoder DLLs + CRT + tauri overlay)'
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -products * -latest -property installationPath
$crt = Get-ChildItem "$vs\VC\Redist\MSVC\*\arm64\Microsoft.VC*.CRT" -Directory | Sort-Object FullName | Select-Object -Last 1
if (!$crt) { throw 'ARM64 app-local CRT unavailable' }
python scripts\prepare-media-distribution.py "$env:VCPKG_ROOT\installed\arm64-windows" target\release\wardrobe-cli.exe C:\wardrobe\dist --target aarch64-pc-windows-msvc --windows-crt-dir $crt.FullName
if ($LASTEXITCODE) { throw 'staging failed' }

Write-Output '== [4/5] tauri NSIS installer'
Push-Location apps\desktop
npm ci
if ($LASTEXITCODE) { throw 'npm ci failed' }
npm run tauri build -- --config C:\wardrobe\dist\tauri-media.json
if ($LASTEXITCODE) { throw 'tauri build failed' }
Pop-Location

Write-Output '== [5/5] NSIS silent install + packaged CLI smoke (real install)'
$installer = Get-ChildItem apps\desktop\src-tauri\target\release\bundle\nsis\*.exe | Select-Object -First 1
if (!$installer) { throw 'NSIS installer missing' }
Write-Output "installer: $($installer.FullName)"
$install = 'C:\wardrobe\installed'
$p = Start-Process -Wait -PassThru -FilePath $installer.FullName -ArgumentList '/S',"/D=$install"
Write-Output "installer exit: $($p.ExitCode)"
if ($p.ExitCode) { throw "installer failed $($p.ExitCode)" }

python scripts\verify-packaged-cli.py "$install\wardrobe-cli.exe" crates\wardrobe-core\tests\fixtures\generated-colors.heic --report C:\wardrobe\dist\nsis-runtime.json
if ($LASTEXITCODE) { throw 'packaged CLI verification failed' }

Write-Output 'WINDOWS-PIPELINE-OK'
Get-ChildItem C:\wardrobe\dist\nsis-runtime.json | Out-String
Stop-Transcript | Out-Null
