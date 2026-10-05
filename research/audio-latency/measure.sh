#!/bin/sh
# Measure sound latency from the Linux machine to this Mac, with MouseTail running on both
# and the Linux machine sending its sound here. Plays quiet noise and a chirp a second.
#   research/audio-latency/measure.sh [secs]
REMOTE=${MOUSETAIL_REMOTE:-omarchy}
SECS=${1:-30}
OUT=/tmp/audio-latency
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p spike-audio-latency
rsync -az --delete --exclude target --exclude .git ./ "$REMOTE":~/Workspace/MouseTail-latency/
ssh "$REMOTE" 'cd ~/Workspace/MouseTail-latency && ~/.cargo/bin/cargo build -q --release -p spike-audio-latency'
offset=$(python3 research/audio-latency/clock.py "$REMOTE" | cut -d' ' -f1)
echo "Linux clock is $((offset / 1000)) µs ahead"
rm -rf "$OUT"
# Core Audio only knows a process once it plays something, so start the sound first and
# listen once it's playing here (the tap then takes a few seconds to start delivering).
ssh "$REMOTE" "~/Workspace/MouseTail-latency/target/release/spike-audio-latency send $((SECS + 10))" \
  > "$OUT-send.txt" &
sleep 3
./target/debug/spike-audio-latency listen "$SECS" "$OUT" > "$OUT-listen.txt"
wait
python3 research/audio-latency/analyse.py "$OUT" "$OUT-send.txt" "$offset"
