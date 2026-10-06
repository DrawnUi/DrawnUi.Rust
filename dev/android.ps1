# Builds an example for Android (cargo-ndk) and packs the APK with Gradle (GameActivity), then
# installs and starts it on the device or emulator adb sees (one only). An example <x> has its
# Android crate in examples\<x>-android: the cdylib with android_main and the Gradle app.
#   dev\android.ps1                       -> HelloRust, x86_64 (the emulator), build + pack + install + start
#   dev\android.ps1 -Example dungeon      -> Dungeon Run
#   dev\android.ps1 -Abi arm64-v8a        -> a phone
#   dev\android.ps1 -Abi all -NoInstall   -> one APK with arm64, armv7 and x86_64, not installed
#   dev\android.ps1 -Debug                -> a debug build (the FPS label), optimized
# Needs: cargo-ndk, rustup targets for Android, NDK 26 and platform 36 in ANDROID_HOME, a JDK 17+
# (JAVA_HOME, else Android Studio's), the Android Skia archives (dev\skia-binaries.ps1 -Android
# <target>). Output: target\android\hellorust.apk; other examples: target\android\<x>\<x>.apk.
param(
    [string]$Example = "hellorust",
    [ValidateSet("x86_64", "arm64-v8a", "armeabi-v7a", "x86", "all")]
    [string]$Abi = "x86_64",
    [switch]$NoInstall,
    [switch]$Debug
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$app = Join-Path $repo "examples\$Example-android"
# HelloRust keeps target\android (its build.gradle names it); every other example has a folder in it.
$out = if ($Example -eq "hellorust") { Join-Path $repo "target\android" } else { Join-Path $repo "target\android\$Example" }
$sdk = $env:ANDROID_HOME
# NDK 26: the Skia archives were built with it (NDK 29 does not compile Skia's ICU).
$ndk = Get-ChildItem (Join-Path $sdk "ndk") -Directory | Where-Object Name -like "26.*" | Select-Object -Last 1
# "all" leaves x86 (32-bit) out: skia-bindings' generated layouts do not match rustc's for
# i686-linux-android (100 vs 104 bytes), and such devices are practically gone. Its Skia archive exists.
$abis = if ($Abi -eq "all") { "arm64-v8a", "armeabi-v7a", "x86_64" } else { @($Abi) }

$saved = @{}
foreach ($name in "ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "CARGO_TARGET_DIR", "JAVA_HOME") {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}
try {
    foreach ($name in "ANDROID_NDK", "ANDROID_NDK_HOME", "ANDROID_NDK_ROOT") {
        [Environment]::SetEnvironmentVariable($name, $ndk.FullName, "Process")
    }
    # A short target dir of its own (the desktop one keeps its Windows build).
    $env:CARGO_TARGET_DIR = Join-Path $repo "target\a\android"
    # An array, also with one item: splatting a plain string passes it character by character.
    # -Debug: the devrel profile, optimized with debug_assertions (the debug FPS label shows).
    [string[]]$release = if ($Debug) { @("--profile", "devrel") } else { @("--release") }
    $targets = $abis | ForEach-Object { "-t"; $_ }
    # Only the ABIs of this build go into the APK: a library left from another build would ship old code.
    Remove-Item -Recurse -Force (Join-Path $out "lib") -ErrorAction SilentlyContinue
    Push-Location $repo
    cargo ndk @targets --platform 26 -o (Join-Path $out "lib") build -p "$Example-android" @release
    if ($LASTEXITCODE) { throw "cargo ndk failed" }
    Pop-Location

    # The app's files are read as "assets/...": they go to the APK's assets/assets.
    $stage = Join-Path $out "stage"
    Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force (Join-Path $stage "assets") | Out-Null
    Copy-Item -Recurse (Join-Path $repo "examples\$Example\assets") (Join-Path $stage "assets\assets")

    # The APK (examples\hellorust-android\build.gradle): the libraries, the assets and GameActivity;
    # not debuggable, signed with the debug key. Gradle's own files stay under target\android.
    if (-not $env:JAVA_HOME) { $env:JAVA_HOME = Join-Path $env:ProgramFiles "Android\Android Studio\jbr" }
    & (Join-Path $app "gradlew.bat") -p $app --project-cache-dir (Join-Path $out "gradle-cache") -q assembleRelease
    if ($LASTEXITCODE) { throw "gradle failed" }
    Remove-Item -Recurse -Force $stage
} finally {
    # An unset variable is removed again (PowerShell would pass $null as "", leaving it empty).
    foreach ($name in $saved.Keys) {
        if ($null -eq $saved[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $saved[$name], "Process") }
    }
}

$apk = Join-Path $out "$Example.apk"
Copy-Item (Join-Path $out "gradle\outputs\apk\release\$Example-release.apk") $apk -Force
Get-Item $apk | Select-Object Name, Length

if (-not $NoInstall) {
    $adb = Join-Path $sdk "platform-tools\adb.exe"
    & $adb install -r $apk
    if ($LASTEXITCODE) { throw "adb install failed" }
    & $adb shell am start -n "net.drawnui.$Example/com.google.androidgamesdk.GameActivity"
}
