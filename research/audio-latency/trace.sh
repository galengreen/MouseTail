#!/bin/sh
# Pull the sound packet trace (`t seq stream`) out of a development build's log, as logged
# with RUST_LOG=...,mousetail::platform::macos_audio=trace.
#   research/audio-latency/trace.sh [log] > trace.txt
sed 's/\x1b\[[0-9;]*m//g' "${1:-/tmp/mousetail-mac.log}" | grep 'sound packet' | python3 -c '
import sys, datetime
for l in sys.stdin:
    w = l.split()
    t = datetime.datetime.fromisoformat(w[0].replace("Z", "+00:00")).timestamp()
    print(f"{t:.6f}", w[-2].split("=")[1], w[-1].split("=")[1])
'
