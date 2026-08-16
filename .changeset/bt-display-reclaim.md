---
"boompi": patch
---

Switching away from Bluetooth (to AirPlay or Spotify) and back no
longer strands the session in limbo: the panel showed track titles
but no source in the footer, and freshly fetched cover art was thrown
away. Bluetooth now reclaims the display the moment it publishes
while no other source holds it, and a failed cover-art fetch retries
after ten seconds instead of giving up on that track forever.
