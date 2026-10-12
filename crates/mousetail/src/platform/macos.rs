//! macOS backend: a Quartz event tap feeds the `Controller`; while the cursor is on another
//! machine the local cursor is hidden and frozen and input is swallowed.
//!
//! The tap callback runs the controller synchronously so it can decide to swallow each event
//! inline. Network actions go out through a channel; grab/release happen immediately.

use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_foundation::string::CFString;
use core_graphics::display::CGDisplay;
use mousetail_core::controller::{Action, Controller, Input};
use mousetail_core::keys::{ev, mac_to_evdev};
use mousetail_core::layout::{Point, Rect};
use mousetail_core::proto::{DisplayInfo, Scroll};
use tokio::sync::mpsc::UnboundedSender;

// ---------------------------------------------------------------------------------------------
// Quartz FFI

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

type CGEventRef = *mut c_void;
type TapCallback = extern "C" fn(*mut c_void, u32, CGEventRef, *mut c_void) -> CGEventRef;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: TapCallback,
        info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetDoubleValueField(event: CGEventRef, field: u32) -> f64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGAssociateMouseAndMouseCursorPosition(connected: bool) -> i32;
    fn CGWarpMouseCursorPosition(point: CGPoint) -> i32;
    fn CGDisplayHideCursor(display: u32) -> i32;
    fn CGDisplayShowCursor(display: u32) -> i32;
    fn CGCursorIsVisible() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGGetOnlineDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayMirrorsDisplay(display: u32) -> u32;
    fn CGEventSourceCreate(state: i32) -> *mut c_void;
    fn CGEventSourceSetLocalEventsSuppressionInterval(source: *mut c_void, seconds: f64);
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopAddSource(rl: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopGetCurrent() -> *mut c_void;
    // Private but long-standing (used by Synergy, Barrier, Deskflow): lets a background
    // process hide the cursor.
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(
        cid: i32,
        target: i32,
        key: *const c_void,
        value: *const c_void,
    ) -> i32;
}

// CGEventType
const LEFT_DOWN: u32 = 1;
const LEFT_UP: u32 = 2;
const RIGHT_DOWN: u32 = 3;
const RIGHT_UP: u32 = 4;
const MOUSE_MOVED: u32 = 5;
const LEFT_DRAGGED: u32 = 6;
const RIGHT_DRAGGED: u32 = 7;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const FLAGS_CHANGED: u32 = 12;
const SCROLL_WHEEL: u32 = 22;
const OTHER_DOWN: u32 = 25;
const OTHER_UP: u32 = 26;
const OTHER_DRAGGED: u32 = 27;
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
/// Trackpad gestures (rotate, begin/end, gesture, magnify, swipe, smart magnify): swallowed
/// while remote so pinches and swipes don't act on the Mac.
const GESTURES: [u32; 7] = [18, 19, 20, 29, 30, 31, 32];

// CGEventField
const MOUSE_BUTTON_NUMBER: u32 = 3;
const MOUSE_DELTA_X: u32 = 4;
const MOUSE_DELTA_Y: u32 = 5;
const KEY_AUTOREPEAT: u32 = 8;
const EVENT_SOURCE_USER_DATA: u32 = 42;
const KEY_KEYCODE: u32 = 9;
const SCROLL_DELTA_AXIS_1: u32 = 11;
const SCROLL_DELTA_AXIS_2: u32 = 12;
const SCROLL_IS_CONTINUOUS: u32 = 88;
const SCROLL_POINT_DELTA_AXIS_1: u32 = 96;
const SCROLL_POINT_DELTA_AXIS_2: u32 = 97;
const SCROLL_PHASE: u32 = 99;
const SCROLL_MOMENTUM_PHASE: u32 = 123;

// CGScrollPhase
const SCROLL_PHASE_ENDED: i64 = 4;
const SCROLL_PHASE_CANCELLED: i64 = 8;
const SCROLL_PHASE_MAY_BEGIN: i64 = 128;

const SESSION_TAP: u32 = 1;
const HEAD_INSERT: u32 = 0;
const TAP_DEFAULT: u32 = 0;
const COMBINED_SESSION_STATE: i32 = 0;

// ---------------------------------------------------------------------------------------------
// Capture

struct Shared {
    controller: Arc<Mutex<Controller>>,
    actions: UnboundedSender<Action>,
    grabbed: AtomicBool,
    tap: AtomicPtr<c_void>,
    /// Hides we owe a show for: macOS sometimes shows the cursor again behind our back, so a
    /// grab can take several hides.
    hides: AtomicU32,
    /// When the cursor was last checked while grabbed (ms since the Unix epoch).
    checked: AtomicU64,
    /// When the cursor was hidden because another computer's cursor left us (ms since the
    /// Unix epoch; 0 = not hidden for that). It shows again once this Mac's own mouse moves.
    parked: AtomicU64,
}

