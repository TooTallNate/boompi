---
"boompi": patch
---

Home Assistant now correctly offers edge builds as updates: HA
compares versions with semver, where the edge stamp's "-sha" suffix
means prerelease - so an edge build ranked OLDER than its base
release and HA showed "Up-to-date" despite listing a newer version.
Suffixed stamps are now presented to HA in a non-semver shape
("v2.1.0 (f06b1b6)"), falling back to plain string comparison;
stable tags keep real semver ordering.
