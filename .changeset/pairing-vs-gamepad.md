---
"boompi": patch
---

Pairing a phone no longer sabotages an in-progress game. Opening the
pairing window used to disconnect every Bluetooth device including
the gamepad (killing inputs mid-game - pads don't reconnect after a
host-side drop), and the still-broadcasting pad would then be
"re-discovered" by gamepad autopair, which promptly closed the
pairing window before the phone could get in. Gamepads now stay
connected through pairing mode, and already-paired pads never close
the window.