/// Injected input this soon after parking is our own move onto the edge, not a controller
/// coming back.
const PARK_SETTLE_MS: u64 = 250;

static SHARED: OnceLock<Shared> = OnceLock::new();

pub struct Capture;

impl Capture {
    /// Start capturing. `prompt` asks macOS to show its permission dialog if needed (do that
    /// once; later retries just check quietly).
    pub fn start(
        controller: Arc<Mutex<Controller>>,
        actions: UnboundedSender<Action>,
        prompt: bool,
    ) -> anyhow::Result<Self> {
        ensure_permissions(prompt)?;
        // Retries after a failed start pass the same controller and channel, so the first
        // call's are kept. Only a tap that's actually running means we've already started.
        let shared = SHARED.get_or_init(|| Shared {
            controller,
            actions,
            grabbed: AtomicBool::new(false),
            tap: AtomicPtr::new(ptr::null_mut()),
            hides: AtomicU32::new(0),
            checked: AtomicU64::new(0),
            parked: AtomicU64::new(0),
        });
        anyhow::ensure!(
            shared.tap.load(Ordering::SeqCst).is_null(),
            "capture already started"
        );

        let (ready_tx, ready_rx) = mpsc::channel();
        thread::Builder::new()
            .name("event-tap".into())
            .spawn(move || run_tap(ready_tx))?;
        ready_rx.recv().context("event tap thread died")??;

        unsafe {
            // Let our warp take effect immediately rather than ignoring input for 250 ms.
            let source = CGEventSourceCreate(COMBINED_SESSION_STATE);
            if !source.is_null() {
                CGEventSourceSetLocalEventsSuppressionInterval(source, 0.0);
            }
        }
        allow_background_cursor();
        Ok(Self)
    }

    pub fn supported() -> bool {
        true
    }

    /// Another computer's cursor just left us over an edge: hide ours, so only theirs is on
    /// show, until this Mac's own mouse moves (or a controller comes back).
    pub fn hide_cursor(&self) {
        let Some(shared) = SHARED.get() else { return };
        if !shared.grabbed.load(Ordering::SeqCst)
            && shared.parked.swap(now_ms().max(1), Ordering::SeqCst) == 0
        {
            hide_cursor(shared);
        }
    }

    /// The event tap sees the whole screen, so it doesn't need to be told about edges.
    pub fn set_edges(&self, _edges: Vec<super::Edge>) {}

    /// Carry out a grab or release decided outside the tap (e.g. peer disconnected).
    pub fn apply(&self, action: &Action) {
        apply(action);
    }

    /// The tap carries on until MouseTail quits.
    pub fn stopped(&self) -> Option<String> {
        None
    }
}

fn ensure_permissions(prompt: bool) -> anyhow::Result<()> {
    let options = CFDictionary::from_CFType_pairs(&[(
        CFString::from_static_string("AXTrustedCheckOptionPrompt").as_CFType(),
        CFBoolean::from(prompt).as_CFType(),
    )]);
    let accessibility =
        unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void) };
    let monitoring =
        unsafe { CGPreflightListenEventAccess() || (prompt && CGRequestListenEventAccess()) };
    // The Mac app tells this apart from other capture errors by its wording (PermissionNotice).
    anyhow::ensure!(
        accessibility && monitoring,
        "MouseTail needs Accessibility and Input Monitoring permission \
         (System Settings → Privacy & Security), then restart it"
    );
    Ok(())
}

fn run_tap(ready: mpsc::Sender<anyhow::Result<()>>) {
    let mut mask: u64 = 0;
    for t in [
        LEFT_DOWN,
        LEFT_UP,
        RIGHT_DOWN,
        RIGHT_UP,
        MOUSE_MOVED,
        LEFT_DRAGGED,
        RIGHT_DRAGGED,
        KEY_DOWN,
        KEY_UP,
        FLAGS_CHANGED,
        SCROLL_WHEEL,
        OTHER_DOWN,
        OTHER_UP,
        OTHER_DRAGGED,
    ]
    .into_iter()
    .chain(GESTURES)
    {
        mask |= 1 << t;
    }
    unsafe {
        let tap = CGEventTapCreate(
            SESSION_TAP,
            HEAD_INSERT,
            TAP_DEFAULT,
            mask,
            tap_callback,
            ptr::null_mut(),
        );
        if tap.is_null() {
            let _ = ready.send(Err(anyhow::anyhow!(
                "couldn't create the event tap (permission missing?)"
            )));
            return;
        }
        SHARED.get().unwrap().tap.store(tap, Ordering::SeqCst);
        let source = CFMachPortCreateRunLoopSource(ptr::null(), tap, 0);
        CFRunLoopAddSource(
            CFRunLoopGetCurrent(),
            source,
            kCFRunLoopCommonModes as *const c_void,
        );
        CGEventTapEnable(tap, true);
    }
    let _ = ready.send(Ok(()));
    CFRunLoop::run_current();
}

