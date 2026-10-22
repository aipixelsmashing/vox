# Design system

Decisions, made once, so nobody has to relitigate them in a pull request. Tokens live in
`src/styles/tokens.css` and this document explains why they are what they are.

## The design problem

Vox appears in glances of two seconds, at the edge of the screen, next to the user's real work,
on three operating systems, in light and dark. It is a **system utility**, not an app someone
sits inside. It should look like it belongs to the machine.

That rules out the SaaS kit: identical rounded cards, one border-radius on everything, soft
grey shadows, gradient washes, tracked-out all-caps labels. Those read as "web app pasted onto
a desktop" and they cost legibility at the sizes we actually use.

**Spend the boldness in one place: the recording indicator.** Everything else stays quiet.

## Colour

Six values, one failure colour used nowhere else, and the person's own accent colour for
selection and focus.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--accent` | `AccentColor` | `AccentColor` | **Selection and keyboard focus**, with `--accent-text` on top. The colour chosen in System Settings → Appearance, as in every list on the Mac; `#007AFF` where the keyword is unknown |
| `--ink` | `#16171A` | `#E9E9EC` | Primary text |
| `--ink-dim` | `#5C5F66` | `#9A9CA3` | Secondary text, provenance, timestamps |
| `--panel` | `#FFFFFF` | `#1C1D20` | Panel and window backgrounds |
| `--rule` | `#00000014` | `#FFFFFF1A` | Hairlines. **Never** a shadow where a rule will do |
| `--field` | `#00000008` | `#FFFFFF0D` | Inputs, hover rows |
| `--signal` | `#E0A106` | `#F0B62E` | **Listening.** Nothing else, ever |
| `--fail` | `#C1382E` | `#E06A5F` | Insertion failed, verification failed, hash mismatch |

Two deliberate choices worth defending:

**Listening is amber, not red.** Red is the convention for recording, and it is also the colour
every operating system uses for "something is wrong". A tray icon that goes red whenever you
speak trains people to feel alarm at their own voice. Amber reads as *live* without reading as
*alert*, and it leaves red meaning exactly one thing here: something failed.

**No success colour.** There is no green anywhere. Success is the text appearing in the field.
Adding a colour for "it worked" would mean drawing attention to the 99% case.

State is never carried by colour alone — the tray icon silhouette differs per state, and every
failed row carries words as well as a colour.

**Selection is the system accent, never amber.** The focus ring used to be amber, which broke
the first rule above: amber was then listening *and* focus. Focus rings and the selected row
take `--accent`, the colour the person chose for exactly this purpose, and the selected row's
text takes `--accent-text` (white on every accent except graphite). That is what
[focus-and-selection.md](https://developer.apple.com/design/human-interface-guidelines/focus-and-selection)
asks for: a highlight in a list, a ring in a text field, both in the app's accent.

## Typography

One typeface: the system's, `system-ui`, which is San Francisco on a Mac. A utility that
sits beside native windows should use the same letterforms they do, and every system panel
(Spotlight, Control Center, Notification Center) is set in one face. Transcripts are the
user's own prose, not interface furniture, and the panel says so by **size and colour
only**: transcripts at 13 in `--ink`, metadata at 11 in `--ink-dim`, titles by weight.
Nothing is bundled and nothing is fetched (no CDN — see [THREAT-MODEL.md](THREAT-MODEL.md)).

Two earlier choices are gone for the record: Literata, bundled, was a third voice beside the
two the OS speaks; New York via `ui-serif` paired with San Francisco but still made the
panel the one menu bar panel on the Mac set in a serif.

| Token | Size / line-height | Use |
| --- | --- | --- |
| `--t-transcript` | 13 / 1.45, `--ink` | Transcript text in the panel and long-form |
| `--t-body` | 13 / 1.45, system-ui | Labels, settings, buttons |
| `--t-meta` | 11 / 1.3, system-ui, `--ink-dim` | Timestamps, app names, provenance |
| `--t-title` | 15 / 1.3, system-ui, 600 | Window and pane titles |
| `--t-numeric` | 12 / 1.3, `tabular-nums` | Timers, byte counts, diagnostics |

No all-caps labels. No letter-spacing on anything under 20px. Sentence case everywhere,
including buttons.

## Layout

- **4px base grid.** A tray utility is dense; an 8px grid wastes the panel's limited height.
- **Panel width 380px.** Fits beside a tray icon on a laptop without covering the work behind it.
- **History rows are a source list, not a table.** Inset 8px from the panel edge, 8px of
  padding, no rules between them; the selected row is a rounded accent-coloured highlight, the
  way Finder's sidebar and Spotlight's results select. Metadata, two lines of transcript, and
  the row's actions only on the selected row, so the resting list is text and nothing else.
- **Hairlines, not cards.** Where a rule is needed (the header, the footer) it is 1px. Nothing
  floats, nothing has a shadow except the panel itself, which is a real window over other
  windows and takes the system's own shadow and the popover material.
- **Radius 12px** on the panel, as a menu bar panel on macOS 26; **6px** on inputs and on the
  selected row; **0** on everything else. Radius signals "this is a separate surface", so
  applying it to everything destroys the signal.
- Left-aligned throughout. Numbers right-aligned in diagnostics tables.

## Motion

Two moments, and no others.

1. **The level ring** on the recording indicator, driven by input amplitude. This is not
   decoration — it is the answer to "is it hearing me?", the question the user has at that exact
   moment. It responds to the person, so it is exempt from the usual restraint.
2. **Panel open**, 120ms, opacity and 4px rise. Once.

No hover transitions on rows, no fade-and-slide on lists, no skeleton shimmer. `prefers-reduced-motion`
removes the panel animation entirely and replaces the level ring with a static three-step meter.

## Icon states

Silhouettes, so they are distinguishable in a monochrome menu bar and by someone who cannot
distinguish amber from grey.

| State | Silhouette |
| --- | --- |
| Idle | Outline mark |
| Listening | Filled mark inside a closed ring |
| Transcribing | Filled mark inside an open arc (one frame of the rotating tick) |
| Attention | Outline mark + corner badge, top right |
| Always listening | Outline mark + persistent dot, bottom right (pre-roll enabled) |

The tray icons are macOS **template images**: the system reads only their alpha channel and
draws the opaque pixels in the menu bar's own colour, light or dark. Amber cannot appear
there, and a coloured, non-template icon is invisible in one of the two menu bar
appearances. So the tray carries state by silhouette alone, and the amber level ring lives on
the recording overlay beside the caret ([UI-SPEC.md](UI-SPEC.md#recording-overlay)). The
files are drawn by `scripts/make-tray-icons.py`, black on transparent, at 22 px and 44 px;
`tests/guards.rs` fails the build if one of them ever ships with an opaque background again.

## What good looks like

Someone should be able to open the history panel, find the thing they said four minutes ago,
and close it, without reading a single label. If a screen needs an explanatory sentence, the
layout is wrong.
