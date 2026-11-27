#!/usr/bin/env bash
# The app's windows on the page, and the link preview's (og-card.html): each
# a window in the middle of a 2700x1800 screen on a desktop's default
# wallpaper, as that desktop composites it, so what shows through the glass
# is the real thing. A desktop has its own machine, so this script only turns
# what they made into src/assets/hero/*.webp, and then cuts the window out of
# some for the page's cards (crop-windows.mjs):
#
#   site/scripts/capture-hero.sh SHOTS_DIR
#
# SHOTS_DIR holds stream.png, host.png, hosts.png, agent.png and
# devices.png, each 2700x1800, and agent-screen.png. Each comes from a build
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
#   (chrome.rs, theme.rs).
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
SHOTS=${1:?usage: capture-hero.sh SHOTS_DIR}
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
for name in stream host hosts agent devices; do
    from=$SHOTS/$name.png
    [ "$name" = host ] && from=$SHOTS/host-windows.png
    node -e 'require(process.argv[1])(process.argv[2]).webp({ quality: 88, effort: 6 }).toFile(process.argv[3])' \
        "$SITE/node_modules/sharp" "$from" "$SITE/src/assets/hero/$name.webp"
    echo "$name"
done
node "$SITE/scripts/crop-windows.mjs"