extern "C" fn tap_callback(
    _proxy: *mut c_void,
    etype: u32,
    event: CGEventRef,
    _info: *mut c_void,
) -> CGEventRef {
    let Some(shared) = SHARED.get() else {
        return event;
    };
    if etype == TAP_DISABLED_BY_TIMEOUT || etype == TAP_DISABLED_BY_USER_INPUT {
        // macOS turns slow taps off; turn it straight back on.
        unsafe { CGEventTapEnable(shared.tap.load(Ordering::SeqCst), true) };
        return event;
    }
    // Input we injected ourselves (another computer controlling this Mac) isn't the local
    // user: never let it cross edges or be forwarded.
    let injected = unsafe { CGEventGetIntegerValueField(event, EVENT_SOURCE_USER_DATA) }
        == super::macos_emulate::MOUSETAIL_EVENT;
    // Hidden since another computer's cursor left: show it for whoever's moving it now.
    let parked = shared.parked.load(Ordering::SeqCst);
    if parked != 0
        && etype != KEY_DOWN
        && etype != KEY_UP
        && etype != FLAGS_CHANGED
        && (!injected || now_ms().saturating_sub(parked) > PARK_SETTLE_MS)
        && shared.parked.swap(0, Ordering::SeqCst) != 0
    {
        show_cursor(shared);
    }
    if injected {
        return event;
    }
    let grabbed = shared.grabbed.load(Ordering::SeqCst);
    if grabbed {
        keep_cursor_hidden(shared);
    }
    let inputs = unsafe { to_inputs(etype, event, grabbed) };
    if etype != MOUSE_MOVED {
        tracing::trace!("tap event {etype} grabbed={grabbed} -> {inputs:?}");
    }
    if inputs.is_empty() {
        let swallow = grabbed && etype != MOUSE_MOVED;
        return if swallow { ptr::null_mut() } else { event };
    }
    let mut swallow = false;
    // A poisoned lock means a panic elsewhere; keep going rather than wedge the user's input.
    let mut controller = shared
        .controller
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for input in inputs {
        let outcome = controller.handle(input);
        swallow |= outcome.swallow;
        for action in outcome.actions {
            match action {
                Action::Grab | Action::Release { .. } => apply(&action),
                other => {
                    let _ = shared.actions.send(other);
                }
            }
        }
    }
    if swallow { ptr::null_mut() } else { event }
}

