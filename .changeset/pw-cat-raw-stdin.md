---
"boompi": patch
---

Fix silent Spotify and AirPlay playback after the Buildroot 2026.08
upgrade. PipeWire 1.6 requires `pw-cat --raw` for headerless PCM on
stdin; the visualizer capture uses raw mode too.
