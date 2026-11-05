#!/bin/bash
# Helio Host's API from inside the container (it answers this computer only),
# signed in with the token the host writes for its own computer: CMD PATH [JSON].
W=https://localhost:47822
T=$(cat "${HELIO_HOST_DATA_DIR:?run me through tools/linux/agent-desktop exec}/local-token")
case "$1" in
get) curl -sk -H "authorization: Bearer $T" "$W$2" ;;
post) curl -sk -H "authorization: Bearer $T" -H 'content-type: application/json' -d "${3:-{\}}" "$W$2" ;;
esac
