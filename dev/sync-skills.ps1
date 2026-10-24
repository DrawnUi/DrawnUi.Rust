# Copies the maintainer's local public skills (~/.claude/skills/<name>/SKILL.md) into
# skills\<name>\ and refuses to copy anything that looks internal (machine paths, accounts,
# tokens, dev tooling). The web build of hellorust then serves them at /skills/<name>/SKILL.md.
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\public-guard.ps1"
$root = Split-Path $PSScriptRoot -Parent
foreach ($name in @("drawnui-rust", "drawnui-port-net-to-rust")) {
    $source = Join-Path $HOME ".claude\skills\$name\SKILL.md"
    if (-not (Test-Path $source)) { throw "local skill not found: $source" }
    $text = [IO.File]::ReadAllText($source)
    Assert-Public $text $source
    New-Item -ItemType Directory -Force "$root\skills\$name" | Out-Null
    Copy-Item $source "$root\skills\$name\SKILL.md" -Force
    Write-Host "synced $source -> skills\$name\SKILL.md ($($text.Length) chars, clean)"
}
