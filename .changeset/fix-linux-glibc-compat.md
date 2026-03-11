---
"@actionbookdev/cli": patch
---

Fix glibc compatibility for Debian 12 and Ubuntu 22.04 by pinning the linux-x64 build runner to ubuntu-22.04 (glibc 2.35), resolving "GLIBC_2.39 not found" errors on systems with glibc < 2.39
