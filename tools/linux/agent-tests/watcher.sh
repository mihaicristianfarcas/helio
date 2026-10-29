#!/bin/bash
pkill -f "^Xvfb :2" 
(Xvfb :2 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &)
sleep 1
rm -f /src/target/linux-out/agent/watch*.png
export DISPLAY=:2 HELIO_DATA_DIR=/src/target/linux-out/agent/helio RUST_LOG=info,mainline=error
export HELIO_TEST_SNAPSHOT=/src/target/linux-out/agent/watch.png@90
export HELIO_TEST_INPUT="wait 6000; takeover; wait 1500; abs 300 250; click; text echo person typed this; key 1c; wait 3000; takeover; wait 9000; quit"
nohup /target/debug/helio watch linux-desk --windowed --size 1280x800 > /src/target/linux-out/agent/watch.log 2>&1 &
