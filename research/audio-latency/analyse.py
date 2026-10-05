#!/usr/bin/env python3
"""Find the chirps in a `listen` capture and work out the latency.

  analyse.py <listen dir> <send output> <linux clock ahead of mac, ns>

Everything is put on the Mac's host clock. The tap gives when the daemon handed each sample
to Core Audio; our own chirps (rendered at a known time) tell us how the tap's timestamps
relate to rendering, and cpal tells us how long from rendering to the output (for AirPods,
mostly Bluetooth)."""
import sys
import numpy as np

d, send, offset = sys.argv[1], sys.argv[2], int(sys.argv[3])
meta = {}
for line in open(f"{d}/meta.txt"):
    w = line.split()
    meta.update(zip(w[::2], w[1::2]))
rate = float(meta["tap_rate"])
wall_minus_host = int(meta["wall_minus_host_ns"])

x = np.fromfile(f"{d}/capture.f32", dtype=np.float32).reshape(-1, 2).mean(axis=1)
chunks = np.loadtxt(f"{d}/chunks.txt", dtype=np.int64, ndmin=2)


def host_at(frame):
    i = np.searchsorted(chunks[:, 1], frame, side="right") - 1
    return chunks[i, 0] + (frame - chunks[i, 1]) * 1e9 / rate


def chirp(rising):
    n = int(0.02 * rate)
    t = np.arange(n) / rate
    f0, f1 = (1500, 7000) if rising else (7000, 1500)
    return np.sin(2 * np.pi * (f0 * t + (f1 - f0) * t * t / 0.04)) * np.hanning(n)


def correlate(rising):
    c = chirp(rising)
    m = len(x) + len(c)
    return np.abs(np.fft.irfft(np.fft.rfft(x, m) * np.conj(np.fft.rfft(c, m)), m)[: len(x)])


frame_hosts = host_at(np.arange(len(x)))


def find(corr, starts, window_ns):
    """For each start (host ns), the host time of the strongest chirp within the window
    after it, if it clearly stands out. Other sound playing can hide a few."""
    floor = np.median(corr)
    found = []
    for s in starts:
        lo, hi = np.searchsorted(frame_hosts, [s, s + window_ns])
        if hi - lo < rate * window_ns / 2e9:
            continue  # not captured
        j = lo + int(np.argmax(corr[lo:hi]))
        if corr[j] > 20 * floor:
            found.append((s, frame_hosts[j]))
    return found


# The tap against our own rendering: how late the tap stamps what a process renders.
renders = np.loadtxt(f"{d}/local.txt", dtype=np.int64, ndmin=2)
local = find(correlate(False), renders[:, 1], 0.1e9)
lags = [(h - r) / 1e6 for r, h in local]
lag = float(np.median(lags))
render_to_out = float(np.median(renders[:, 2])) / 1e6

sent = [l.split() for l in open(send) if l.startswith("chirp")]
emitted = np.array([int(w[2]) for w in sent]) - offset - wall_minus_host  # Mac host ns
remote = find(correlate(True), emitted, 0.99e9)
to_tap = np.array([(h - e) / 1e6 for e, h in remote])
first_found = len(remote) and remote[0][0] == emitted[0]
to_render = to_tap - lag
total = to_render + render_to_out


def show(name, v):
    print(f"{name:<44} median {np.median(v):6.1f}  min {np.min(v):6.1f}  max {np.max(v):6.1f} ms")


print(f"{len(remote)} of {len(sent)} sent chirps found, {len(local)} local")
show("tap stamp minus render (local calibration)", lags)
if first_found:
    print(f"{'first chirp (sound starting)':<44} {to_render[0]:6.1f} + {render_to_out:.1f} output ms")
    to_render, total = to_render[1:], total[1:]
show("Linux graph -> Mac daemon renders", to_render)
print(f"{'  p10 / p50 / p90':<44} " + " / ".join(f"{v:.1f}" for v in np.percentile(to_render, [10, 50, 90])))
print(f"{'render -> output (cpal, this device)':<44} {render_to_out:6.1f} ms")
show("Linux graph -> Mac output", total)

# Dropouts: the sender's noise floor going quiet for 5 ms or more while it was sending.
if len(remote):
    lo, hi = np.searchsorted(frame_hosts, [remote[0][1], remote[-1][1]])
    block = int(rate * 0.0025)
    seg = x[lo : hi - (hi - lo) % block].reshape(-1, block)
    rms = np.sqrt((seg**2).mean(axis=1))
    # Other sound may be playing too, so measure against the noise floor, not the typical level.
    floor = np.percentile(rms[rms > 1e-5], 5)
    quiet = rms < 0.25 * floor
    runs, n = [], 0
    for q in quiet:
        if q:
            n += 1
        elif n:
            runs.append(n)
            n = 0
    gaps = [r * 2.5 for r in runs if r >= 2]
    secs = (hi - lo) / rate
    print(f"{'dropouts':<44} {len(gaps)} in {secs:.0f} s, {sum(gaps):.0f} ms silent in all")
