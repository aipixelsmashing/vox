# S2 — Insertion and read-back: does accessibility insertion work, and can we read the field back?

**Answer: yes where the target exposes a settable `AXSelectedText`, and the read-back works
in every app tested.** Three of the roadmap's target apps were run; the rest are marked
untested below and move to the M2 compatibility matrix.

Run 2026-09-15 on macOS 26.5.2 (25F84), Apple M1 Pro, `axuielement` 0.9.1, release build,
launched from the Claude desktop app's integrated terminal (Accessibility granted to that
app). Harness: [`spikes/s2-inject`](../../spikes/s2-inject/src/main.rs), run with
`--method both` so each app gets the accessibility path and then the paste path into the
same field. Test string: `vox s2 test 123 — ünïcödé ✓ ` (28 UTF-16 units, trailing space).

## Results

| App | Focused element | Accessibility insert | Clipboard paste | Read-back of field |
| --- | --- | --- | --- | --- |
| Notes | `AXTextArea`, `AXSelectedText` settable | **Inserted.** Set ok in 7–15 ms; caret +28, count +28, value gained one copy | **Inserted.** Verified 81 ms after Cmd+V, same three checks | Full value readable |
| Terminal | `AXTextArea`, `AXSelectedText` **not** settable | **Silent no-op.** Set returned ok in 0.04 ms; caret +0, count +0, nothing arrived | **Inserted.** Verified 37 ms after Cmd+V | Full scrollback readable (5 k chars) |
| Brave address bar | `AXTextField`, settable | **Inserted.** Set ok in 2 ms; exact match | **Inserted, normalised.** Landed, but the omnibox trimmed the trailing space: caret +27, count +27 | Full value readable |
| Slack | — | untested | untested | untested |
| Chrome/Brave web textarea | — | untested | untested | untested |
| VS Code | — | untested | untested | untested |
| Second Electron app | — | untested | untested | untested |

Latency of the accessibility set call: 2–15 ms. Paste evidence appeared 37–81 ms after the
synthetic Cmd+V.

## What it means for the design

1. **[ADR 0005](../adr/0005-no-unverified-injection-success.md) is vindicated on the first
   non-trivial app.** Terminal accepts `AXUIElementSetAttributeValue(AXSelectedText)` with
   `kAXErrorSuccess` and inserts nothing. Only the verification step caught it. A chain that
   trusted the return value would report `Inserted` and lose the text.

2. **The designed way to find the focused element does not work on this machine.**
   [TEXT-INJECTION.md](../TEXT-INJECTION.md) step 2 reads `kAXFocusedUIElementAttribute`
   from `AXUIElementCreateSystemWide()`. From a trusted process (`AXIsProcessTrusted` true)
   every attribute read on the system-wide element returns `kAXErrorCannotComplete`, via the
   raw C API and via `axuielement` alike. `AXUIElementCreateApplication(pid)` for
   `NSWorkspace.frontmostApplication` works and its `AXFocusedUIElement` is the right
   element. The harness falls back to that route and every successful row above used it.
   Observed only from the integrated terminal; a Terminal.app comparison was not completed,
   so whether this is launch-context-specific or general to macOS 26.5 is **open**. M1
   should implement the per-app route as primary and keep the system-wide route as a
   first attempt.

3. **Verification must tolerate target-side normalisation.** The spec says confirm the
   caret advanced by the inserted length. The Brave omnibox trimmed the trailing space, so an
   exact check reported `ClipboardOnly` on a successful insert. The harness now accepts the
   `trim_end` form as evidence and labels it `~`. The product needs the same tolerance,
   at minimum for trailing whitespace; smart-quote and autocomplete substitutions are
   plausible next cases.

4. **The pasteboard `changeCount` cannot detect a paste.** The spec restores the previous
   clipboard "once `changeCount` has moved again". A read by the target never increments
   `changeCount`; only writes do. The only evidence a paste landed is the field itself,
   read through accessibility. Where that is unavailable the 1500 ms timeout is the only
   signal, and per the spec the transcript then stays on the clipboard. Update the doc.

5. **`objc2-speech` cannot reach SpeechAnalyzer, and `axuielement` needs a build fix.**
   Related build findings from the same session: `axuielement` 0.9 compiles a Swift bridge
   and its build script assumes an Xcode.app toolchain path; with Command Line Tools only
   the link fails on `swiftCompatibility56` until `<xcode-select -p>/usr/lib/swift/macosx`
   is added ([`s2-inject/build.rs`](../../spikes/s2-inject/build.rs)). `src-tauri` will
   need the same. And `objc2-speech` 0.3 only wraps the Objective-C `SFSpeechRecognizer`
   API; SpeechAnalyzer is Swift-only, so the product needs its own Swift bridge for the
   engine (see S3).

6. Restoring the pasteboard restores only the string type. Rich clipboard contents (HTML,
   files) were present before one run and would be lost by the current restore. The product
   should snapshot all pasteboard items, not the string.

## Open items

- The four untested apps. Slack, VS Code and Electron are where `AXSelectedText` is most
  likely to be unsettable or the web area to lag; run before M2's gate.
- Terminal.app launch comparison for the system-wide element failure.
- Whether the concealed pasteboard type is honoured by any clipboard manager the user runs.
