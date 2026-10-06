# Makes the assets of a drawnui release in target\release\<version>\ (nothing is published):
#   dev\release.ps1 -Version 0.1.0-preview.1
#
# - Skia: the archive of every target, keyed by the commit drawnui-skia-bindings was published
#   from (its .cargo_vcs_info.json): inside a crate skia-bindings keys its download by it. The
#   sibling ..\rust-skia is checked out there, so dev\skia-binaries.ps1 gives the archives that
#   name. The Mac makes the Apple ones (dev/release-apple.sh) and uploads them.
# - drawnui-rust-<version>-src.tar.gz: the committed tree (git archive) with a .commit file, for
#   builds without access to the repository.
# - SHA256SUMS.txt over every file here.
param(
    [Parameter(Mandatory)][string]$Version
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root "target\release\$Version"

# The commit the pinned drawnui-skia-bindings was published from; the sibling checkout that builds
# the archives must be on it.
$skiaVersion = (Select-String -Path "$root\Cargo.toml" -Pattern 'package = "drawnui-skia-safe", version = "=([0-9.]+)"').Matches[0].Groups[1].Value
$crate = Get-ChildItem "$env:USERPROFILE\.cargo\registry\src\*\drawnui-skia-bindings-$skiaVersion" -Directory | Select-Object -First 1
if (-not $crate) { throw "drawnui-skia-bindings $skiaVersion is not in the cargo registry: cargo fetch in an app that uses drawnui" }
$pin = (Get-Content "$($crate.FullName)\.cargo_vcs_info.json" | ConvertFrom-Json).git.sha1
$sibling = git -C (Join-Path $root "..\rust-skia") rev-parse HEAD
if ($sibling -ne $pin) { throw "..\rust-skia is at $sibling, drawnui-skia-bindings $skiaVersion was published from ${pin}: its archives would be keyed for another commit" }
# rust-skia keys with the first 20 characters of the hash.
$key = $pin.Substring(0, 20)

$commit = git -C $root rev-parse HEAD
if (git -C $root status --porcelain --untracked-files=no) { Write-Warning "uncommitted changes: the source archive is $commit without them" }

Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $out | Out-Null

# The archive drawnui needs per target: its default features (svg pulls textlayout), Vulkan on
# Android, Metal on Apple targets.
$base = "ganesh-gl-jpegd-jpege-pdf-svg-textlayout"
$wanted = [ordered]@{
    "x86_64-pc-windows-msvc"      = $base
    "x86_64-unknown-linux-gnu"    = $base
    "wasm32-unknown-emscripten"   = $base
    "aarch64-linux-android"       = "$base-vulkan"
    "armv7-linux-androideabi"     = "$base-vulkan"
    "x86_64-linux-android"        = "$base-vulkan"
    "aarch64-apple-darwin"        = "ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout"
    "aarch64-apple-ios"           = "ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout"
    "aarch64-apple-ios-sim"       = "ganesh-gl-jpegd-jpege-metal-pdf-svg-textlayout"
}
$missing = @()
foreach ($target in $wanted.Keys) {
    $name = "skia-binaries-$key-$target-$($wanted[$target]).tar.gz"
    $file = Join-Path $root "target\skia-bin\$name"
    if (Test-Path $file) { Copy-Item $file $out } else { $missing += $name }
}

$src = Join-Path $out "drawnui-rust-$Version-src.tar.gz"
git -C $root archive --format=tar.gz --prefix="drawnui-rust-$Version/" --add-virtual-file="drawnui-rust-$Version/.commit:$commit`n" -o $src HEAD
if ($LASTEXITCODE) { throw "git archive failed" }

Push-Location $out
try {
    Get-ChildItem -File | Where-Object Name -ne "SHA256SUMS.txt" | Sort-Object Name | ForEach-Object {
        "$((Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower())  $($_.Name)"
    } | Set-Content SHA256SUMS.txt -Encoding ascii
}
finally {
    Pop-Location
}

Write-Host "release $Version from $commit, Skia key $key (drawnui-skia-bindings $skiaVersion, DrawnUi/rust-skia $pin)"
Get-ChildItem $out | ForEach-Object { "{0,12:N0}  {1}" -f $_.Length, $_.Name }
if ($missing) {
    Write-Warning "not here (dev\skia-binaries.ps1; Apple targets: dev/release-apple.sh on the Mac, which uploads them):`n  $($missing -join "`n  ")"
}
