# Builds an example for the desktop and for the browser.
#   dev\build.ps1                      -> bench, both targets
#   dev\build.ps1 -Example hello
#   dev\build.ps1 -Features full       -> with text layout + SVG (payload measurement)
#   dev\build.ps1 -WebOnly
# Web output: target\web\<example>\ (serve that folder over http).
#
# The browser build takes Skia from the wasm binaries archive of skia-binaries.ps1 -Web, else
# compiles it from source (about 6 minutes, once): the prebuilt Skia that rust-skia publishes for
# wasm uses the old emscripten exception ABI, which cannot be linked with Rust 1.93+ (rust-skia
# issue 1287). Needs emsdk 5+; a source build also needs Python, Ninja and LLVM (libclang).
param(
    [string]$Example = "bench",
    [string]$Features = "",
    [switch]$WebOnly,
    [string]$Emsdk = $(if ($env:EMSDK) { $env:EMSDK } else { "C:\Dev\Tools\emsdk" })
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $common = @("build", "--release", "-p", $Example)
    if ($Features) { $common += @("--features", $Features) }

    if (-not $WebOnly) {
        cargo @common
        if ($LASTEXITCODE) { exit $LASTEXITCODE }
    }

    # Everything below changes the environment for the browser build only. It is restored after,
    # so a later desktop build in the same shell is not forced into a Skia source build.
    . "$PSScriptRoot\web-env.ps1"
    # Skia from the wasm binaries archive when there is one for this rust-skia commit (skia-binaries.ps1
    # -Web; only for the default features), else compiled from source (about 6 minutes, once).
    $archive = if ($Features) { $null } else { Get-WebSkiaArchive $root }
    if ($archive) { Write-Host "wasm Skia: $($archive.Name)" }
    $saved = Set-WebEnv -Root $root -Emsdk $Emsdk -BuildSkia:(-not $archive)
    $code = 1
    try {
        cargo @common --target wasm32-unknown-emscripten
        $code = $LASTEXITCODE
    }
    finally {
        Restore-WebEnv $saved
    }
    if ($code) { exit $code }

    $out = "$root\target\web\$Example"
    New-Item -ItemType Directory -Force $out | Out-Null
    $built = "$root\target\wasm32-unknown-emscripten\release"
    Copy-Item "$built\$Example.js", "$built\$Example.wasm", "$root\drawnui\web\drawnui_host.js" $out -Force
    Copy-Item "$root\examples\$Example\web\*" $out -Recurse -Force
    Copy-Item "$root\examples\$Example\assets" $out -Recurse -Force

    # Version stamps. The page is checked with the server on every load, but a browser may keep
    # .js, images and fonts for hours (a zone's browser cache TTL): a returning visitor would run
    # the new wasm with the old JavaScript, or see an old picture. A changed file gets a new
    # address; an unchanged one keeps its address and stays cached. The wasm's address is in the
    # emscripten glue, the scripts' and the favicon's in the page, and the assets' in a map the page
    # hands the host (`duiAssetVersions`, read by drawnUiVersioned in drawnui_host.js).
    $stamp = { param($file) (Get-FileHash $file -Algorithm SHA256).Hash.Substring(0, 10).ToLower() }
    $glue = "$out\$Example.js"
    $wasmName = "`"$Example.wasm`""
    $text = Get-Content $glue -Raw
    if (-not $text.Contains($wasmName)) { throw "$glue does not name $wasmName" }
    Set-Content $glue $text.Replace($wasmName, "`"$Example.wasm?v=$(& $stamp "$out\$Example.wasm")`"") -NoNewline
    $glueStamp, $hostStamp = (& $stamp $glue), (& $stamp "$out\drawnui_host.js")
    $versions = [ordered]@{}
    foreach ($file in Get-ChildItem "$out\assets" -Recurse -File -ErrorAction SilentlyContinue) {
        $versions[[IO.Path]::GetRelativePath($out, $file.FullName).Replace('\', '/')] = & $stamp $file.FullName
    }
    $hostTag = '<script src="drawnui_host.js"></script>'
    $hostTags = "<script>window.duiAssetVersions = $(ConvertTo-Json $versions -Compress);</script>`n    " +
        "<script src=`"drawnui_host.js?v=$hostStamp`"></script>"
    foreach ($page in Get-ChildItem $out -Filter *.html) {
        $html = (Get-Content $page.FullName -Raw).Replace("src=`"$Example.js`"", "src=`"$Example.js?v=$glueStamp`"")
        if ($html.Contains('src="drawnui_host.js"') -and -not $html.Contains($hostTag)) { throw "$($page.Name): write the host script as $hostTag" }
        $html = $html.Replace($hostTag, $hostTags)
        if (Test-Path "$out\favicon.ico") { $html = $html.Replace('href="favicon.ico"', "href=`"favicon.ico?v=$(& $stamp "$out\favicon.ico")`"") }
        Set-Content $page.FullName $html -NoNewline
    }

    # Files for AI agents, when the example ships an llms.txt: the public skills (skills\, synced by
    # dev\sync-skills.ps1) served under skills/, and llms-full.txt = llms.txt with every skill inlined.
    # Refuses anything that looks internal.
    if (Test-Path "$out\llms.txt") {
        . "$PSScriptRoot\public-guard.ps1"
        $full = [IO.File]::ReadAllText("$out\llms.txt")
        Assert-Public $full llms.txt
        $skills = @(Get-ChildItem "$root\skills\*\SKILL.md" -ErrorAction SilentlyContinue)
        foreach ($skill in $skills) {
            $name = $skill.Directory.Name
            $text = [IO.File]::ReadAllText($skill.FullName)
            Assert-Public $text "skills/$name/SKILL.md"
            $full += "`n`n---`n`n# Skill: $name (skills/$name/SKILL.md)`n`n$text"
        }
        if ($skills) { Copy-Item "$root\skills" $out -Recurse -Force }
        [IO.File]::WriteAllText("$out\llms-full.txt", $full)
        Write-Host "agent files: $($skills.Count) skill(s), llms-full.txt $($full.Length) chars"

        # The engine's API docs under docs/ (cargo doc of the desktop build, its own target dir so no
        # other crate's docs come along), checked like the skills.
        $docTarget = "$root\target\a\docs"
        Remove-Item "$docTarget\doc", "$out\docs" -Recurse -Force -ErrorAction SilentlyContinue
        cargo doc --no-deps -p drawnui --target-dir $docTarget
        if ($LASTEXITCODE) { throw "cargo doc failed" }
        foreach ($file in Get-ChildItem "$docTarget\doc" -Recurse -File -Include *.html, *.js) {
            Assert-Public ([IO.File]::ReadAllText($file.FullName)) "docs/$($file.Name)"
        }
        Copy-Item "$docTarget\doc" "$out\docs" -Recurse
        Set-Content "$out\docs\index.html" '<!doctype html><meta http-equiv="refresh" content="0; url=drawnui/">' -NoNewline
        Write-Host "api docs: $((Get-ChildItem "$out\docs" -Recurse -File).Count) files"
    }
    Write-Host "web build: $out"
}
finally { Pop-Location }
