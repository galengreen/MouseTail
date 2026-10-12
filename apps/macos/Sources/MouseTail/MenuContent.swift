import AppKit
import SwiftUI

/// The menu bar popover: what's connected, where the cursor is, and the few things you might
/// want to change. Laid out like the system's own menu bar panels.
struct MenuContent: View {
    @EnvironmentObject var model: AppModel
    @ObservedObject private var updater = Updater.shared
    @Environment(\.openWindow) private var openWindow
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
                .padding(.horizontal, 6)
                .padding(.bottom, 10)
            if let status = model.status {
                VStack(alignment: .leading, spacing: 8) {
                    if !status.canControl {
                        PermissionNotice(detail: status.captureError)
                    }
                    if status.controlling != nil && status.keyboardBlocked == true {
                        Label("A password field on this Mac is blocking typing. The mouse still works.",
                              systemImage: "keyboard.badge.exclamationmark")
                            .font(.caption)
                            .foregroundStyle(.orange)
                    }
                    if let shown = status.pairingCode {
                        ShownCodePanel(shown: shown)
                    }
                }
                .padding(.bottom, 4)
                MenuDivider()
                SectionHeader("Computers")
                machines(status)
                if let pairing = model.pairing {
                    PairingPanel(pairing: pairing).padding(.top, 6)
                }
                ActionRow("Arrange Displays…", systemImage: "rectangle.3.group") {
                    openWindow(id: "arrange")
                    NSApp.bringToFront()
                }
                MenuDivider()
                SectionHeader("Settings")
                SettingRow("Sound follows you", systemImage: "speaker.wave.2", isOn: Binding(
                    get: { status.settings.audio ?? true },
                    set: { on in Task { await model.setAudio(on) } }
                ))
                SettingRow("Share clipboard", systemImage: "doc.on.clipboard", isOn: Binding(
                    get: { status.settings.clipboard },
                    set: { on in Task { await model.setClipboard(on) } }
                ))
                SettingRow("Ripple when crossing", systemImage: "water.waves", isOn: Binding(
                    get: { status.settings.ripple ?? true },
                    set: { on in Task { await model.setRipple(on) } }
                ))
                SettingRow("Open at login", systemImage: "power", isOn: Binding(
                    get: { model.openAtLogin },
                    set: { model.openAtLogin = $0 }
                ))
                if updater.available {
                    SettingRow("Update automatically", systemImage: "arrow.down.circle", isOn: Binding(
                        get: { updater.automatic },
                        set: { updater.automatic = $0 }
                    ))
                }
            } else {
                Text(model.problem ?? "Starting…")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 4)
            }
            MenuDivider()
            if updater.available {
                ActionRow("Check for Updates…") {
                    NSApp.bringToFront()
                    updater.checkForUpdates()
                }
            }
            ActionRow("Quit MouseTail", shortcut: "⌘Q") { NSApp.terminate(nil) }
                .keyboardShortcut("q")
        }
        .padding(10)
        .frame(width: 320)
        .noFocusRing()
    }

    private var header: some View {
        HStack(spacing: 10) {
            // The mouse-only logo on a dark menu; the white mouse is too faint on a light one,
            // so there it's the app icon.
            if colorScheme == .dark, let mouse = NSImage(named: "LogoMouse") {
                Image(nsImage: mouse)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 44, height: 34)
            } else {
                Image(nsImage: NSApp.applicationIconImage)
                    .resizable()
                    .frame(width: 34, height: 34)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("MouseTail").font(.system(size: 14, weight: .semibold))
                Text(summary).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String {
                Text(version).font(.caption2).foregroundStyle(.tertiary)
            }
        }
    }

    private var summary: String {
        guard let s = model.status else { return "" }
        if let peer = s.peerName(s.controlling) { return "Cursor on \(peer)" }
        if let peer = s.peerName(s.controlledBy) { return "Controlled by \(peer)" }
        let connected = s.peers.filter { $0.paired && $0.connected && $0.paused != true }.count
        switch connected {
        case 0: return "Not connected"
        case 1: return "Ready"
        default: return "Ready · \(connected) computers"
        }
    }

    @ViewBuilder
    private func machines(_ status: Status) -> some View {
        let visible = status.peers.filter { $0.paired || $0.connected }
        if visible.isEmpty {
            Label("Looking for other computers on your network…", systemImage: "magnifyingglass")
                .font(.callout)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 6)
                .padding(.vertical, 4)
        }
        ForEach(visible) { peer in
            PeerRow(peer: peer, active: status.controlling == peer.id || status.controlledBy == peer.id)
        }
    }
}

