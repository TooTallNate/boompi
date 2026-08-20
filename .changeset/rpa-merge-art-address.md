---
"boompi": patch
---

Cover art survives BlueZ's mid-connection identity merge. When a phone
first appears under a private rotating address and BlueZ later resolves
it to the real device, the AVRCP player is orphaned under the old path -
and the art fetcher kept dialing that fossil address forever ("Host is
down"), while audio played happily on the real one. The OBEX target is
now resolved at request time: use the live device at the latched path,
or fall back to the connected device when the merge has erased it.
Diagnosed and verified live against a box stuck in exactly this state.
