---
"boompi": patch
---

Bluetooth volume now works like AirPlay and Spotify Connect: the
phone sends full-quality audio and volume commands, and the speaker
renders the volume. iOS's previous behavior - scaling the audio on
the phone before sending it - turned out to be a reaction to
PipeWire's participation in the Bluetooth volume handshake, not an
Apple constant (verified against the v1 image, where the same phone
behaved correctly). One PipeWire setting restores spec behavior, the
iPhone-specific volume mode is no longer auto-assigned, and the
speaker's volume slider is authoritative for every source.
