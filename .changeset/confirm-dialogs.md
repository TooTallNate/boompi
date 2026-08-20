---
"boompi": patch
---

Native browser confirm() popups are gone from the web UI. Restart,
device unpair, ROM delete, and the hardware profile apply/lock flows
now use a proper styled confirmation dialog (shadcn AlertDialog via a
shared ConfirmButton) - keyboard-dismissable, themed, and immune to
webviews that silently swallow window.confirm.
