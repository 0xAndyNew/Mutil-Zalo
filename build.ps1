$ErrorActionPreference = "Stop"

# Use Rustup's self-contained windows-gnullvm + ld.lld.exe (no Visual Studio or external MinGW required)
$toolchain = "stable-x86_64-pc-windows-gnullvm"
$sysroot = (rustc +$toolchain --print sysroot).Trim()
$lld = Join-Path $sysroot "lib\rustlib\x86_64-pc-windows-gnullvm\bin\gcc-ld\ld.lld.exe"
if (-not (Test-Path $lld)) {
    throw "Cannot find bundled ld.lld.exe at $lld"
}
$gnuSysroot = (rustc +stable-x86_64-pc-windows-gnu --print sysroot).Trim()
$gnuSelfContained = Join-Path $gnuSysroot "lib\rustlib\x86_64-pc-windows-gnu\lib\self-contained"

$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $lld
$env:RUSTFLAGS = "-C link-self-contained=yes -C linker-flavor=ld -C link-arg=-m -C link-arg=i386pep -C link-arg=-L$gnuSelfContained"

Write-Host "=== [1/4] Building mutil-zalo-core (Release) ===" -ForegroundColor Cyan
cargo +$toolchain build --release -p mutil-zalo-core
if ($LASTEXITCODE -ne 0) { throw "Build mutil-zalo-core failed!" }

$coreExe = Join-Path $PSScriptRoot "target\release\mutil-zalo-core.exe"
if (-not (Test-Path $coreExe)) {
    throw "Cannot find $coreExe"
}

Write-Host "=== [2/4] Locating WebView2Loader.dll & staging payload ===" -ForegroundColor Cyan
$dllCandidate = Get-ChildItem -Path (Join-Path $PSScriptRoot "target\release\build") -Filter "WebView2Loader.dll" -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "x64" } |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $dllCandidate) {
    $dllCandidate = Get-ChildItem -Path (Join-Path $PSScriptRoot "target\release\build") -Filter "WebView2Loader.dll" -Recurse -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}

if (-not $dllCandidate) {
    throw "Cannot find WebView2Loader.dll in target\release\build!"
}

$stagingDir = Join-Path $PSScriptRoot "target\staging"
New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
Copy-Item -Force $coreExe (Join-Path $stagingDir "mutil-zalo-core.exe")
Copy-Item -Force $dllCandidate.FullName (Join-Path $stagingDir "WebView2Loader.dll")

Write-Host "  Staged core: $((Get-Item (Join-Path $stagingDir 'mutil-zalo-core.exe')).Length) bytes"
Write-Host "  Staged DLL : $((Get-Item (Join-Path $stagingDir 'WebView2Loader.dll')).Length) bytes ($($dllCandidate.FullName))"

Write-Host "=== [3/4] Building single-file Mutil-Zalo.exe (Release) ===" -ForegroundColor Cyan
$env:MUTIL_ZALO_REQUIRE_PAYLOAD = "1"
$env:MUTIL_ZALO_STAGING_DIR = $stagingDir
cargo +$toolchain build --release -p mutil-zalo
if ($LASTEXITCODE -ne 0) { throw "Build mutil-zalo single-file launcher failed!" }

Write-Host "=== [4/4] Exporting single file to dist\Mutil-Zalo.exe ===" -ForegroundColor Cyan
$distDir = Join-Path $PSScriptRoot "dist"
if (Test-Path $distDir) {
    Remove-Item -Recurse -Force $distDir
}
New-Item -ItemType Directory -Force -Path $distDir | Out-Null

$builtSingleExe = Join-Path $PSScriptRoot "target\release\Mutil-Zalo.exe"
$finalExe = Join-Path $distDir "Mutil-Zalo.exe"
Copy-Item -Force $builtSingleExe $finalExe

$sizeMB = [Math]::Round((Get-Item $finalExe).Length / 1MB, 2)
Write-Host ""
Write-Host "BUILD THANH CONG! (1 FILE DUY NHAT)" -ForegroundColor Green
Write-Host "  Output: $finalExe ($sizeMB MB)" -ForegroundColor Yellow
