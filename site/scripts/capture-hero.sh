#!/usr/bin/env bash
# The app's windows on the page, and the link preview's (og-card.html): each
# a window in the middle of a 2700x1800 screen on a desktop's default
# wallpaper, as that desktop composites it, so what shows through the glass
# is the real thing. A desktop has its own machine, so this script only turns
# what they made into src/assets/hero/*.webp, and then cuts the window out of
# some for the page's cards (crop-windows.mjs):
#
#   CHROME=/path/to/chromium site/scripts/capture-hero.sh SHOTS_DIR
#
# SHOTS_DIR holds stream.png, host.png, hosts.png, agent.png and
# devices.png, each 2700x1800, agent-screen.png and stats.txt. Each comes from a build
# of this checkout:
#
# - The Mac's way: hero/macos/wall.swift shows a wallpaper as a window behind
#   Helio, System Events places the window, screencapture -R takes the screen
#   there, 1350x900 points with the window 115 and 80 points in. Wallpaper:
#   macOS Tahoe's light one, the middle 16:9 of NeptuneOneWallpaper.appex's
#   TahoeLight.heic.
# - stream.png and host.png, the hero: one real stream, on a Mac alone. A
#   second copy of the installed Helio Host.app, with this checkout's
#   helio-host in it and signed again with the same Developer ID (so the
#   Mac's Screen Recording grant applies), runs with a HELIO_HOST_DATA_DIR
#   of its own (name gaming-pc, ports 47810-47812, internet_access and
#   port_mapping off), and a client with a HELIO_DATA_DIR of its own is
#   paired to it by
#   keys (helio host add-client macbook, helio add-host gaming-pc
#   127.0.0.1:47810). helio stream gaming-pc --size 2240x1480 --fps 120
#   --windowed --stats --keep-host-displays makes the host's virtual display,
#   which wall.swift's host mode covers with Windows 11's dark Bloom, over
#   the menu bar, two of its corner pixels changing shade at each refresh so
#   frames flow as a game's would. stream.png: the stream's window at its own
#   1120x773, 115 and 64 points in, over Tahoe. host.png: helio-app with that
#   HELIO_HOST_DATA_DIR and HELIO_UI_DEMO=host=overview,size=1120x740, over
#   the same Bloom, in the Mac's way; this script then lays it out as
#   helio-ui does on Windows, so the host reads as the Windows PC it stands
#   for: the sidebar's rows and the page moved up to where Windows has them,
#   over the traffic lights, and Windows 11's caption buttons drawn
#   (chrome.rs, theme.rs). Then each becomes a whole screen for the
#   devices the page draws round them (hero/screens.html). The host's is
#   that window above Windows 11's taskbar, cut to the stream's 2240:1480.
#   The stream's window then shows that screen, as a stream of that PC
#   would: the stream's own picture in stream.png gives way to it, under
#   the client's statistics, which stats.swift draws again as
#   helio-client/src/mac/text.rs does from stats.txt, the twelve lines the
#   shot shows (they match the shot's own text in 99% of its pixels). The
#   Mac's screen is that, under a MacBook's menu bar and notch, above its
#   Dock, with dock.swift's icons as this Mac draws them. CHROME is a
#   Chromium browser's program, which renders the screens headless at 2x on
#   this Mac, whose system font has the menu bar's Apple logo.
# - hosts.png: the Mac's way, with HELIO_UI_DEMO=hosts-sample,size=1120x740:
#   the Hosts page with four sample hosts, for the One app card.
# - agent-screen.png (2560x1600), the screen the agent's window shows: a
#   Windows VM, hero/windows/agentshot.ps1, the desktop the sample session
#   talks about. The page does not show it by itself;
#   src/assets/hero/agent-screen.webp keeps it, for this and for
#   tools/capture-docs-images.
# - agent.png: the Mac's way, with HELIO_UI_DEMO=agents=sample-ask,size=1120x740
#   and HELIO_UI_DEMO_SCREEN=agent-screen.png, so the window shows that screen.
# - devices.png: the Mac's way, with
#   HELIO_UI_DEMO=hosting=on,host-sample,host=devices,size=1120x740 and empty
#   HELIO_DATA_DIR (onboarded = true) and HELIO_HOST_DATA_DIR folders: This
#   computer → Devices, a pairing request above the paired devices.
set -euo pipefail
SITE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SHOTS=${1:?usage: CHROME=/path/to/chromium capture-hero.sh SHOTS_DIR}
: "${CHROME:?set CHROME to the program of a Chromium browser (see the top of this file)}"
# host.png laid out as on Windows, in pixels at 2x, the window 230 px in and
# 160 down. The sidebar's top lane is 8 points there against a Mac's 44, so
# its rows (432 px wide) go 72 px up; the page's strip is 32 points against
# 44, so the page goes 24 px up. Each moved block fades out at its foot into
# the glass under it, below where the rows were, so no step shows. Then
# Windows 11's caption buttons, 46x32 points each with 10-point glyphs, along
# the top right.
magick "$SHOTS/host.png" -crop 428x1152+232+248 +repage \
    \( -size 428x1054 xc:white -size 428x98 gradient:white-black -append \) \
    -alpha off -compose CopyOpacity -composite "$SHOTS/host-sidebar.png"
