---
"@actionbookdev/cli": patch
---

Fix CWS extension ID mismatch and browser close bridge lifecycle:

- Support Chrome Web Store extension ID alongside dev extension ID for origin validation and native messaging
- Remove misleading port change suggestion from bridge conflict error message
- `browser close --extension` now fully cleans up bridge lifecycle: best-effort tab detach → stop bridge process → delete all state files (PID, port, token)
