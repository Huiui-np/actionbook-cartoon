# @actionbookdev/extension

## 0.3.0

### Minor Changes

- [#533](https://github.com/actionbook/actionbook/pull/533) [`e429866`](https://github.com/actionbook/actionbook/commit/e429866115d75475eaafaa91cdfcbaa489d95df2) Thanks [@mcfn](https://github.com/mcfn)! - Release 0.3.0: align extension bridge with Actionbook CLI 1.x.

  - Support CLI 1.x stateless architecture — every message is self-contained with explicit `--session`/`--tab` addressing, no implicit current-tab state.
  - Concurrent multi-tab operation: bridge protocol upgraded to handle parallel CDP traffic across multiple tabs in a single session.
  - Health check on startup to prevent connect/disconnect loops.