magick "$SHOTS/host.png" -crop 1804x1252+664+248 +repage \
    \( -size 1804x1176 xc:white -size 1804x76 gradient:white-black -append \) \
    -alpha off -compose CopyOpacity -composite "$SHOTS/host-page.png"
magick "$SHOTS/host.png" "$SHOTS/host-sidebar.png" -geometry +232+176 -composite \
    "$SHOTS/host-page.png" -geometry +664+224 -composite \
    -fill none -stroke '#e6e6e6' -strokewidth 2 \
    -draw 'line 2230,192 2250,192' -draw 'roundrectangle 2322,182 2342,202 3,3' \
    -draw 'line 2414,182 2434,202' -draw 'line 2434,182 2414,202' "$SHOTS/host-windows.png"
swiftc -O -o "$SHOTS/dock" "$SITE/scripts/hero/macos/dock.swift"
swiftc -O -o "$SHOTS/stats" "$SITE/scripts/hero/macos/stats.swift"
mkdir -p "$SHOTS/dock-icons"
"$SHOTS/dock" "$SHOTS/dock-icons"
screen() { # OS SHOT OUT
    "$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
        --window-size=1350,900 --virtual-time-budget=3000 --screenshot="$3" \
        "file://$SITE/scripts/hero/screens.html?os=$1&shot=$2&icons=$SHOTS/dock-icons" >/dev/null 2>&1
}
# The PC's screen, cut to the stream's 2240:1480: 16 px of wallpaper off its
# top.
screen windows "$SHOTS/host-windows.png" "$SHOTS/host-desktop.png"
magick "$SHOTS/host-desktop.png" -gravity south -crop 2700x1784+0+0 +repage "$SHOTS/host-screen.png"
# The stream's picture is 2240x1480 at 230,192 in stream.png, one pixel to
# each of the stream's, the statistics 24 px into it (the client's 12-point
# margin), and the window's 2-pixel border and bottom corners (34 px inside
# it) over it: the PC's screen takes its place inside those.
"$SHOTS/stats" "$SHOTS/stats.txt" "$SHOTS/stats.png" >/dev/null
magick "$SHOTS/host-screen.png" -resize '2240x1480!' "$SHOTS/stats.png" -geometry +24+24 -composite \
    \( -size 2240x1480 xc:black -fill white -draw 'roundrectangle 2,-40 2237,1479 34,34' \) \
    -geometry +0+0 -alpha off -compose CopyOpacity -composite "$SHOTS/stream-picture.png"
magick "$SHOTS/stream.png" "$SHOTS/stream-picture.png" -geometry +230+192 -composite \
    "$SHOTS/stream-hosted.png"
screen macos "$SHOTS/stream-hosted.png" "$SHOTS/stream-screen.png"
for name in stream host hosts agent devices; do
    from=$SHOTS/$name.png
    [ "$name" = stream ] && from=$SHOTS/stream-screen.png
    [ "$name" = host ] && from=$SHOTS/host-screen.png
    node -e 'require(process.argv[1])(process.argv[2]).webp({ quality: 88, effort: 6 }).toFile(process.argv[3])' \
        "$SITE/node_modules/sharp" "$from" "$SITE/src/assets/hero/$name.webp"
    echo "$name"
done
node "$SITE/scripts/crop-windows.mjs"
