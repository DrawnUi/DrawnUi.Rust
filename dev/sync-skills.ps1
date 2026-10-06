# Copies the maintainer's local public skill (~/.claude/skills/drawnui-rust/SKILL.md) into
# skills\drawnui-rust\ and refuses to copy anything that looks internal (machine paths, accounts,
# tokens, dev tooling). The web build of hellorust then serves it at /skills/drawnui-rust/SKILL.md.
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\public-guard.ps1"
$root = Split-Path $PSScriptRoot -Parent
$source = Join-Path $HOME ".claude\skills\drawnui-rust\SKILL.md"
if (-not (Test-Path $source)) { throw "local skill not found: $source" }
$text = [IO.File]::ReadAllText($source)
Assert-Public $text $source
New-Item -ItemType Directory -Force "$root\skills\drawnui-rust" | Out-Null
Copy-Item $source "$root\skills\drawnui-rust\SKILL.md" -Force
Write-Host "synced $source -> skills\drawnui-rust\SKILL.md ($($text.Length) chars, clean)"