/// Translate a Quartz event into controller input. Events that should just be swallowed
/// while remote (gestures, key repeats, unmapped keys) produce nothing.
unsafe fn to_inputs(etype: u32, event: CGEventRef, grabbed: bool) -> Vec<Input> {
    unsafe {
        match etype {
            MOUSE_MOVED | LEFT_DRAGGED | RIGHT_DRAGGED | OTHER_DRAGGED => {
                let at = CGEventGetLocation(event);
                vec![Input::Motion {
                    at: Point::new(at.x, at.y),
                    dx: CGEventGetDoubleValueField(event, MOUSE_DELTA_X),
                    dy: CGEventGetDoubleValueField(event, MOUSE_DELTA_Y),
                    dragging: etype != MOUSE_MOVED,
                }]
            }
            LEFT_DOWN | LEFT_UP | RIGHT_DOWN | RIGHT_UP | OTHER_DOWN | OTHER_UP => {
                let n = CGEventGetIntegerValueField(event, MOUSE_BUTTON_NUMBER);
                let code = match n {
                    0 => ev::BTN_LEFT,
                    1 => ev::BTN_RIGHT,
                    2 => ev::BTN_MIDDLE,
                    3 => ev::BTN_SIDE,
                    4 => ev::BTN_EXTRA,
                    n => ev::BTN_LEFT + (n as u16).min(15),
                };
                let down = matches!(etype, LEFT_DOWN | RIGHT_DOWN | OTHER_DOWN);
                vec![Input::Button { code, down }]
            }
            KEY_DOWN | KEY_UP => {
                // The receiving side repeats held keys itself.
                if grabbed && CGEventGetIntegerValueField(event, KEY_AUTOREPEAT) != 0 {
                    return vec![];
                }
                let vk = CGEventGetIntegerValueField(event, KEY_KEYCODE) as u16;
                match mac_to_evdev(vk) {
                    Some(code) => vec![Input::Key {
                        code,
                        down: etype == KEY_DOWN,
                    }],
                    None => vec![],
                }
            }
            FLAGS_CHANGED => {
                let vk = CGEventGetIntegerValueField(event, KEY_KEYCODE) as u16;
                let flags = CGEventGetFlags(event);
                let Some(code) = mac_to_evdev(vk) else {
                    return vec![];
                };
                if code == ev::CAPSLOCK {
                    // Caps Lock reports a state change, not press/release: send a tap.
                    return vec![
                        Input::Key { code, down: true },
                        Input::Key { code, down: false },
                    ];
                }
                // Device-dependent bits distinguish left from right modifiers.
                let bit = match vk {
                    0x3B => 0x0000_0001, // left control
                    0x38 => 0x0000_0002, // left shift
                    0x3C => 0x0000_0004, // right shift
                    0x37 => 0x0000_0008, // left command
                    0x36 => 0x0000_0010, // right command
                    0x3A => 0x0000_0020, // left option
                    0x3D => 0x0000_0040, // right option
                    0x3E => 0x0000_2000, // right control
                    _ => return vec![],
                };
                vec![Input::Key {
                    code,
                    down: flags & bit != 0,
                }]
            }
            SCROLL_WHEEL => {
                let continuous = CGEventGetIntegerValueField(event, SCROLL_IS_CONTINUOUS) != 0;
                let phase = CGEventGetIntegerValueField(event, SCROLL_PHASE);
                let momentum = CGEventGetIntegerValueField(event, SCROLL_MOMENTUM_PHASE);
                // Quartz: positive = content towards the top/left; Wayland is the reverse.
                let dx = -CGEventGetDoubleValueField(event, SCROLL_POINT_DELTA_AXIS_2);
                let dy = -CGEventGetDoubleValueField(event, SCROLL_POINT_DELTA_AXIS_1);
                if continuous && (phase != 0 || momentum != 0) {
                    // A trackpad (or Magic Mouse): send only what the fingers do, so the
                    // other computer scrolls as if they were on its own trackpad, momentum
                    // included.
                    return match phase {
                        0 | SCROLL_PHASE_MAY_BEGIN => vec![],
                        SCROLL_PHASE_ENDED | SCROLL_PHASE_CANCELLED => {
                            vec![Input::TrackpadScrollEnd]
                        }
                        _ => vec![Input::TrackpadScroll { dx, dy }],
                    };
                }
                let scroll = if continuous {
                    Scroll {
                        dx,
                        dy,
                        notches: None,
                    }
                } else {
                    let nx = -CGEventGetIntegerValueField(event, SCROLL_DELTA_AXIS_2) as i32;
                    let ny = -CGEventGetIntegerValueField(event, SCROLL_DELTA_AXIS_1) as i32;
                    Scroll {
                        dx: nx as f64 * 15.0,
                        dy: ny as f64 * 15.0,
                        notches: Some((nx, ny)),
                    }
                };
                vec![Input::Scroll(scroll)]
            }
            _ => vec![],
        }
    }
}

fn apply(action: &Action) {
    let Some(shared) = SHARED.get() else { return };
    unsafe {
        match action {
            Action::Grab if !shared.grabbed.swap(true, Ordering::SeqCst) => {
                CGAssociateMouseAndMouseCursorPosition(false);
                hide_cursor(shared);
            }
            Action::Release { warp } if shared.grabbed.swap(false, Ordering::SeqCst) => {
                CGWarpMouseCursorPosition(CGPoint {
                    x: warp.x,
                    y: warp.y,
                });
                CGAssociateMouseAndMouseCursorPosition(true);
                show_cursor(shared);
            }
            _ => {}
        }
    }
}

/// Let this background process hide the cursor. Private but long-standing (Synergy, Barrier
/// and Deskflow use it); set before every hide and show since macOS can drop it.
fn allow_background_cursor() {
    let key = CFString::from_static_string("SetsCursorInBackground");
    unsafe {
        let cid = _CGSDefaultConnection();
        CGSSetConnectionProperty(
            cid,
            cid,
            key.as_concrete_TypeRef() as *const c_void,
            CFBoolean::true_value().as_concrete_TypeRef() as *const c_void,
        );
    }
}

