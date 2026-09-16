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

Six values, plus one failure colour used nowhere else.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
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

## Typography

Two faces, with a clear division of labour.

**Chrome — the system UI font.** `system-ui` on every platform. A utility that sits beside
native windows should use the same letterforms they do. Shipping a branded sans for menu labels
would make Vox look like a visitor.

**Transcripts — Literata**, bundled locally (no CDN — see [THREAT-MODEL.md](THREAT-MODEL.md)).
Transcripts are the user's own prose, not interface furniture, and giving them a serif marks
that difference the instant you look at the panel. Literata is drawn for screen reading at
small sizes, which is the whole job here.

| Token | Size / line-height | Use |
| --- | --- | --- |
| `--t-transcript` | 15 / 1.55, Literata | Transcript text in the panel and long-form |
| `--t-body` | 13 / 1.45, system-ui | Labels, settings, buttons |
| `--t-meta` | 11 / 1.3, system-ui, `--ink-dim` | Timestamps, app names, provenance |
| `--t-title` | 15 / 1.3, system-ui, 600 | Window and pane titles |
| `--t-numeric` | 12 / 1.3, `tabular-nums` | Timers, byte counts, diagnostics |

No all-caps labels. No letter-spacing on anything under 20px. Sentence case everywhere,
including buttons.

## Layout

- **4px base grid.** A tray utility is dense; an 8px grid wastes the panel's limited height.
- **Panel width 380px.** Fits beside a tray icon on a laptop without covering the work behind it.
- **Row height 56px** in the history panel: two lines of transcript plus metadata, and a
  comfortable pointer target.
- **Hairlines, not cards.** Rows are separated by 1px rules. Nothing floats, nothing has a
  shadow except the panel itself, which has one because it is a real window over other windows.
- **Radius 6px** on the panel and on inputs; **0** on rows. Radius signals "this is a separate
  surface", so applying it to everything destroys the signal.
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
| Listening | Filled mark + amber level ring |
| Transcribing | Filled mark + rotating tick |
| Attention | Outline mark + corner badge |
| Always listening | Outline mark + persistent amber dot (pre-roll enabled) |

## What good looks like

Someone should be able to open the history panel, find the thing they said four minutes ago,
and close it, without reading a single label. If a screen needs an explanatory sentence, the
layout is wrong.
