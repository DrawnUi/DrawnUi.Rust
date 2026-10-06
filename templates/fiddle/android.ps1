# Android: builds libmyapp.so with cargo-ndk, packs the APK with Gradle (android\, GameActivity) and,
# with -Install, installs and starts it on the phone or emulator adb sees.
#   .\android.ps1                         -> arm64-v8a (phones): target\android\myapp.apk
#   .\android.ps1 -Abi x86_64 -Install    -> the emulator
#   .\android.ps1 -Abi all                -> arm64-v8a, armeabi-v7a, x86_64 and x86 in one APK
# Needs: the Android SDK (ANDROID_HOME) with platform 36, NDK 26, a JDK 17 or newer (JAVA_HOME,
# Android Studio's jbr works), cargo-ndk (cargo install cargo-ndk) and the Rust Android targets.
param(
    [ValidateSet("arm64-v8a", "armeabi-v7a", "x86_64", "x86", "all")]
    [string]$Abi = "arm64-v8a",
    [switch]$Install
)
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    # NDK 26: the prebuilt Skia is built with it, and the app links Skia's C++ library from it.
    # ANDROID_NDK_HOME is used when it is a 26, else the SDK's NDK 26.
    if (-not $env:ANDROID_NDK_HOME -or (Split-Path -Leaf $env:ANDROID_NDK_HOME) -notlike "26.*") {
        $ndk = Get-ChildItem (Join-Path $env:ANDROID_HOME "ndk") -Directory -ErrorAction SilentlyContinue | Where-Object Name -like "26.*" | Select-Object -Last 1
        if (-not $ndk) { throw "NDK 26 not found: install it with the SDK Manager (ndk;26.1.10909125)" }
        $env:ANDROID_NDK_HOME = $ndk.FullName
    }
    if (-not $env:JAVA_HOME) { $env:JAVA_HOME = Join-Path $env:ProgramFiles "Android\Android Studio\jbr" }
    $abis = if ($Abi -eq "all") { "arm64-v8a", "armeabi-v7a", "x86_64", "x86" } else { @($Abi) }
    $targets = $abis | ForEach-Object { "-t"; $_ }

    # Only this build's libraries go into the APK.
    Remove-Item -Recurse -Force target\android\lib, target\android\stage -ErrorAction SilentlyContinue
    cargo ndk @targets --platform 26 -o target\android\lib rustc --lib --release --crate-type cdylib
    if ($LASTEXITCODE) { throw "cargo ndk failed" }
    # The app reads its files as "assets/...": they go to the APK as assets/assets/...
    New-Item -ItemType Directory -Force target\android\stage | Out-Null
    Copy-Item -Recurse assets target\android\stage\assets

    & android\gradlew.bat -p android --project-cache-dir "$PSScriptRoot\target\android\gradle-cache" -q assembleRelease
    if ($LASTEXITCODE) { throw "gradle failed" }
    Copy-Item target\android\gradle\outputs\apk\release\myapp-release.apk target\android\myapp.apk -Force
    Write-Host "APK: $PSScriptRoot\target\android\myapp.apk"

    if ($Install) {
        adb install -r target\android\myapp.apk
        if ($LASTEXITCODE) { throw "adb install failed" }
        adb shell am start -n com.example.myapp/com.google.androidgamesdk.GameActivity
    }
}
finally {
    Pop-Location
}