fn show_cursor(shared: &Shared) {
    allow_background_cursor();
    unsafe {
        for _ in 0..shared.hides.swap(0, Ordering::SeqCst) {
            CGDisplayShowCursor(CGMainDisplayID());
        }
        // If macOS reset the count under us these overshoot, but the next grab's check
        // re-hides. If it still says hidden, keep going (bounded) so it never stays invisible
        // here.
        for _ in 0..8 {
            if CGCursorIsVisible() {
                break;
            }
            CGDisplayShowCursor(CGMainDisplayID());
        }
    }
}

fn hide_cursor(shared: &Shared) {
    allow_background_cursor();
    unsafe { CGDisplayHideCursor(CGMainDisplayID()) };
    shared.hides.fetch_add(1, Ordering::SeqCst);
}

/// macOS occasionally shows the cursor again while it's on another computer (another app
/// setting its cursor, the Dock, display changes), leaving a frozen cursor on each screen.
/// Check a few times a second while grabbed and hide it again if so.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn keep_cursor_hidden(shared: &Shared) {
    let now = now_ms();
    let last = shared.checked.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 100
        || shared
            .checked
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_err()
    {
        return;
    }
    if unsafe { CGCursorIsVisible() } {
        tracing::debug!("cursor reappeared while remote; hiding it again");
        hide_cursor(shared);
    }
}

/// True while macOS withholds keystrokes from every app (a password field, a locked screen,
/// Terminal's Secure Keyboard Entry). The mouse still works; typing can't be forwarded.
/// Is Caps Lock on here?
pub fn caps_lock_on() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceFlagsState(state: i32) -> u64;
    }
    const HID_SYSTEM_STATE: i32 = 1;
    const ALPHA_SHIFT: u64 = 0x0001_0000;
    unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) & ALPHA_SHIFT != 0 }
}

/// How long since this Mac's own keyboard, mouse or trackpad was last used.
pub fn idle_time() -> Option<std::time::Duration> {
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    }
    const HID_SYSTEM_STATE: i32 = 1;
    const ANY_INPUT: u32 = !0;
    let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT) };
    std::time::Duration::try_from_secs_f64(seconds).ok()
}

pub fn keyboard_blocked() -> bool {
    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn IsSecureEventInputEnabled() -> bool;
    }
    unsafe { IsSecureEventInputEnabled() }
}

// ---------------------------------------------------------------------------------------------
// Displays and notifications

/// Online displays, including ones that are asleep (the "active" list drops those, and the
/// layout mustn't change just because the screens blanked). Mirrors are skipped.
pub fn displays() -> Vec<DisplayInfo> {
    let mut ids = [0u32; 32];
    let mut count = 0u32;
    if unsafe { CGGetOnlineDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return vec![];
    }
    ids[..count as usize]
        .iter()
        .copied()
        .filter(|id| unsafe { CGDisplayMirrorsDisplay(*id) } == 0)
        .map(|id| {
            let d = CGDisplay::new(id);
            let b = d.bounds();
            let px = d.display_mode().map(|m| m.pixel_width()).unwrap_or(0) as f64;
            DisplayInfo {
                id: id.to_string(),
                name: if d.is_builtin() {
                    "Built-in Display".into()
                } else {
                    format!("Display {id}")
                },
                rect: Rect::new(b.origin.x, b.origin.y, b.size.width, b.size.height),
                scale: if b.size.width > 0.0 {
                    px / b.size.width
                } else {
                    1.0
                },
                primary: d.is_main(),
            }
        })
        .collect()
}

/// Plain-text clipboard contents, if any.
pub fn clipboard_get() -> Option<(String, Vec<u8>)> {
    let out = std::process::Command::new("pbpaste")
        .env("LANG", "en_US.UTF-8")
        .output()
        .ok()?;
    (out.status.success() && !out.stdout.is_empty()).then(|| (TEXT_MIME.to_string(), out.stdout))
}

pub fn clipboard_set(mime: &str, data: &[u8]) {
    use std::io::Write;
    if !mime.starts_with("text/") {
        return;
    }
    let child = std::process::Command::new("pbcopy")
        .env("LANG", "en_US.UTF-8")
        .stdin(std::process::Stdio::piped())
        .spawn();
    if let Ok(mut child) = child {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(data);
        }
        let _ = child.wait();
    }
}

const TEXT_MIME: &str = "text/plain;charset=utf-8";

pub fn notify(title: &str, body: &str) {
    let script = format!(
        "display notification {} with title {}",
        applescript_string(body),
        applescript_string(title)
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn()
        // Collect it when it's done, so it doesn't linger as a zombie.
        .map(|mut child| std::thread::spawn(move || child.wait()));
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
