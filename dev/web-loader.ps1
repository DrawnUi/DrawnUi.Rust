# The loading screen of every DrawnUI for Rust web page (dot-sourced by dev\build.ps1): the app's
# title over a glow and the real download progress, as the DrawnUI Blazor heads show it. The web
# host (drawnui_host.js) fills the progress from the wasm's bytes and removes the screen with the
# app's first drawn frame. A page can recolor it with the --drawnui-loader-* variables in its own
# style, and name it with <meta name="drawnui-loader-title" content="...">; without that the title
# is og:site_name, else <title>.

$script:LoaderStyle = @'
    <style>
      .drawnui-loader { position: fixed; inset: 0; z-index: 10; display: flex; flex-direction: column; align-items: center;
                        justify-content: center; gap: 22px; transition: opacity 0.35s;
                        background: var(--drawnui-loader-background, radial-gradient(ellipse at 50% 55%, #2a1406 0%, #000 65%)); }
      .drawnui-loader.done { opacity: 0; pointer-events: none; }
      .drawnui-loader-title { font: 800 44px var(--drawnui-loader-font, system-ui, sans-serif); letter-spacing: 2px; text-align: center;
                              padding: 0 16px; background: var(--drawnui-loader-title, linear-gradient(#fff4d0, #ffc050 55%, #d06810));
                              -webkit-background-clip: text; background-clip: text; color: transparent;
                              animation: drawnui-loader-glow 1.6s ease-in-out infinite; }
      .drawnui-loader-bar { width: min(260px, 60vw); height: 4px; border-radius: 2px; overflow: hidden;
                            background: var(--drawnui-loader-track, #2a1d10); }
      .drawnui-loader-fill { height: 100%; width: var(--drawnui-load-percentage, 0%); transition: width 0.2s;
                             background: var(--drawnui-loader-fill, linear-gradient(90deg, #d06810, #ffc050));
                             box-shadow: 0 0 10px var(--drawnui-loader-glow, #ff8a20); }
      .drawnui-loader-text { font: 12px var(--drawnui-loader-font, system-ui, sans-serif); letter-spacing: 3px;
                             color: var(--drawnui-loader-text, #8a7a66); }
      .drawnui-loader-text::after { content: " " var(--drawnui-load-percentage-text, ""); }
      @keyframes drawnui-loader-glow {
        0%, 100% { filter: drop-shadow(0 0 10px var(--drawnui-loader-glow-soft, rgba(255, 138, 32, 0.45))); }
        35% { filter: drop-shadow(0 0 22px var(--drawnui-loader-glow-strong, rgba(255, 138, 32, 0.8))); }
        60% { filter: drop-shadow(0 0 14px var(--drawnui-loader-glow-soft, rgba(255, 110, 20, 0.55))); }
      }
      @media (prefers-reduced-motion: reduce) { .drawnui-loader-title { animation: none; } }
    </style>
'@

# The page with the loading screen in it, unless it has one already. $WasmBytes: the size of the
# app's wasm, for the progress.
function Add-WebLoader([string]$Html, [long]$WasmBytes) {
    if ($Html.Contains('class="drawnui-loader')) { return $Html }
    $decode = { param($text) [System.Net.WebUtility]::HtmlDecode($text) }
    $title = $null
    foreach ($pattern in '<meta\s+name="drawnui-loader-title"\s+content="([^"]*)"', '<meta\s+property="og:site_name"\s+content="([^"]*)"', '<title>([^<]*)</title>') {
        $m = [regex]::Match($Html, $pattern)
        if ($m.Success -and $m.Groups[1].Value.Trim()) { $title = (& $decode $m.Groups[1].Value).Trim(); break }
    }
    if (-not $title) { $title = "DrawnUI" }
    $encoded = [System.Net.WebUtility]::HtmlEncode($title)
    $markup = "<div class=`"drawnui-loader`" role=`"progressbar`" aria-label=`"Loading $encoded`" aria-valuemin=`"0`" aria-valuemax=`"100`" data-wasm-bytes=`"$WasmBytes`">`n" +
        "      <div class=`"drawnui-loader-title`">$([System.Net.WebUtility]::HtmlEncode($title.ToUpperInvariant()))</div>`n" +
        "      <div class=`"drawnui-loader-bar`"><div class=`"drawnui-loader-fill`"></div></div>`n" +
        "      <div class=`"drawnui-loader-text`">LOADING</div>`n" +
        "    </div>"
    if (-not $Html.Contains('</head>')) { throw "a page without </head>" }
    $Html = $Html.Replace('</head>', "$script:LoaderStyle  </head>")
    $body = [regex]'<body([^>]*)>'
    if (-not $body.IsMatch($Html)) { throw "a page without <body>" }
    return $body.Replace($Html, "<body`$1>`n    $markup", 1)
}
