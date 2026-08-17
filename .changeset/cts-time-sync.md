---
"boompi": minor
---

The box now reads the time off your phone. Phones expose the Bluetooth
Current Time Service as a GATT server - iPhones in particular - so
whenever a connected device's GATT database resolves and offers it,
boompid reads the phone's clock (plus its UTC offset when published)
and steps the system clock through the same guarded path as the web
app's time offers: only when NTP has never synchronized this boot,
never against a working internet connection. Between this, the web
app, and the upcoming iOS app, an RTC-less box now has three ways to
learn the time before it ever finds the internet.
