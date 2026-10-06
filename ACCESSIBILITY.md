# Accessibility

How DrawnUI for Rust gives screen readers and keyboards its drawn controls, what an app does for it,
and where it was checked. Per-feature parity with DrawnUI for .NET and React: `PARITY.md`
("Accessibility, keyboard navigation", "SkiaProgress, SkiaSlider").

## How it works

**One snapshot, every platform.** `Ui` builds an accessibility snapshot (`AccessibilityNode`,
`drawnui/src/ui.rs` `build_accessibility`) of every visible control that has a role: its label,
hint, drawn rect in points (through transforms and scroll offsets), whether it takes input, a
toggle's state, a range control's value, a scroll's axes, its live-region politeness, the text lines
of selectable text, the nearest node above it and its arrow-key group. Nodes are sorted in reading
order: by top, and nodes whose tops are within half the smaller height form a row, read left to
right. The snapshot is rebuilt at the end of a frame, at most once per `ACCESSIBILITY_INTERVAL_MS`
(1 s), and on the very next frame after a screen reader acts (a click, Increment / Decrement,
SetValue), so the reader reads the new value or state back at once. It is built only while a host
renders it (a screen reader is connected, or the browser overlay is on).

**Hosts.**

| Platform | Bridge | Screen readers |
|---|---|---|
| Browser | an ARIA overlay of invisible elements over the canvas (`drawnui/web/drawnui_host.js`) | NVDA, JAWS, VoiceOver, TalkBack in the browser |
| Windows | AccessKit, UI Automation (`drawnui/src/host_access.rs`) | Narrator, NVDA |
| macOS, iOS | AccessKit, NSAccessibility / UIAccessibility | VoiceOver |
| Linux | AccessKit, AT-SPI | Orca |
| Android | AccessKit, the GameActivity view | TalkBack |

The AccessKit tree is nested (a card holds its items); the browser overlay is flat.

**Roles, names, values.**

- A role comes from the control's `accessibility_role`, else the app's default for its type
  (`Ui::default_accessibility_role::<T>`), else the control's own (`Control::accessibility_role`:
  buttons, toggles, sliders, progress bars, editors...). No role: not in the tree.
- The name is `accessibility_label`, else the control's own text (a label's text, a button's
  caption). A name is said once: a card titled by a text or heading child is not named again.
- A range control (slider, progress bar) has a value, not a name made of its value
  (`AccessibilityValue`: now, min, max, step, and a spoken text where the number alone is not it,
  "65%", "20 – 80"). It is horizontal (without an orientation VoiceOver on macOS reads a "circular
  slider" and its adjust jumped to the ends).
- A control role that takes no input is read as unavailable (aria-disabled, UI Automation
  IsEnabled false).

**Actions a screen reader sends.**

| Action | What the engine does |
|---|---|
| Click / press / double tap | a Tapped at the node's center (`Ui::accessibility_activate`); a text field takes the caret |
| Focus | the scrolls above the node bring it into view (8 pt padding, 250 ms); an editor takes the caret |
| Increment / Decrement | ArrowUp / ArrowDown to the control, as the keyboard: a slider moves one step |
| SetValue | `Control::accessibility_set_value`, snapped to the step |
| ScrollIntoView | every node: as Focus, without moving the focus |
| ScrollUp / Down / Left / Right | a scroll's node pages its content by the viewport less a tenth, animated (`Ui::accessibility_scroll`) |

Every SkiaScroll is a node holding its content's nodes (`Aria::SCROLL_VIEW`). On iOS (VoiceOver's
three-finger swipe) and Android (TalkBack scroll forward / back) it pages; on the desktops the
adapters page nothing, so there it is a container they leave out (no extra "region" level), and the
browser overlay leaves it out too.

**Focus.** When the node a screen reader was on disappears (its page closed), the host moves the
AccessKit focus to the first node on screen, in reading order, that says something
(`host_access::refocus`); otherwise VoiceOver kept its cursor on the empty spot. On the desktop the
screen reader follows the keyboard focus.

**Keyboard (desktop hosts, DrawnUI's own navigation).** Tab / Shift+Tab walk the tab stops in
reading order; an arrow-key group (list, listbox, grid, toolbar, radiogroup, tablist, menu,
menubar) is one stop that remembers its item and moves by index; Home / End, PageUp / PageDown;
Enter / Space activate; Escape leaves; a focus ring after keyboard use. The browser keeps its own
Tab order and ring through the overlay (roving tabindex for groups).

**Selectable text.** `accessibility_text_selectable` on a label: the browser overlay renders its
lines as real selectable text; the desktop hosts draw the selection (drag, double click, Ctrl / Cmd+C,
Ctrl / Cmd+A; touch long press and a Copy button).

## What an app does

- Name every control that has no text of its own by what it controls, never by its role:
  `.accessibility_label("Wi-Fi")` on a switch, "Volume" on a slider, "Download" on a progress bar.
- Give a container a role when it is a group a screen reader should know (`Aria::LIST`,
  `Aria::TOOLBAR`...): arrow keys then move inside it.
- A custom range control implements `Control::accessibility_value` (and `accessibility_set_value`
  when it can be set); a custom control that is a button sets `accessibility_role(Aria::BUTTON)`.
- `accessibility_live("polite")` on a status text that should be read when it changes.

## Checked

| Platform | How | Result |
|---|---|---|
| Windows | UI Automation client (names, roles, bounds, Invoke, RangeValue, SetValue, ScrollItem), posted keys | names and values read; SetValue 30 moves the slider; the last slider scrolled into view in one call; Tab walks the cards |
| Browser | Chrome AX tree and DOM (aria-label, aria-value*, aria-checked), real keys | values present, ArrowUp 65 to 66, groups as one Tab stop |
| macOS | AX API dump and VoiceOver's speech log | cards and controls named once; sliders "65 % Volume slider"; Increment / Decrement step; Back lands on the catalog's first node |
| iPhone | XCUITest tree (names, values), VoiceOver by hand | names and values read; the stale cursor after Back (fix e800611) is fixed on macOS, its iPhone retry and the slider adjust are pending |
| Android | TalkBack on an emulator and a phone (BV8800), uiautomator tree | explore by touch, swipes in reading order, double tap opens a card |
| Linux | Orca 42 in WSL, its speech log; AT-SPI tree and actions | Orca reads the catalog and pages |

## Not yet

- macOS: VoiceOver does not page a scroll (accesskit_macos has no scroll actions), and the slider
  adjust after the read-back fix (d93dbe4) is not rechecked there yet.
- iOS: UI-test "scroll to visible" (kAXScrollToVisibleAction) is not mapped by accesskit_ios 0.2.1;
  VoiceOver focus moves scroll through Focus instead. accesskit_ios maps a horizontal three-finger
  swipe the opposite way to Android's forward.
- Items of a recycled list that are not realized yet cannot be focused by the keyboard; live
  regions wait for the snapshot interval.
