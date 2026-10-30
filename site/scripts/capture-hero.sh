#!/usr/bin/env bash
# The hero's pictures, which the link preview (og-card.html) shows too: one
# Hosts window (HELIO_UI_DEMO's hosts-sample, 1120x740 points, dark) in the
# middle of a 2700x1800 screen on the desktop's default wallpaper, as that
# desktop composites it, so what shows through the glass is the real thing. A
# desktop has its own machine, so this script only turns what they made into
# src/assets/hero/*.webp:
#
#   site/scripts/capture-hero.sh SHOTS_DIR
#
# SHOTS_DIR holds macos.png, windows.png and agent.png, each 2700x1800, and
# agent-screen.png. Each comes from a demo build of this checkout:
#
# - macOS (this Mac): hero/macos/wall.swift shows the wallpaper as a window
#   behind Helio, System Events places the window, screencapture -R takes the
#   screen there. Wallpaper: macOS Tahoe's light one.
# - Windows (a VM): hero/windows/. tools/set-resolution.ps1 then scale.ps1 for
#   2880x1800 at 200%, wall.ps1 and theme.ps1 -Light 0, then hero.ps1 hides the
#   taskbar and icons and takes the screen. Stop the guest agent (vdservice)
#   first, or it fits the display to its window. With no GPU, Windows cannot
#   draw acrylic: the backdrop falls back to a solid one, which is why Windows
#   itself is put in dark mode. Wallpaper: Windows 11's Bloom.
# - agent-screen.png (2560x1600), behind the agent's window on /agents: the same
#   VM, hero/windows/agentshot.ps1, the desktop the sample session talks about.
# - agent.png: as macos.png, with HELIO_UI_DEMO=agents=sample-ask,size=1120x740
#   and HELIO_UI_DEMO_SCREEN=agent-screen.png, so the window shows that screen.
set -euo pipefail
SITE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SHOTS=${1:?usage: capture-hero.sh SHOTS_DIR}
for name in macos windows agent agent-screen; do
    node -e 'require(process.argv[1])(process.argv[2]).webp({ quality: 88, effort: 6 }).toFile(process.argv[3])' \
        "$SITE/node_modules/sharp" "$SHOTS/$name.png" "$SITE/src/assets/hero/$name.webp"
    echo "$name"
done
