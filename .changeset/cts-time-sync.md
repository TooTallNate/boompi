---
"boompi": minor
---

The box now reads the time off your phone. Phones expose the Bluetooth
Current Time Service as a GATT server - iPhones in particular - so
whenever a connected device's GATT database resolves and offers it,
boompid reads the phone's clock (plus its UTC offset when published)
and steps the system clock through the same guarded path as the web
app's time offers. It is strictly the last resort: never when NTP has
synchronized this boot, never when any client (web, app) recently set
or confirmed the clock, and only from bonded devices - because the
encrypted read can otherwise pop a pairing dialog on the phone
mid-connection. In practice that leaves exactly the case it exists
for: an off-grid box playing music from a paired phone with no
controlling client in sight. Between this, the web
app, and the upcoming iOS app, an RTC-less box now has three ways to
learn the time before it ever finds the internet.
