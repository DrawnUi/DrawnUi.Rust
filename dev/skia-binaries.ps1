# Builds Skia (from the sibling rust-skia checkout, see the [patch] in Cargo.toml) ONCE and exports
# it as a rust-skia binaries archive into target\skia-bin. `.cargo\config.toml` points every build
# at that archive (SKIA_BINARIES_URL + FORCE_SKIA_BINARIES_DOWNLOAD), so no target dir compiles
# Skia again: a new target dir unpacks the archive in seconds.
#   dev\skia-binaries.ps1        -> desktop (x86_64-pc-windows-msvc)
#   dev\skia-binaries.ps1 -Web   -> browser (wasm32-unknown-emscripten), compiled with wasm exceptions
#                                   (rust-skia issue 1287); dev\build.ps1 and other machines (the
#                                   Fiddle builder) take it instead of compiling Skia.
#   dev\skia-binaries.ps1 -Android x86_64-linux-android
#                                -> one Android ABI (aarch64, armv7, x86_64, i686), through cargo-ndk
#                                   and the NDK in ANDROID_NDK_HOME, API level 26 (rust-skia's).
#   dev\skia-binaries.ps1 -Target aarch64-pc-windows-msvc
#                                -> another desktop target, cross-compiled (Windows on ARM needs
#                                   Visual Studio's MSVC ARM64 build tools).
#
# Run it again after every commit in ..\rust-skia (the archive name carries its commit hash) or when
# the Skia features of drawnui change.
param(
    [switch]$Web,
    [string]$Android,
    [string]$Target,
    [string]$Emsdk = $(if ($env:EMSDK) { $env:EMSDK } else { "C:\Dev\Tools\emsdk" })
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$out = Join-Path $repo "target\skia-bin"
$stage = Join-Path $out "stage"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null

$saved = @{}
foreach ($name in "BUILD_ARTIFACTSTAGINGDIRECTORY", "FORCE_SKIA_BUILD", "CARGO_TARGET_DIR") {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}
$webSaved = $null
try {
    # Build instead of downloading, and export what was built. A short target dir: Skia's build
    # fails on long paths.
    $env:BUILD_ARTIFACTSTAGINGDIRECTORY = $stage
    $env:FORCE_SKIA_BUILD = "1"
    # A cross target adds a folder level: clang-cl then passes MAX_PATH (GetFullPathNameA) on Skia's
    # deepest sources under the repo, so it builds in a folder at the drive root.
    $env:CARGO_TARGET_DIR = if ($Target) { "$env:SystemDrive\skt-" + $Target.Split("-")[0] } else {
        Join-Path $repo $(if ($Web) { "target\a\skiaweb" } elseif ($Android) { "target\a\sk-" + $Android.Split("-")[0] } else { "target\a\skia" }) }
    $cargo = @("build", "-p", "drawnui")
    if ($Target) { $cargo += @("--target", $Target) }
    if ($Target -like "*-windows-msvc") {
        # The bindings' C++ with LLVM's clang-cl, as Skia: Visual Studio may have the target's
        # libraries but not its x64-hosted compiler (cc then finds no cl.exe).
        $clang = (Get-Command clang-cl -ErrorAction SilentlyContinue).Source
        if (-not $clang) { $clang = "$env:ProgramFiles\LLVM\bin\clang-cl.exe" }
        foreach ($name in "CC_$($Target -replace '-', '_')", "CXX_$($Target -replace '-', '_')") {
            $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
            if (-not $saved[$name]) { [Environment]::SetEnvironmentVariable($name, $clang, "Process") }
        }
    }
    if ($Android) {
        # NDK 26: with NDK 29 ICU does not compile (umapfile.cpp: posix_madvise undeclared).
        $ndk = Get-ChildItem (Join-Path $env:ANDROID_HOME "ndk") -Directory | Where-Object Name -like "26.*" | Select-Object -Last 1
        if (-not $ndk) { throw "no NDK 26 under $env:ANDROID_HOME\ndk" }
        foreach ($name in "ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT") {
            $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
            [Environment]::SetEnvironmentVariable($name, $ndk.FullName, "Process")
        }
        $cargo = @("ndk", "-t", $Android, "--platform", "26") + $cargo
        # Skia's build calls `python3`; where that is only the Microsoft Store stub, forward it to `python`.
        cmd /c "python3 --version >nul 2>&1"
        if ($LASTEXITCODE) {
            $saved["PATH"] = $env:PATH
            $shim = Join-Path $repo "target\shim"
            New-Item -ItemType Directory -Force $shim | Out-Null
            Set-Content "$shim\python3.bat" '@python %*' -Encoding ascii
            $env:PATH = "$shim;$env:PATH"
        }
    }
    if ($Web) {
        . "$PSScriptRoot\web-env.ps1"
        $webSaved = Set-WebEnv -Root $repo -Emsdk $Emsdk -BuildSkia
        $cargo += @("--release", "--target", "wasm32-unknown-emscripten")
    }
    Push-Location $repo
    # Only skia-bindings has to build; the rest of the crate may be mid-edit.
    cargo @cargo 2>&1 | Select-String -Pattern "EXPORTING|skia-bindings|error: failed to run custom build" | ForEach-Object { $_.Line }
    Pop-Location
} finally {
    if ($webSaved) { Restore-WebEnv $webSaved }
    foreach ($name in $saved.Keys) {
        if ($null -eq $saved[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $saved[$name], "Process") }
    }
}

$binaries = Join-Path $stage "skia-binaries"
if (-not (Test-Path (Join-Path $binaries "key.txt"))) { throw "no binaries were exported to $binaries" }
$key = (Get-Content (Join-Path $binaries "key.txt") -Raw).Trim()
$archive = Join-Path $out "skia-binaries-$key.tar.gz"
# Windows' own tar: started from Git Bash, `tar` is GNU tar, which reads "C:" as a remote host.
& (Join-Path $env:SystemRoot "System32\tar.exe") -czf $archive -C $stage skia-binaries
if ($LASTEXITCODE -ne 0) { throw "tar failed" }
Remove-Item -Recurse -Force $stage
Get-Item $archive | Select-Object Name, Length
