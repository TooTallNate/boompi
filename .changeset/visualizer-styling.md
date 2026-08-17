---
"boompi": patch
---

The spectrum bars now render in the album art's secondary palette
color (the sliders keep the primary), so they no longer drown out the
volume and playback tracks - and their opacity is adjustable in the
web settings. The bars are also truly volume-independent now: music
mixes through a dedicated pre-volume bus that the visualizer taps
directly, instead of mathematically undoing the volume from the
attenuated signal (which fell apart at low volumes).