private struct MenuDivider: View {
    var body: some View {
        Divider().padding(.vertical, 6).padding(.horizontal, 6)
    }
}

private struct SectionHeader: View {
    let title: String
    init(_ title: String) { self.title = title }

    var body: some View {
        Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 6)
            .padding(.top, 2)
            .padding(.bottom, 4)
    }
}

/// A clickable row that highlights on hover, like a menu item.
private struct ActionRow: View {
    let title: String
    var systemImage: String?
    var shortcut: String?
    let action: () -> Void
    @State private var hovering = false

    init(_ title: String, systemImage: String? = nil, shortcut: String? = nil, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.shortcut = shortcut
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let systemImage {
                    RowIcon(systemImage: systemImage)
                }
                Text(title)
                Spacer()
                if let shortcut {
                    Text(shortcut).foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 5)
            .contentShape(Rectangle())
            .background(RoundedRectangle(cornerRadius: 6).fill(hovering ? Color.primary.opacity(0.1) : .clear))
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}

private struct SettingRow: View {
    let title: String
    let systemImage: String
    @Binding var isOn: Bool

    init(_ title: String, systemImage: String, isOn: Binding<Bool>) {
        self.title = title
        self.systemImage = systemImage
        self._isOn = isOn
    }

    var body: some View {
        HStack(spacing: 8) {
            RowIcon(systemImage: systemImage)
            Text(title).lineLimit(1)
            Spacer(minLength: 8)
            Toggle(title, isOn: $isOn)
                .labelsHidden()
                .toggleStyle(.switch)
                .controlSize(.mini)
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 4)
    }
}

/// A small symbol in a round tile, as in Control Center.
private struct RowIcon: View {
    let systemImage: String
    var tint: Color = .primary.opacity(0.1)
    var foreground: Color = .primary

    var body: some View {
        Image(systemName: systemImage)
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(foreground)
            .frame(width: 24, height: 24)
            .background(Circle().fill(tint))
    }
}

struct PeerRow: View {
    @EnvironmentObject var model: AppModel
    let peer: PeerStatus
    let active: Bool

