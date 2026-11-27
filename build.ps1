$ErrorActionPreference = "Stop"

# Helper to embed a multi-resolution .ico directly into a Windows PE .exe (using native kernel32 API, zero external tools)
Add-Type -TypeDefinition @"
using System;
using System.IO;
using System.Runtime.InteropServices;

public static class PeIconEmbedder {
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr BeginUpdateResource(string pFileName, bool bDeleteExistingResources);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool UpdateResource(IntPtr hUpdate, IntPtr lpType, IntPtr lpName, ushort wLanguage, byte[] lpData, uint cbData);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool EndUpdateResource(IntPtr hUpdate, bool fDiscard);

    public static void EmbedIco(string exePath, string icoPath) {
        byte[] ico = File.ReadAllBytes(icoPath);
        if (ico.Length < 6) throw new Exception("Invalid ICO file");
        ushort count = BitConverter.ToUInt16(ico, 4);

        IntPtr hUpdate = BeginUpdateResource(exePath, false);
        if (hUpdate == IntPtr.Zero) {
            throw new Exception("BeginUpdateResource failed: " + Marshal.GetLastWin32Error());
        }

        try {
            using (var grpMs = new MemoryStream())
            using (var bw = new BinaryWriter(grpMs)) {
                bw.Write((ushort)0);     // reserved
                bw.Write((ushort)1);     // type = icon
                bw.Write(count);         // count

                for (int i = 0; i < count; i++) {
                    int entryOffset = 6 + i * 16;
                    byte width = ico[entryOffset + 0];
                    byte height = ico[entryOffset + 1];
                    byte colorCount = ico[entryOffset + 2];
                    byte reserved = ico[entryOffset + 3];
                    ushort planes = BitConverter.ToUInt16(ico, entryOffset + 4);
                    ushort bitCount = BitConverter.ToUInt16(ico, entryOffset + 6);
                    uint bytesInRes = BitConverter.ToUInt32(ico, entryOffset + 8);
                    uint imageOffset = BitConverter.ToUInt32(ico, entryOffset + 12);

                    byte[] imgData = new byte[bytesInRes];
                    Array.Copy(ico, (int)imageOffset, imgData, 0, (int)bytesInRes);

                    ushort iconId = (ushort)(i + 1);
                    // RT_ICON = 3
                    if (!UpdateResource(hUpdate, (IntPtr)3, (IntPtr)iconId, 0, imgData, bytesInRes)) {
                        throw new Exception("UpdateResource(RT_ICON) failed: " + Marshal.GetLastWin32Error());
                    }

                    // GRPICONDIRENTRY (14 bytes)
                    bw.Write(width);
                    bw.Write(height);
                    bw.Write(colorCount);
                    bw.Write(reserved);
                    bw.Write(planes);
                    bw.Write(bitCount);
                    bw.Write(bytesInRes);
                    bw.Write(iconId);
                }

                bw.Flush();
                byte[] grpBytes = grpMs.ToArray();
                // RT_GROUP_ICON = 14, resource ID = 1
                if (!UpdateResource(hUpdate, (IntPtr)14, (IntPtr)1, 0, grpBytes, (uint)grpBytes.Length)) {
                    throw new Exception("UpdateResource(RT_GROUP_ICON) failed: " + Marshal.GetLastWin32Error());
                }
            }

            if (!EndUpdateResource(hUpdate, false)) {
                throw new Exception("EndUpdateResource failed: " + Marshal.GetLastWin32Error());
            }
        } catch {
            EndUpdateResource(hUpdate, true);
            throw;
        }
    }

    public static void EmbedManifest(string exePath, string manifestXml) {
        byte[] data = System.Text.Encoding.UTF8.GetBytes(manifestXml);
        IntPtr hUpdate = BeginUpdateResource(exePath, false);
        if (hUpdate == IntPtr.Zero) {
            throw new Exception("BeginUpdateResource(manifest) failed: " + Marshal.GetLastWin32Error());
        }
        try {
            // RT_MANIFEST = 24, CREATEPROCESS_MANIFEST_RESOURCE_ID = 1
            if (!UpdateResource(hUpdate, (IntPtr)24, (IntPtr)1, 0, data, (uint)data.Length)) {
                throw new Exception("UpdateResource(RT_MANIFEST) failed: " + Marshal.GetLastWin32Error());
            }
            if (!EndUpdateResource(hUpdate, false)) {
                throw new Exception("EndUpdateResource(manifest) failed: " + Marshal.GetLastWin32Error());
            }
        } catch {
            EndUpdateResource(hUpdate, true);
            throw;
        }
    }
}
"@

$launcherManifest = @"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity version="1.0.0.0" processorArchitecture="amd64" name="MutilZalo" type="win32"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, System</dpiAwareness>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
    </windowsSettings>
  </application>
</assembly>
"@

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

$icoPath = Join-Path $PSScriptRoot "crates\core\assets\zalo.ico"
if (-not (Test-Path $icoPath)) {
    throw "Cannot find icon at $icoPath"
}

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
$stagedCore = Join-Path $stagingDir "mutil-zalo-core.exe"
Copy-Item -Force $coreExe $stagedCore
[PeIconEmbedder]::EmbedIco($stagedCore, $icoPath)
Copy-Item -Force $dllCandidate.FullName (Join-Path $stagingDir "WebView2Loader.dll")

Write-Host "  Staged core: $((Get-Item $stagedCore).Length) bytes (embedded Zalo PE icon)"
Write-Host "  Staged DLL : $((Get-Item (Join-Path $stagingDir 'WebView2Loader.dll')).Length) bytes ($($dllCandidate.FullName))"

Write-Host "=== [3/4] Building single-file Mutil-Zalo.exe (Release) ===" -ForegroundColor Cyan
$env:MUTIL_ZALO_REQUIRE_PAYLOAD = "1"
$env:MUTIL_ZALO_STAGING_DIR = $stagingDir
cargo +$toolchain build --release -p mutil-zalo
if ($LASTEXITCODE -ne 0) { throw "Build mutil-zalo single-file launcher failed!" }

Write-Host "=== [4/4] Exporting single file dist\Mutil-Zalo.exe (embedded Zalo icon + manifest) ===" -ForegroundColor Cyan
$distDir = Join-Path $PSScriptRoot "dist"
if (Test-Path $distDir) {
    Remove-Item -Recurse -Force $distDir
}
New-Item -ItemType Directory -Force -Path $distDir | Out-Null

$builtSingleExe = Join-Path $PSScriptRoot "target\release\Mutil-Zalo.exe"
$finalExe = Join-Path $distDir "Mutil-Zalo.exe"
Copy-Item -Force $builtSingleExe $finalExe
[PeIconEmbedder]::EmbedIco($finalExe, $icoPath)
[PeIconEmbedder]::EmbedManifest($finalExe, $launcherManifest)

$sizeMB = [Math]::Round((Get-Item $finalExe).Length / 1MB, 2)
Write-Host ""
Write-Host "BUILD THANH CONG! (1 FILE DUY NHAT + INSTALLER + LOGO ZALO)" -ForegroundColor Green
Write-Host "  Output: $finalExe ($sizeMB MB)" -ForegroundColor Yellow
