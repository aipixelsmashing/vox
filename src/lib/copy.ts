/**
 * Every user-facing string, from the copy deck in docs/UI-STATES.md. Components import from
 * here and never invent a string, so the same situation reads identically in a notification,
 * a toast and a pane. Add to the deck first, then here.
 */

export const copy = {
  history: {
    searchPlaceholder: 'Search transcripts…',
    empty: 'Nothing dictated yet. Hold right Option and speak.',
    disabled: 'History is off. Turn it on in Settings → Privacy.',
    noMatch: (query: string) => `No transcripts match “${query}”.`,
    loading: 'Loading…',
    notInserted: (reason: string) => `not inserted — ${reason}`,
    items: (n: number) => (n === 1 ? '1 item' : `${n} items`),
    words: (n: number) => (n === 1 ? '1 word' : `${n} words`),
    deleteAll: 'Delete all…',
    wipeConfirm: (n: number) => `Delete all ${n} transcripts? This can't be undone.`,
    wipeYes: 'Delete',
    wipeNo: 'Cancel',
    copy: 'Copy',
    insert: 'Insert',
    delete: 'Delete',
    more: 'More',
    less: 'Less',
    copied: 'Copied',
    live: {
      arming: 'Opening the microphone…',
      recording: 'Listening…',
      transcribing: 'Transcribing…',
      injecting: 'Placing the text…',
    },
  },
  time: {
    justNow: 'just now',
    minutes: (m: number) => `${m} min ago`,
    hours: (h: number) => `${h} h ago`,
    days: (d: number) => `${d} d ago`,
  },
} as const
