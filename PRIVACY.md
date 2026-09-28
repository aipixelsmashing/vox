# Privacy

Plain statement, no legal padding.

## What Vox sends

Two requests exist in the entire application:

1. **Model download.** When you choose a model, its files are fetched from the URLs listed in
   the model registry. Happens once per model. Skippable entirely by importing a model from a
   folder.
2. **Update check.** An HTTPS request to the update endpoint, by default at launch. It sends
   nothing but the request itself — no identifier, no version fingerprint beyond what is
   required to compare versions, no usage data. Set to manual or off in Settings → Privacy.

That is the complete list. There is no analytics endpoint, no crash reporter, no account
system, no license check, and no "anonymous usage statistics" toggle, because the toggle would
mean you had to trust a claim rather than read a guarantee.

Turn on **offline lock** in Settings → Privacy and the app will not construct an HTTP client at
all — including for updates.

## What stays on your machine

- **Audio** is held in memory, transcribed, and discarded. It is never written to disk unless
  you explicitly enable debug capture, which displays a persistent warning while active.
- **Transcripts** are stored in a local SQLite database with owner-only file permissions,
  capped by your retention settings, deletable individually or all at once. In v1.0 this
  database is not encrypted — file permissions and full-disk encryption are what protect it.
- **Settings** are a local JSON file.
- **Logs** never contain transcript text at any log level.

## What Vox learns about you

Vox watches the text field for 90 seconds after it inserts something, and notices when you
fix a word. It keeps the wrong form, the right form, a count, the dates and which apps —
never the sentence, and never from a password field. This runs from the first launch so the
list has time to build; Settings → Privacy states it and has a button that deletes all of
it, and "Watch my corrections" in Settings → Vocabulary turns it off. Only if you also turn
on "Apply what it has learned" does Vox start making a fix for you, and then only after it
has seen the same fix three times on at least two different days or sittings.

What that stores: the wrong form, the right form, a count, a date, and which apps it happened
in. Not the sentences. Not the surrounding text. Kilobytes in total, in the same local
database as your history.

Every learned term is listed in Settings → Vocabulary with where it came from, and every one
has a delete button. Deleting a term also deletes the evidence behind it, so it will not come
back. You can export the whole list as plain text, or turn the feature off and delete
everything it has collected.

This is off by default. It stays off until we have enough real-world data to know how often it
gets things wrong.

## Taking your data with you

**Export everything** in Settings → Privacy writes your full history as plain `.md` and
`.json`, and your vocabulary as a plain list. Settings are already readable JSON. Models are
ordinary files other tools can use.

The point is that if this project is ever abandoned, or you simply find something better,
nothing you have accumulated is trapped here.

## Clipboard

When Vox writes a transcript to the clipboard it marks it private, which keeps it out of
Windows clipboard history, the Microsoft cloud clipboard, and cooperating macOS clipboard
managers. This is a hint that well-behaved software honours; a clipboard manager that ignores
it will still capture the text. We can't control that and won't pretend to.

## Permissions

Vox asks for microphone access, and for accessibility and input-monitoring permission so it can
see the hotkey while unfocused and place text in the focused field. Those permissions are
powerful. What we do with them is in [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md), and the code
is open so you can check rather than trust.

## Verifying any of this

- Run a network monitor while dictating. With offline lock on you will see nothing.
- Read `src-tauri/src/telemetry.rs`. It is empty, and a test keeps it that way.
- Build from source.
