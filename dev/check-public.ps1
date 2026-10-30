# Checks the staged text of the given files against the internal markers of public-guard.ps1
# (called by dev/hooks/pre-commit for skills/*/SKILL.md). Exit code 1 when one looks internal.
. "$PSScriptRoot\public-guard.ps1"
$failed = $false
foreach ($path in $args) {
    $text = (git show ":$path") -join "`n"
    try { Assert-Public $text $path } catch { Write-Host $_.Exception.Message; $failed = $true }
}
if ($failed) { exit 1 }
