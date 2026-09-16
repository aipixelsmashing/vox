# Prior art

This is a crowded category. Pretending otherwise would be dishonest and would lead to building
the wrong things. GitHub's `dictation` topic alone lists well over a thousand repositories,
and several are good.

## Open source

| Project | Stack | Notable |
| --- | --- | --- |
| Handy | Tauri + whisper.cpp, MIT | The closest architectural relative. Cross-platform, hotkey-driven, local. Its issue tracker is the best available catalogue of the failure modes in this space — the clipboard-restore race and the Wayland paste reliability threads in particular |
| Whispering | TypeScript/Electron | Polished, local-first, cloud optional |
| VoiceInk, Spokenly, and the macOS Swift/WhisperKit cluster | Swift | Excellent on Apple silicon, single-platform, several bundle both Parakeet and Whisper |
| hyprvoice | Go, Wayland-native | Serious about the Wayland injection problem: ydotool, wtype and clipboard fallbacks with restore |
| WhisperWriter | Python | Long-standing, simple, Python runtime cost |
| murmure, openless, and others | Rust/Tauri | Actively working the same Wayland and injection problems; worth reading their design discussions before repeating their mistakes |

## Commercial

Wispr Flow, Superwhisper, Aqua Voice, Dragon. Better polish and, in Wispr Flow's case,
excellent latency. All are subscription products; most process audio in the cloud; Wispr
Flow's own documentation acknowledges that dictated text is not concealed from Windows
clipboard managers.

## Where Vox actually differs

Not in the concept. The concept is settled — hold a key, speak, text appears. The
differentiation is in three specific decisions the field mostly hasn't made:

1. **Verified insertion.** Most tools use the save-clipboard / synthesise-paste / sleep /
   restore pattern and treat a successful call as a successful delivery. Vox uses delayed
   rendering on Windows, accessibility-API insertion with read-back verification on macOS, and
   refuses to report success on unverifiable Wayland paths. See
   [TEXT-INJECTION.md](TEXT-INJECTION.md).
2. **A real offline guarantee.** Not "local-first with an optional cloud model" — a lock that
   prevents the HTTP client from being constructed, with a test that proves it. Several
   "privacy-first" tools in this list ship cloud providers behind a toggle.
3. **Cross-platform parity as a requirement.** The best tools in this space are macOS-only.
   The best cross-platform ones treat Linux as a second-class port. Vox treats Wayland's
   constraints as a design input rather than a bug to work around.

Everything else — the tray, the history, the hotkey — is table stakes, and should be built by
reading what these projects learned rather than rediscovering it.

## Deliberately borrowed

- `win-text-inject`'s analysis of the four Windows clipboard defects.
- `keytap`'s framing of what a keyboard tap crate needs to do, which came out of exactly this
  use case.
- hyprvoice's and murmure's Wayland fallback ordering.
- Handy's issue tracker as a pre-written list of things that will go wrong.
