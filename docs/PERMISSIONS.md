# OS permissions

Vox needs unusually powerful permissions: it watches every keystroke and can drive other
applications' UIs. That deserves both careful handling and a plain explanation to the user.
Onboarding asks for each one at the moment it becomes necessary, with a sentence saying what
it is for — never a wall of requests at launch.

## macOS

| Permission | Needed for | How |
| --- | --- | --- |
| **Microphone** | Recording | Standard `AVCaptureDevice` prompt on first use. `NSMicrophoneUsageDescription` in Info.plist |
| **Input Monitoring** | Seeing the hotkey while unfocused | `IOHIDRequestAccess(kIOHIDRequestTypeListen)` shows the system prompt and adds the app to the pane. **`keytap` only checks (`IOHIDCheckAccess`) and never requests**, so the app must make this call itself before creating the tap ([S1](spikes/s1-hotkey.md)). Onboarding also opens `x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent` and explains the toggle |
| **Accessibility** | Reading the focused element and inserting text | `AXIsProcessTrustedWithOptions` with the prompt option, plus a direct link to `…?Privacy_Accessibility`. Note: being trusted does not make the system-wide `AXUIElement` work on macOS 26.5; use the per-application element ([TEXT-INJECTION.md](TEXT-INJECTION.md#macos)) |
| **Notifications** | Telling the user why text went to the clipboard instead of the field | `UNUserNotificationCenter.requestAuthorization` on the first notification, from the Swift bridge. Not at launch: the first failure is the first time the prompt has a reason. Declining loses nothing that matters — the text is still on the clipboard and in history — but the user is not told why |
| **Speech model assets** | First transcription in a locale | Not a permission, but a first-use step: `AssetInventory.status` reports `supported` until `assetInstallationRequest(...).downloadAndInstall()` has run once. Apple's download, not ours; under a second on a machine with dictation already installed ([S3](spikes/s3-engine.md)). Needs the network once; `network.offlineLock` blocks it and says so |

Notes that will otherwise cost days:

- Both Input Monitoring and Accessibility require **restarting the app** after being granted.
  Onboarding says so and offers a "Quit and reopen" button.
- The grant is keyed to the code signature. An unsigned dev build and a signed release build
  are different entries; developers will grant it repeatedly during development.
- `IsSecureEventInputEnabled()` can be stuck on because some other app leaked the state. When
  detected, the UI names it as a system-wide condition rather than a Vox failure.
- Entitlements: hardened runtime and `com.apple.security.device.audio-input`.
  `com.apple.security.cs.disable-library-validation` was for the ONNX Runtime dylib; v1 ships
  no ONNX runtime ([adr/0016](adr/0016-macos-first.md)) and does not carry it.

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
