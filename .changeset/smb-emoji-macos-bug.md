---
"boompi": patch
---

The games share is now actually openable from the Finder sidebar. Two
bugs conspired: macOS's SMB client fails session setup ("server
rejected the authentication") whenever the advertised DNS-SD instance
name contains any character outside the Basic Multilingual Plane -
emoji, in practice - so "George's 🔊" could be seen but never opened;
meanwhile smbd was quietly registering its own duplicate advert under
the machine hostname (BOOMPI-XXXX), which is the entry that *did*
work and hid the breakage. The advert now strips non-BMP characters
(curly quotes and BMP symbols like ♪ survive; the bug boundary was
confirmed experimentally against Apple's client), and smbd's
duplicate registration is disabled - Finder shows exactly one entry,
named after the speaker, that connects.
