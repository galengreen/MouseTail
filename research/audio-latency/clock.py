#!/usr/bin/env python3
"""How far the Linux machine's wall clock is ahead of this Mac's, from the quickest of many
SSH round trips. Prints `offset_ns rtt_ns`."""
import subprocess, sys, time

host = sys.argv[1] if len(sys.argv) > 1 else "omarchy"
echo = "import sys,time;[print(time.time_ns(),flush=True) for _ in sys.stdin]"
p = subprocess.Popen(["ssh", host, "python3", "-u", "-c", repr(echo)],
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
best = None
for _ in range(300):
    t0 = time.time_ns()
    p.stdin.write("\n"); p.stdin.flush()
    remote = int(p.stdout.readline())
    t1 = time.time_ns()
    if best is None or t1 - t0 < best[1]:
        best = (remote - (t0 + t1) // 2, t1 - t0)
p.stdin.close(); p.wait()
print(*best)
