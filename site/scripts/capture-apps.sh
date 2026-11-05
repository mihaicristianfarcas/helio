#!/usr/bin/env bash
# The page's screenshots of Helio, from the app's demo mode (HELIO_UI_DEMO,
# docs/ui.md) in its dark appearance, as WebP in src/assets/shots. macOS; the
# terminal needs Screen Recording, and Accessibility to place the window. The
# hero's pictures are capture-hero.sh's.
#
#   cargo build -p helio-app
#   site/scripts/capture-apps.sh
#
# HELIO_APP names the program, for a build elsewhere than this checkout.
#
# Each window is 1200x800 points, which the page shows at about its own size:
# at the app's default 940x620, shrunk to fit the page, the pictures look
# crowded. Each is the screen where the window is, over a night blue
# (scripts/hero/macos/wall.swift): the demo's snapshot step is the window with
# nothing behind its glass, and dark pages come out lighter that way. The
# corners are then cut, so the page shows through them.
#
# The data folders are empty and temporary, marked as past setup, so nothing
# of this computer's hosts or settings reaches a picture. The window names no
# version under HELIO_UI_DEMO: the page's badge is the one place that says it.
set -euo pipefail
SITE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
REPO=$(cd "$SITE/.." && pwd)
APP=${HELIO_APP:-$REPO/target/debug/helio-app}
SHOTS=$SITE/src/assets/shots
TMP=$(mktemp -d)
WALL=
trap '[[ -n $WALL ]] && kill "$WALL" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir -p "$SHOTS"

# Where the window goes, in points from the screen's top left, and the night
# around it.
X=156 Y=91 W=1200 H=800
SHARP=$SITE/node_modules/sharp
swiftc -O "$SITE/scripts/hero/macos/wall.swift" -o "$TMP/wall"
node -e 'require(process.argv[1])({ create: { width: 64, height: 64, channels: 3, background: "#111a2e" } }).png().toFile(process.argv[2])' \
    "$SHARP" "$TMP/night.png"

shot() { # name steps
    local data out=$TMP/$1.png app
    # Afresh for each shot: when an app quits, whatever was in front before
    # it (the terminal) comes back above this, and would show in the glass.
    "$TMP/wall" "$TMP/night.png" $((X - 50)) $((Y - 30)) $((W + 100)) $((H + 60)) 30 &
    WALL=$!
    sleep 1.5
    data=$(mktemp -d "$TMP/data.XXXX")
    echo 'onboarded = true' >"$data/settings.toml"
    # A home of its own: Agent setup lists the agents it finds and where, and
    # this computer's paths stay out of the picture.
    mkdir -p "$data/.local/bin"
    for agent in codex claude; do
        printf '#!/bin/sh\n' >"$data/.local/bin/$agent"
        chmod +x "$data/.local/bin/$agent"
    done
    HOME=$data PATH=$data/.local/bin:/usr/bin:/bin \
        HELIO_DATA_DIR=$data HELIO_HOST_DATA_DIR=$data/host HELIO_APPEARANCE=dark \
        HELIO_UI_DEMO="$2,size=${W}x$H" "$APP" >/dev/null 2>&1 &
    app=$!
    sleep 4
    osascript -e "tell application \"System Events\" to tell (first process whose unix id is $app) to set position of window 1 to {$X, $Y}" >/dev/null
    sleep 1.5
    screencapture -x -R"$X,$Y,$W,$H" "$out"
    kill "$app" "$WALL" 2>/dev/null
    wait "$app" "$WALL" 2>/dev/null || true
    WALL=
    node -e '
        const [sharp, from, to] = process.argv.slice(1);
        const s = require(sharp);
        (async () => {
            const { width: w, height: h } = await s(from).metadata();
            const mask = Buffer.from(`<svg width="${w}" height="${h}"><rect width="${w}" height="${h}" rx="44"/></svg>`);
            await s(from).ensureAlpha().composite([{ input: mask, blend: "dest-in" }])
                .webp({ quality: 92, alphaQuality: 100, effort: 6 }).toFile(to);
        })();' "$SHARP" "$out" "$SHOTS/$1.webp"
    echo "$1"
}

shot devices "hosting=on,host-sample,host=devices"
shot video "settings=video"
shot input "settings=input"
shot hosting "hosting=on,host-sample,settings=hosting"
shot agent-setup "hosting=on,host-sample,settings=agents"
