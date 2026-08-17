---
"boompi": patch
---

Bluetooth senders now always deliver full-quality, full-scale audio.
The absolute-volume negotiation is disabled at the Bluetooth stack
level (a carried bluez patch adds a config option for it): iPhones
were freezing their transmitted audio at a stale low volume when the
half-disabled handshake left nobody completing the notification loop
- the "max volume but very quiet" bug. Phones now treat the box like
any classic speaker (their volume slider is a local gain on their
side), and the speaker's own two-track mixer is the one true volume.
Re-pair phones after updating - they cache the old capabilities.
