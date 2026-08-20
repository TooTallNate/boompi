---
"boompi": patch
---

Update-check failures explain themselves now. "error sending request
for url (...)" told you nothing - was it DNS, TLS, a timeout, a rate
limit? Errors shown in the settings UIs now carry the HTTP status
when a response arrived ("HTTP 502 from ...") and the full cause
chain when one didn't ("request to ... failed: ... dns error: failed
to lookup address"), so a failed check reads like a diagnosis instead
of a shrug.
