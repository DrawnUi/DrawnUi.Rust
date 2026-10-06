# The environment of a browser build (wasm32-unknown-emscripten), dot-sourced by build.ps1 and
# skia-binaries.ps1:
#   $saved = Set-WebEnv -Root $root -Emsdk $emsdk [-BuildSkia]
#   ... cargo ... --target wasm32-unknown-emscripten
#   Restore-WebEnv $saved
#
# -BuildSkia compiles Skia from source with wasm exceptions, to match what rustc links with: the
# prebuilt wasm Skia of rust-skia uses the old exception ABI (rust-skia issue 1287). Without it,
# Skia comes from the binaries archive `.cargo\config.toml` points at (skia-binaries.ps1 -Web).

$WebEnvNames = "PATH", "EMSDK", "EMSDK_NODE", "EMSDK_PYTHON", "EMSDK_QUIET", "FORCE_SKIA_BUILD", "SKIA_GN_ARGS",
    "CC_wasm32_unknown_emscripten", "CXX_wasm32_unknown_emscripten", "AR_wasm32_unknown_emscripten",
    "CFLAGS_wasm32_unknown_emscripten", "CXXFLAGS_wasm32_unknown_emscripten"

function Set-WebEnv([string]$Root, [string]$Emsdk, [switch]$BuildSkia) {
    $saved = @{}
    foreach ($n in $WebEnvNames) { $saved[$n] = [Environment]::GetEnvironmentVariable($n) }

    # Puts the emsdk tools on PATH and sets EMSDK.
    $env:EMSDK_QUIET = "1"
    & "$Emsdk\emsdk_env.ps1" | Out-Null

    # Skia's build calls `python3`. Where that is only the Microsoft Store stub, forward it to `python`.
    cmd /c "python3 --version >nul 2>&1"
    if ($LASTEXITCODE) {
        $shim = "$Root\target\shim"
        New-Item -ItemType Directory -Force $shim | Out-Null
        Set-Content "$shim\python3.bat" '@python %*' -Encoding ascii
        $env:PATH = "$shim;$env:PATH"
    }

    # The tools are named by their .exe: Skia's Windows toolchain and the cc crate both default to
    # .bat files, which emsdk 6 no longer ships.
    $tools = "$($Emsdk -replace '\\', '/')/upstream/emscripten"
    if ($BuildSkia) {
        $env:FORCE_SKIA_BUILD = "1"
        $env:SKIA_GN_ARGS = 'skia_emsdk_dir="" cc="' + $tools + '/emcc.exe" cxx="' + $tools + '/em++.exe" ar="' + $tools +
            '/emar.exe" extra_cflags_c=["-fwasm-exceptions"] extra_cflags_cc=["-fwasm-exceptions"]'
    }
    $env:CC_wasm32_unknown_emscripten = "$tools/emcc.exe"
    $env:CXX_wasm32_unknown_emscripten = "$tools/em++.exe"
    $env:AR_wasm32_unknown_emscripten = "$tools/emar.exe"
    $env:CFLAGS_wasm32_unknown_emscripten = "-fwasm-exceptions"
    $env:CXXFLAGS_wasm32_unknown_emscripten = "-fwasm-exceptions"
    $saved
}

# A variable that did not exist must be removed, not set to an empty string: the Skia build script
# only checks that FORCE_SKIA_BUILD exists.
function Restore-WebEnv([hashtable]$Saved) {
    foreach ($n in $WebEnvNames) {
        if ($null -eq $Saved[$n]) { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
        else { Set-Item "Env:$n" $Saved[$n] }
    }
}

# The wasm Skia archive skia-binaries.ps1 -Web made for the current rust-skia commit, if any.
function Get-WebSkiaArchive([string]$Root) {
    $commit = (git -C "$Root\..\rust-skia" rev-parse HEAD).Substring(0, 20)
    Get-ChildItem "$Root\target\skia-bin" -Filter "skia-binaries-$commit-wasm32-unknown-emscripten-*.tar.gz" -ErrorAction SilentlyContinue |
        Select-Object -First 1
}
