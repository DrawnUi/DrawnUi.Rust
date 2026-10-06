# Markers that must never appear in anything published from this repo (skills, llms files).
# Dot-source it and call Assert-Public on the text before it ships.
$PublicMarkers = @(
    'C:\\Dev\\', 'C:/Dev/', 'C:\\Users', 'C:/Users', 'scratchpad', 'localhost:9222', 'connectOverCDP',
    'wrangler', 'CLOUDFLARE', 'cloudflare pages', 'npm_[A-Za-z0-9]{20,}', '_authToken', 'dev@drawnui\.net',
    'taublast', 'gmail', 'bypass 2FA', 'granular token', 'PROGRESS\.md', 'DEVELOPMENT\.md', 'gh run watch'
)

function Assert-Public([string]$Text, [string]$Label) {
    $hits = $PublicMarkers | Where-Object { $Text -match $_ }
    if ($hits) { throw "$Label contains internal markers: $($hits -join ', ')" }
}
