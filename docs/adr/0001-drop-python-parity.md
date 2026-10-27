# Drop parity with the Python app

This crate began as a port of the Python PlaylistGen, with the Python source as the spec (see `PLAN.md`) and a golden test (`tests/parity.rs`) checking byte-identical output. We no longer treat the Python app as the spec: playlist output, summary text, menu text and Python quirks kept for compatibility may change when that makes the app better. The layout file keeps its current format and location for now, but carries no promise to stay readable by the Python app.

## Consequences

- Behaviour that `PLAN.md` or code comments justify only as "Python behavior, kept as is" is open to change.
- `tests/parity.rs` goes away; Rust tests are the spec.
