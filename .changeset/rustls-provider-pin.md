---
"boompi": patch
---

Spotify Connect works again. The TLS-stack cleanup two releases back
left both of rustls's crypto providers (ring and aws-lc-rs) in the
dependency graph, and rustls panics at the first TLS connection when
it can't auto-pick one - which killed librespot's session task at
startup on every box. boompid now installs ring as the process
provider explicitly, so no dependency-graph drift can ever make TLS
ambiguous again.
