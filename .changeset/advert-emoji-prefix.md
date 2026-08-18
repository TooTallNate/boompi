---
"boompi": patch
---

The control channel's Bluetooth name is now "🎛️ <speaker name>"
instead of "Boompi Remote - <speaker name>" - the emoji prefix costs 8
of the advert's hard 29 bytes instead of 16, more than doubling the
space for the name you chose. To guarantee the advert always fits,
speaker names are now capped at 21 UTF-8 bytes (server-enforced,
emoji-safe), and every name field - web settings, setup wizard, iOS -
shows a live bytes-used counter while you type.
