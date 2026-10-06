# The browser build: dist\ holds the page, myapp.js / .wasm, the assets and drawnui_host.js.
# Needs emsdk's tools on PATH first (emsdk_env.ps1) and `rustup target add wasm32-unknown-emscripten`.
$ErrorActionPreference = "Stop"
Push-Location $PSScriptRoot
try {
    cargo build --release --target wasm32-unknown-emscripten
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
    $built = "target\wasm32-unknown-emscripten\release"
    New-Item -ItemType Directory -Force dist | Out-Null
    Copy-Item "$built\myapp.js", "$built\myapp.wasm" dist -Force
    Copy-Item web\*, assets dist -Recurse -Force
    # The page script of the drawnui version this build linked: it and the wasm speak one protocol.
    $drawnui = (cargo metadata --format-version 1 | ConvertFrom-Json).packages | Where-Object name -eq "drawnui"
    Copy-Item (Join-Path (Split-Path $drawnui.manifest_path) "web\drawnui_host.js") dist -Force
    Write-Host "web build: $PSScriptRoot\dist (serve it over http)"
}
finally {
    Pop-Location
}
