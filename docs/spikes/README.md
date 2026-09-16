# Spikes

Milestone M0 is four questions that can each kill the project. Each spike is throwaway code
plus a written finding in this directory. Nothing in M1 starts until all four answer yes and
any revised numbers are back in the docs.

| Spike | Question | Finding |
| --- | --- | --- |
| S1 | Does `keytap` deliver right-Alt down/up reliably on macOS, Windows, X11 and Wayland, from an unfocused app, without eating the key? | `s1-hotkey.md` |
| S2 | Does accessibility insertion work in the top ten target apps on macOS? Does delayed-rendering clipboard insertion beat the restore race on Windows? What works on GNOME 46+ Wayland via libei? Can we read the field back afterwards? | `s2-injection.md` |
| S3 | Warm Parakeet TDT int8, 6 s utterance: is inference inside the 150–400 ms budget on each reference machine? | `s3-latency.md` |
| S4 | With mmapped weights, what is idle RSS and what does a warm reload from page cache cost? Does a smaller model plus learned vocabulary beat a larger generic one on a real user's words? | `s4-footprint.md` |

A finding is one page: what was tried, what happened, numbers, and what it means for the
design. A spike that concludes "it works" without numbers has not concluded anything.
