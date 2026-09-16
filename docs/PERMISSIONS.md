# OS permissions

Vox needs unusually powerful permissions: it watches every keystroke and can drive other
applications' UIs. That deserves both careful handling and a plain explanation to the user.
Onboarding asks for each one at the moment it becomes necessary, with a sentence saying what
it is for — never a wall of requests at launch.

## macOS

| Permission | Needed for | How |
| --- | --- | --- |
| **Microphone** | Recording | Standard `AVCaptureDevice` prompt on first use. `NSMicrophoneUsageDescription` in Info.plist |
| **Input Monitoring** | Seeing the hotkey while unfocused | Cannot be prompted programmatically in a useful way. Onboarding opens `x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent` and explains the toggle |
| **Accessibility** | Reading the focused element and inserting text | `AXIsProcessTrustedWithOptions` with the prompt option, plus a direct link to `…?Privacy_Accessibility` |

Notes that will otherwise cost days:

- Both Input Monitoring and Accessibility require **restarting the app** after being granted.
  Onboarding says so and offers a "Quit and reopen" button.
- The grant is keyed to the code signature. An unsigned dev build and a signed release build
  are different entries; developers will grant it repeatedly during development.
- `IsSecureEventInputEnabled()` can be stuck on because some other app leaked the state. When
  detected, the UI names it as a system-wide condition rather than a Vox failure.
- Entitlements: hardened runtime, `com.apple.security.device.audio-input`, and
  `com.apple.security.cs.disable-library-validation` for the ONNX Runtime dylib.

## Windows

No formal permission model for the pieces we use, but three real constraints:

- **`WH_KEYBOARD_LL` may trip antivirus heuristics.** Mitigation: Authenticode signing (see
  [PACKAGING.md](PACKAGING.md)), reputation building through consistent signing, and a
  documented explanation to point support requests at.
- **UIPI blocks injection into elevated windows.** Detected and reported, never silently
  swallowed. We do **not** ship an elevated helper to work around it: a background process
  running as administrator that types into any window is a much larger risk than the
  inconvenience it solves.
- **Microphone access** can be revoked in Settings → Privacy → Microphone. Detect denial and
  link to the pane.

## Linux

- **Keyboard capture** reads `/dev/input/event*`, which requires membership of the `input`
  group. The installer does not silently add the user to it. Onboarding shows the exact
  command and explains what it grants:
  `sudo usermod -aG input $USER` (then log out and back in), or a udev rule scoped to the
  keyboard device for the more careful.
- **Audio** via PipeWire or PulseAudio; Flatpak builds request the audio socket.
- **Injection** — see [TEXT-INJECTION.md](TEXT-INJECTION.md). On Wayland the portal grants a
  RemoteDesktop session; the user will see a compositor prompt, which onboarding warns about
  so it isn't mistaken for something suspicious.

## Design rules

1. **Ask late, ask once, explain why.** No permission is requested before the user does
   something that needs it.
2. **Degrade visibly.** Missing permissions produce a badged tray icon and a one-line
   explanation, never a silently dead hotkey.
3. **Re-check on resume.** Permissions can be revoked while the app runs; state is re-verified
   on wake and on window focus.
4. **Never work around a denial.** No elevated helpers, no root daemons, no accessibility
   spoofing. If the platform says no, the user is told what to change.
