---
"@actionbookdev/cli": patch
---

Fix duplicate short tab IDs after drag-out-drag-in in extension mode (ACT-986). `push_tab` now skips any candidate already in use, and `push_tab_with_id` advances the auto-counter past any t{n}-style custom ID to prevent future collisions.