    var body: some View {
        HStack(spacing: 8) {
            RowIcon(
                systemImage: peer.platform == "macos" ? "laptopcomputer" : "desktopcomputer",
                tint: active ? .accentColor : .primary.opacity(0.1),
                foreground: active ? .white : .primary
            )
            .overlay(alignment: .bottomTrailing) {
                Circle()
                    .fill(dotColour)
                    .frame(width: 8, height: 8)
                    .overlay(Circle().stroke(Color(nsColor: .windowBackgroundColor), lineWidth: 1.5))
                    .offset(x: 1, y: 1)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(peer.name).fontWeight(active ? .semibold : .medium)
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if !peer.paired && peer.connected {
                Button("Pair…") { Task { await model.startPairing(peer) } }
                    .controlSize(.small)
            } else if peer.paired {
                Button {
                    Task { await model.setPaused(peer, !paused) }
                } label: {
                    Image(systemName: paused ? "play.fill" : "pause.fill")
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                        .frame(width: 22, height: 22)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .help(paused ? "Resume \(peer.name)" : "Pause \(peer.name) without forgetting it")
                .focusable(false)
                Menu {
                    Button(paused ? "Resume \(peer.name)" : "Pause \(peer.name)") {
                        Task { await model.setPaused(peer, !paused) }
                    }
                    Divider()
                    Button("Forget \(peer.name)", role: .destructive) { Task { await model.unpair(peer) } }
                } label: {
                    Image(systemName: "ellipsis")
                        .foregroundStyle(.secondary)
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .focusable(false)
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 4)
    }

    private var paused: Bool { peer.paused == true }

    private var dotColour: Color {
        guard peer.connected else { return .secondary }
        if paused { return .yellow }
        return peer.paired ? .green : .orange
    }

    private var detail: String {
        if !peer.paired { return peer.connected ? "Found on your network" : "Not paired" }
        if paused { return peer.connected ? "Paused" : "Paused · Offline" }
        guard peer.connected else { return "Offline" }
        var parts = ["Connected"]
        if let ms = peer.rttMs { parts.append(String(format: "%.0f ms", ms)) }
        switch peer.sound {
        case "here": parts.append("its sound plays here")
        case "there": parts.append("plays your sound")
        default: break
        }
        return parts.joined(separator: " · ")
    }
}

struct PairingPanel: View {
    @EnvironmentObject var model: AppModel
    let pairing: Pairing
    @State private var code = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            switch pairing {
            case .awaitingCode(let peer):
                Text("Type the code shown on \(peer.name):").font(.callout)
                HStack {
                    TextField("0000", text: $code)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(.title3, design: .monospaced))
                        .frame(width: 90)
                        .onSubmit(submit)
                        .onChange(of: code) { new in
                            let digits = String(new.filter(\.isNumber).prefix(4))
                            // Tidying it up (a pasted "12-34") runs this again with the digits;
                            // submit then, not twice.
                            if digits != new {
                                code = digits
                            } else if digits.count == 4 {
                                submit()
                            }
                        }
                    Button("Cancel") { model.pairing = nil }
                }
            case .working(let peer):
                HStack { ProgressView().controlSize(.small); Text("Pairing with \(peer.name)…") }
            case .done(let message):
                Label(message, systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                    .task { try? await Task.sleep(for: .seconds(3)); model.pairing = nil }
            case .failed(let message):
                Label(message, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                Button("OK") { model.pairing = nil }
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(.quaternary))
    }

    private func submit() {
        guard case .awaitingCode(let peer) = pairing, code.count == 4 else { return }
        let entered = code
        Task { await model.submitCode(entered, for: peer) }
    }
}

/// Another computer asked to pair: show the code big, to type over there.
struct ShownCodePanel: View {
    let shown: ShownCode

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Pairing code").font(.caption).foregroundStyle(.secondary)
            Text(shown.code.map { String($0) }.joined(separator: " "))
                .font(.system(size: 30, weight: .semibold, design: .monospaced))
            Text("Type this on \(shown.name ?? "the other computer") to connect it.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(.quaternary))
    }
}

struct PermissionNotice: View {
    let detail: String?

    /// The daemon got past the permission check but still couldn't start capturing.
    private var permissionGranted: Bool {
        guard let detail else { return false }
        return !detail.contains("needs Accessibility and Input Monitoring")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Allow MouseTail to use your keyboard and mouse", systemImage: "hand.raised.fill")
                .font(.callout.weight(.semibold))
            Text("Turn on MouseTail in Accessibility and Input Monitoring. It starts working as soon as you do.")
                .font(.caption)
                .foregroundStyle(.secondary)
            if permissionGranted {
                // macOS says yes, but still won't let it watch the keyboard and mouse: usually a
                // permission left over from the version before an update.
                Text("Already on? Turn MouseTail off and on again in both lists.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            HStack {
                Button("Accessibility…") { open("Privacy_Accessibility") }
                Button("Input Monitoring…") { open("Privacy_ListenEvent") }
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color.orange.opacity(0.12)))
    }

    private func open(_ pane: String) {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
            NSWorkspace.shared.open(url)
        }
    }
}

extension NSApplication {
    func bringToFront() {
        if #available(macOS 14, *) { activate() } else { activate(ignoringOtherApps: true) }
    }
}

private extension View {
    // Otherwise the first control gets a focus ring each time the menu opens. Ventura has
    // no way to turn it off, so there it shows.
    @ViewBuilder func noFocusRing() -> some View {
        if #available(macOS 14, *) { focusEffectDisabled() } else { self }
    }
}
