# Compatibility results

One file per release: `<version>.md`, recording the injection compatibility matrix run
described in [../TESTING.md](../TESTING.md). Each row records the application, the platform,
the method that succeeded, the outcome, and observed latency.

Committing these makes regressions visible across releases, and turns "it used to work in
Slack" into a checkable claim.
