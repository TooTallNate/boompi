---
"boompi": patch
---

The version in the connection greeting (shown as "Software" in the
apps' About screens) now reports the real OS image version instead of
a fossilized "2.0.0-dev". Changesets bump the image version while the
Rust workspace keeps a placeholder, and the greeting was reading the
wrong one - the Software Update page was already correct, since it
read the on-disk image stamp. One source of truth now: everything
reads /etc/boompi-version.
