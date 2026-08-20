---
"boompi": patch
---

Experimental kernel patch: the Bluetooth dongle's extended advertising
is now bypassed entirely. Its firmware claims LE Extended Advertising
support but delivers spec-violating termination events, EBUSY races,
and broadcasts that silently die - the root soil of every advertising
workaround this month. A new kernel quirk (candidate for upstream
submission, sibling of the extended-scan quirk mainline already
applied to this chip) drops the host back to legacy advertising, the
decade-hardened path where BlueZ multiplexes instances in software.
If the field results hold, the parking/re-assert machinery becomes
belt-and-suspenders instead of load-bearing.
