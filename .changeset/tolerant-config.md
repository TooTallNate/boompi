---
"boompi": patch
---

Unknown keys in the persisted config are now warned about and ignored
instead of failing the parse - previously a leftover key from a
withdrawn feature (or a config written by a newer build) could keep
the daemon from starting at boot.
