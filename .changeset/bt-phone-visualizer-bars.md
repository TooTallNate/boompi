---
"boompi": patch
---

The spectrum bars no longer flatline when an iPhone plays over
Bluetooth at moderate volume. iOS scales its own PCM (the box's sink
stays at reference), so the captured samples carried the phone's
steep volume taper straight into the display while AirPlay and
Spotify bars kept dancing. The visualizer now undoes the phone-side
attenuation and re-applies the same linear volume every other source
gets - consistent bars at the same loudness, whatever the source.
