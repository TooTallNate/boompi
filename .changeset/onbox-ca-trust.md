---
"boompi": patch
---

HTTPS works from the box's command line now. The image never shipped
a CA trust store, so every on-box HTTPS client except the updater
(which carries its own compiled-in roots) failed instantly with a
trust-anchor error - curl, wget, anything a bench session shells out
to. During a wifi debugging session this masqueraded as the network
dropping TCP data mid-flow and cost an hour of chasing phantom router
filtering. The Mozilla CA bundle is now installed, and the build
asserts it actually landed in the image.
