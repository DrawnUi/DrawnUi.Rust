# Compiles drawnui-add3d's Vulkan shaders (drawnui-add3d/src/render/vulkan/*.vert, *.frag) to the
# SPIR-V files next to them, which the crate includes. Run after changing a shader; it needs the
# Android NDK (its shader-tools has glslc and spirv-val). The other back ends keep their shaders as
# text in render.rs (GLSL) and render/metal.rs (MSL): change all three together.
$ErrorActionPreference = "Stop"
$ndk = $env:ANDROID_NDK_HOME
if (-not $ndk) { throw "ANDROID_NDK_HOME is not set" }
$tools = Get-ChildItem (Join-Path $ndk "shader-tools") -Directory | Select-Object -First 1
$glslc = Join-Path $tools.FullName "glslc"
$validate = Join-Path $tools.FullName "spirv-val"
$dir = Join-Path $PSScriptRoot "..\drawnui-add3d\src\render\vulkan"
foreach ($shader in Get-ChildItem $dir -Include *.vert, *.frag -Recurse) {
    $out = "$($shader.FullName).spv"
    & $glslc --target-env=vulkan1.0 -O $shader.FullName -o $out
    if ($LASTEXITCODE) { throw "glslc failed on $($shader.Name)" }
    & $validate $out
    if ($LASTEXITCODE) { throw "spirv-val failed on $($shader.Name).spv" }
    "$($shader.Name) -> $($shader.Name).spv"
}
