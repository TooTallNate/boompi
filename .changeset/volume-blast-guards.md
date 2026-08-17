---
"boompi": patch
---

Volume can never jump suddenly again. Upward volume changes now ramp
smoothly (about 15% per second - downward stays instant), the speaker
never wakes louder than 70% regardless of what was persisted, and if
a music stream ever escapes the volume-controlled mixing bus it gets
the music volume applied directly instead of playing at full level.
