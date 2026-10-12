## What's new

- 0.2.12: MouseTail now works on GNOME (thanks to Ming Bao). It shares this computer's mouse
  and keyboard through GNOME's input capture, which asks your permission the first time, and
  adds a MouseTail menu to the top bar on GNOME 50. If you stop the sharing from GNOME,
  MouseTail says so in its status; restart it to share again.

From 0.2.11:

- 0.2.11: MouseTail now runs on macOS 13 Ventura and later (it needed 14.2 before).
  Everything works there except sending the Mac's own sound to another computer, which
  still needs macOS 14.2.

From 0.2.10:

- 0.2.10: Sharing sound uses less CPU on Linux. Opus encoding does less work and reuses
  its scratch buffer, and MouseTail avoids copying each audio frame into a new allocation.
  Linux now asks PipeWire for 10 ms callbacks, matching its audio packets. This adds about
  5 ms of capture latency. The audio format, bitrate and packet-loss protection stay the same,
  so this release can share sound with existing copies.

From 0.2.9:

- 0.2.9: Shared sound arrives sooner. MouseTail now sizes its buffer from how your network
  is actually behaving, so after a choppy patch it shrinks back within a minute instead of
  staying large for the rest of the day, and it sends and plays sound in smaller pieces. On
  a steady network sound from the other computer now arrives in well under a tenth of a
  second (Bluetooth headphones add their own delay on top). Dropouts are rarer too.

From 0.2.8:

- Sound shared from another computer glitches less. MouseTail now keeps its small
  buffer steady as the two computers' clocks drift apart, gives itself more room when the
  network gets choppy (then shrinks back), and fades over any gap instead of clicking. Linux
  also sends its sound in smaller, steadier pieces. If you still hear dropouts, a cable
  helps more than anything: Wi-Fi, especially 2.4 GHz, can stall for a tenth of a second.

From 0.2.7:

- **Fix Firewall…** on Linux now works on computers that updated themselves to 0.2.6
  (the script it runs hadn't been installed there).

From 0.2.6:

- Paired computers find each other again by themselves. Each remembers where it last reached
  the other and keeps trying there, and looks again after sleep or a network change, so
  they no longer stay apart for up to an hour after a laptop wakes.
- Linux: if the firewall stops other computers reaching MouseTail (Omarchy and Fedora turn
  one on), the Omarchy panel says so and **Fix Firewall…** opens MouseTail's port to your
  local network (it asks for your password). Or run
  `~/.local/share/mousetail/enable-firewall.sh`. New installs offer it straight away.
- A Mac on Wi-Fi and Ethernet at once no longer confuses other computers about its name and
  address.
- The crossing ripple shows on every display again after a Mac wakes from sleep, rather than
  going missing on an external monitor.

Also since 0.2.3: the Mac download opens in a proper window (drag MouseTail along the trail
into Applications), and a new website.

If you're coming from 0.2.2 or earlier, 0.2.3 brought:

- A ripple spreads from the edge wherever the cursor crosses to or from another computer, so
  you can see where it went. Turn it off with **Ripple when crossing** in the menu (Mac) or
  the Omarchy panel, or `mousetail set ripple off`.
- Pause a paired computer without forgetting it: the pause button beside it in the menu (or
  the Omarchy panel, or `mousetail pause <computer>`). Nothing crosses until you resume it,
  from either computer.
- Scrolling with a Mac's trackpad on a Linux computer feels like its own trackpad: its scroll
  speed and its apps' momentum apply, rather than the Mac's.
- Only one cursor on show: the computer you've just left hides its own until you use its
  mouse again, rather than leaving it sitting at the edge.
- Sound goes both ways: push from Linux onto a Mac and the Mac's sound comes to Linux too
  (it needs the System Audio Recording permission). Headphone buttons, media keys and
  the desktop's play controls play, pause and skip whatever the other computer is playing.
- The Omarchy panel can now pair, arrange (drag computers to where they sit, like Arrange
  Displays on a Mac), change settings and check for updates.
- MouseTail keeps itself up to date. If you're on 0.2.0 or earlier, install this release by
  hand once on each computer; 0.2.1 and later update by themselves.

## Install

**Mac** (macOS 13 Ventura or later, Apple Silicon or Intel): download `MouseTail-macos.dmg`, drag MouseTail to
Applications and open it. This build isn't notarised yet, so the first time macOS will refuse
to open it: go to **System Settings → Privacy & Security** and click **Open Anyway**.

**Linux** (Wayland):

```sh
curl -fsSL https://mousetail.galen.green/install.sh | sh
```

or download `mousetail-linux-<arch>.tar.gz`, extract it and run `./install.sh`.

Then pair: click **Pair…** next to the other computer in MouseTail's menu on a Mac, or run
`mousetail pair` on Linux, and type the code the other computer shows.
