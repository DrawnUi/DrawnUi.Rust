//! Keyboard input: key names, the modifier state, the events controls and apps receive, and the
//! host's cursor. Key names are the DOM `KeyboardEvent.code` values ("KeyA", "ArrowLeft",
//! "Space", "ShiftLeft", ...) on every host, as DrawnUI.Blazor and React (KeyboardManager).

/// A key by its DOM `code` name: `"KeyA"`, `"Digit1"`, `"ArrowLeft"`, `"Space"`, `"Enter"`,
/// `"Backspace"`, `"ShiftLeft"`, `"F1"`, ... `"Unknown"` for a key the host has no name for.
pub type InputKey = &'static str;

/// Every key name (the W3C `code` values winit and the browsers share), so a name a host gets as
/// text becomes a static one without allocating.
macro_rules! with_key_codes {
    ($callback:ident) => {
        $callback! {
            Backquote, Backslash, BracketLeft, BracketRight, Comma, Digit0, Digit1, Digit2, Digit3, Digit4, Digit5,
            Digit6, Digit7, Digit8, Digit9, Equal, IntlBackslash, IntlRo, IntlYen, KeyA, KeyB, KeyC, KeyD, KeyE, KeyF,
            KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX,
            KeyY, KeyZ, Minus, Period, Quote, Semicolon, Slash, AltLeft, AltRight, Backspace, CapsLock, ContextMenu,
            ControlLeft, ControlRight, Enter, ShiftLeft, ShiftRight, Space, Tab, Convert, KanaMode, Lang1, Lang2,
            Lang3, Lang4, Lang5, NonConvert, Delete, End, Help, Home, Insert, PageDown, PageUp, ArrowDown, ArrowLeft,
            ArrowRight, ArrowUp, NumLock, Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7,
            Numpad8, Numpad9, NumpadAdd, NumpadBackspace, NumpadClear, NumpadClearEntry, NumpadComma, NumpadDecimal,
            NumpadDivide, NumpadEnter, NumpadEqual, NumpadHash, NumpadMemoryAdd, NumpadMemoryClear,
            NumpadMemoryRecall, NumpadMemoryStore, NumpadMemorySubtract, NumpadMultiply, NumpadParenLeft,
            NumpadParenRight, NumpadStar, NumpadSubtract, Escape, Fn, FnLock, PrintScreen, ScrollLock, Pause,
            BrowserBack, BrowserFavorites, BrowserForward, BrowserHome, BrowserRefresh, BrowserSearch, BrowserStop,
            Eject, LaunchApp1, LaunchApp2, LaunchMail, MediaPlayPause, MediaSelect, MediaStop, MediaTrackNext,
            MediaTrackPrevious, Power, Sleep, AudioVolumeDown, AudioVolumeMute, AudioVolumeUp, WakeUp, Hyper,
            Turbo, Abort, Resume, Suspend, Again, Copy, Cut, Find, Open, Paste, Props, Select, Undo, Hiragana,
            Katakana, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, F13, F14, F15, F16, F17, F18, F19, F20,
            F21, F22, F23, F24, F25, F26, F27, F28, F29, F30, F31, F32, F33, F34, F35
        }
    };
}
pub(crate) use with_key_codes;

macro_rules! key_code_names {
    ($($code:ident),*) => { &[$(stringify!($code),)* "MetaLeft", "MetaRight"] };
}

/// Every key name, as `InputKey` values.
pub const KEY_CODES: &[InputKey] = with_key_codes!(key_code_names);

/// The static name of a key the host reports as text (a DOM `code`); `"Unknown"` when it is not
/// one of `KEY_CODES`.
pub fn key_name(code: &str) -> InputKey {
    KEY_CODES.iter().copied().find(|k| *k == code).unwrap_or("Unknown")
}

/// The modifier keys held during an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Command on macOS, the Windows key.
    pub meta: bool,
}

impl Modifiers {
    /// Bits as the hosts send them: 1 shift, 2 ctrl, 4 alt, 8 meta.
    pub fn from_bits(bits: u32) -> Self {
        Self { shift: bits & 1 != 0, ctrl: bits & 2 != 0, alt: bits & 4 != 0, meta: bits & 8 != 0 }
    }
}

/// What a keyboard event is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyKind {
    Down,
    Up,
    /// Text typed: a printable key without Ctrl / Alt / Meta (React KeyChar), an IME commit, a
    /// paste through the host's text input. `KeyEvent::text` carries it; `key` is empty.
    Char,
}

/// The keyboard state the engine keeps (React KeyboardManager): the modifiers and which keys
/// are held. Read it from a key event or from `Ui::keyboard`.
#[derive(Default, Debug)]
pub struct Keyboard {
    pub modifiers: Modifiers,
    pressed: Vec<InputKey>,
}

impl Keyboard {
    pub fn is_shift_pressed(&self) -> bool {
        self.modifiers.shift
    }
    /// Ctrl, or Meta (React counts Command as Control).
    pub fn is_control_pressed(&self) -> bool {
        self.modifiers.ctrl || self.modifiers.meta
    }
    pub fn is_alt_pressed(&self) -> bool {
        self.modifiers.alt
    }
    /// The key is held down right now (games move while it is).
    pub fn is_pressed(&self, key: &str) -> bool {
        self.pressed.iter().any(|k| *k == key)
    }

    pub(crate) fn apply(&mut self, kind: KeyKind, key: InputKey, modifiers: Modifiers) {
        self.modifiers = modifiers;
        match kind {
            KeyKind::Down if !self.is_pressed(key) => self.pressed.push(key),
            KeyKind::Up => self.pressed.retain(|k| *k != key),
            _ => {}
        }
    }

    /// The window lost the keyboard: no key is held anymore (React clears on `blur`).
    pub(crate) fn reset(&mut self) {
        self.pressed.clear();
        self.modifiers = Modifiers::default();
    }
}

/// One keyboard event as controls and app handlers see it. Nothing in it is allocated.
#[derive(Clone, Copy, Debug)]
pub struct KeyEvent<'a> {
    pub kind: KeyKind,
    /// The key for Down / Up; `""` for Char.
    pub key: InputKey,
    /// The text for Char; `""` otherwise.
    pub text: &'a str,
    /// The key is held and auto-repeating (Down only).
    pub repeat: bool,
    /// The modifiers and the held keys as of this event.
    pub keyboard: &'a Keyboard,
}

impl KeyEvent<'_> {
    pub fn modifiers(&self) -> Modifiers {
        self.keyboard.modifiers
    }
}

/// The mouse cursor the app asks the host for (React: `cursor: pointer` over interactive controls).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Cursor {
    #[default]
    Default,
    /// The hand: something under the mouse can be tapped.
    Pointer,
    /// The I-beam: text can be selected or edited.
    Text,
}
