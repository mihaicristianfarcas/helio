#!/usr/bin/env bash
# The app's windows on the page, and the link preview's (og-card.html): one
# window (HELIO_UI_DEMO's hosts-sample, 1120x740 points, dark) in the middle of
# a 2700x1800 screen on the desktop's default wallpaper, as that desktop
# composites it, so what shows through the glass is the real thing. A desktop
# has its own machine, so this script only turns what they made into
# src/assets/hero/*.webp, and then cuts the window out of each for the page
# (crop-windows.mjs):
#
#   site/scripts/capture-hero.sh SHOTS_DIR
#
# SHOTS_DIR holds macos.png, windows.png, agent.png and devices.png, each
# 2700x1800, and agent-screen.png. Each comes from a demo build of this
# checkout:
#
# - macOS (this Mac): hero/macos/wall.swift shows the wallpaper as a window
#   behind Helio, System Events places the window, screencapture -R takes the
#   screen there, 1350x900 points with the window 115 and 80 points in.
#   Wallpaper: macOS Tahoe's light one, the middle 16:9 of
#   NeptuneOneWallpaper.appex's TahoeLight.heic.
# - Windows (a PC with a GPU, which acrylic needs; with SudoVDA installed, as
#   hosting installs it): hero/windows/, each script run in the signed-in
#   session (tools/host-run.ps1 -Limited, from SSH), from C:\pp\hero holding
#   a release helio-app.exe beside FFmpeg's DLLs. hero-display.rs, built as an
#   example of helio-display, makes a 2880x1800 virtual display the whole
#   desktop as a stream does, until C:\pp\hero\done exists; check that it
#   says ACTIVE and that the screen is 2880x1800 before going on. Then
#   scale.ps1 for 200%, wall.ps1 for the wallpaper, and hero.ps1 hides the
#   taskbar and icons and takes the screen. Answer the firewall's question
#   about the new helio-app.exe first, and leave nothing else on the screen:
#   the picture is the screen. Then the wallpaper as it was, and the done file
#   puts the monitors back. Wallpaper: Windows 11's light Bloom (img0.jpg).
# - agent-screen.png (2560x1600), the screen the agent's window shows: a
#   Windows VM, hero/windows/agentshot.ps1, the desktop the sample session
#   talks about. The page does not show it by itself;
#   src/assets/hero/agent-screen.webp keeps it, for this and for
#   tools/capture-docs-images.
# - agent.png: as macos.png, with HELIO_UI_DEMO=agents=sample-ask,size=1120x740
#   and HELIO_UI_DEMO_SCREEN=agent-screen.png, so the window shows that screen.
# - devices.png: as macos.png, with
#   HELIO_UI_DEMO=hosting=on,host-sample,host=devices,size=1120x740 and empty
#   HELIO_DATA_DIR (onboarded = true) and HELIO_HOST_DATA_DIR folders: This
#   computer → Devices, a pairing request above the paired devices.
set -euo pipefail
SITE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SHOTS=${1:?usage: capture-hero.sh SHOTS_DIR}
for name in macos windows agent devices; do
    node -e 'require(process.argv[1])(process.argv[2]).webp({ quality: 88, effort: 6 }).toFile(process.argv[3])' \
        "$SITE/node_modules/sharp" "$SHOTS/$name.png" "$SITE/src/assets/hero/$name.webp"
    echo "$name"
done
node "$SITE/scripts/crop-windows.mjs"
