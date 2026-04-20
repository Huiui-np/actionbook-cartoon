---
"@actionbookdev/cli": patch
---

- Fix duplicate short tab IDs after drag-out-drag-in in extension mode (ACT-986): `push_tab` skips already-occupied candidates; `push_tab_with_id` advances auto-counter past t{n}-style custom IDs
- Fix `browser new-tab` not awaiting navigation commit before returning
- Fix plain-text error output routed to stderr instead of stdout
- Fix case-insensitive internal-scheme detection
- Fix network-idle check incorrectly blocking on off-screen lazy images (ACT-964)
- Fix wait stability with dual-threshold for already-loaded baseline URL (ACT-938-a)
- Revert `--auto-connect` flag (feature rollback)
