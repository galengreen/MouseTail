# Spike results — 2026-09-29

Feasibility checks run before building. Mac: MacBook Pro M1 Pro, macOS 27.0.
Target: Omarchy 4.0.0.alpha iMac, Hyprland 0.56.2, kernel 7.2.

Run Linux spikes with `dev/sync.sh && ssh $MOUSETAIL_REMOTE '~/Workspace/MouseTail/dev/linux-tests.sh all'`.

| # | Question | Result |
|---|----------|--------|
| 1 | Absolute pointer on Hyprland without root? | **Pass.** `zwlr_virtual_pointer_v1.motion_absolute` lands on the exact pixel (5/5 points incl. corners). Scroll via `axis` accepted. |
| 2 | Keyboard on Hyprland without root? | **Pass.** `zwp_virtual_keyboard_v1` using the seat's own keymap typed `MouseTail ok 123` (with Shift) into a foot window exactly. |
| 3 | Injection cost | 1000 absolute moves in 22 ms (~22 µs each), no socket stalls when flushing per event. Bursts *can* hit `EWOULDBLOCK`; the sender must wait for writability, not panic or drop. |
| 4 | Clipboard from a windowless daemon | **Pass** both ways via data-control (`wl-clipboard-rs`), cross-checked with `wl-copy`/`wl-paste`. |
| 5 | Does injected input count as activity for Omarchy idle (screensaver 150 s, lock 300 s)? | **Pass.** A nudge every 30 s kept the screensaver off for 3.5 min; after the last nudge it started exactly 150 s later. Pointer motion does **not** dismiss the screensaver (Omarchy's screensaver only exits on a key press or losing focus — same with a real mouse). |
| 6 | mDNS discovery across the LAN | **Pass** both directions; resolved in 40–500 ms. ufw's default mDNS rule lets it through. |
| 7 | TCP Mac → iMac | **Blocked.** ufw on the iMac denies inbound by default (only Lan Mouse UDP 4242 from the Mac, SSH and LocalSend are open). |
| 8 | TCP iMac → Mac | **Pass**, through the macOS application firewall (enabled). |
| 9 | Network latency | Wi-Fi ↔ Wi-Fi (mesh): ping avg 5.4 ms, p99 ~12 ms, one 3 s stall seen over 1000 TCP round trips. Mac wired (`en11`) ↔ iMac Wi-Fi: avg 2.6 ms. The Mac routes via Wi-Fi by default (service order). |
| 10 | Mac permissions for event tap | Granted to the current host app (Accessibility, Input Monitoring, post events; active tap created). The shipping app will need its own grant. |
| 11 | Mac display geometry | Built-in 1512×982 pt @2x at (0,0), main. Dell 2560×1440 pt @2x at (−587, −1440) — i.e. **above** the MacBook. |
| 12 | Input while locked (Omarchy Quickshell lock, `ext-session-lock`, secure) | **Pass.** Absolute pointer 5/5 points; typed characters appear in the password field; Backspace clears it. No password was submitted (`authenticating: false`). |

## Consequences for the design

- **No uinput, no udev rule, no root on Linux.** Wayland virtual pointer + keyboard cover it
  on Hyprland (and other wlroots compositors). uinput stays as a fallback backend only.
- **Status UI on Omarchy is a shell plugin**, not Waybar: Omarchy 4 replaced Waybar and
  hyprlock with its Quickshell shell, which loads user plugins from
  `~/.config/omarchy/plugins/<id>/` (`bar-widget` kind).
- **Hyprland 0.56 dispatch is Lua** (`hyprctl dispatch 'hl.dsp.exec_cmd([[…]])'`); anything
  shelling out to `hyprctl dispatch` must use the new syntax.
- **Connection direction is independent of role.** Both sides advertise and both dial; the
  first authenticated connection wins. In practice the iMac dials the Mac, so the Omarchy
  install needs no firewall change and no sudo.
- **Transport: QUIC** instead of plain TCP. Pointer motion goes in unreliable datagrams
  (absolute positions, latest wins, so loss self-heals and a stall can't queue up stale
  motion); keys, buttons, clipboard and control go on reliable streams. TLS 1.3 with pinned
  self-signed certificates replaces a hand-rolled Noise layer.
- **Address selection:** probe every advertised address and use the lowest RTT (prefers the
  wired link here).
- **Screensaver:** on entering the iMac, dismiss Omarchy's screensaver the way Omarchy does
  (SIGTERM its script, which restores the cursor) rather than faking a key press.
- **Lock screen:** fully usable from the Mac's keyboard, and since the Mac owns the return,
  it can never trap the cursor.

## Follow-up: wired path (2026-09-29)

`spike-path-probe` (QUIC handshake to one address at a time, from the iMac):

| Mac address | Single wildcard socket | One socket per address |
|---|---|---|
| Wi-Fi | connected, 39 ms | connected, 26 ms |
| Ethernet | **timed out**: reply came from .64, ufw dropped it | connected, **7 ms** |

## Follow-up: media controls on the Mac (2026-09-30)

`research/mac-nowplaying` drives the daemon's `macos_media.rs`: claims Now Playing, then
paused, then lets go, logging commands (media keys synthesised with `NSEvent` system-defined
events, which travel the same MediaRemote path as AirPods).

- **A bare binary can be Now Playing.** No app bundle or window needed; play/pause, previous
  and paused-state presses all arrived.
- **Commands arrive on the main thread only.** Registering from another thread running its own
  run loop received nothing, so the daemon's main thread has to run the run loop.
- **Actually playing sound matters.** While a browser app was really playing, a process that
  only *claimed* to be playing didn't get the presses. The daemon plays the other computer's
  sound itself, so it competes like any music app; the spike doesn't.

## Follow-up: sound both ways (2026-09-30)

- **Linux playback** (iMac): the PipeWire player stream appears when sound arrives and is gone
  about 3 s after it stops. When the default output is one of MouseTail's own speakers it plays
  to a real output instead (checked: linked to `imac_speakers`, not the MouseTail sink), so it
  can't feed back.
- **Linux media controls**: the MPRIS player takes its bus name on show and drops it on clear,
  and PlayPause/Play/Pause/Stop/Next/Previous all arrive. Omarchy 4's media keys run
  `omarchy-shell media …` (its shell's MPRIS support), not `playerctl`, which isn't installed.
- **Mac sending** (`research/mac-tap`): built but not yet run; it needs the System Audio
  Recording permission and mutes the Mac while it taps.
- **macOS 14.2 minimum**: cpal already links `AudioHardwareDestroyProcessTap` directly (0.2.2's
  daemon included), so the daemon can't load on 14.0/14.1; the app now says 14.2.
  Later: the daemon weak-links Core Audio (`crates/mousetail/build.rs`), so it loads on
  Ventura (13) and only sending the Mac's sound needs 14.2.
