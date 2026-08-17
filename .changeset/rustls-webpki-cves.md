---
"boompi": patch
---

Evicted an end-of-life TLS stack from the daemon. librespot's proxy
support (hyper-proxy2) and the MQTT client (rumqttc) both dragged
rustls 0.22 / rustls-webpki 0.102 into the build - a line that stopped
receiving fixes and had collected four advisories (dependabot 22-25,
including a high-severity CRL parsing panic). MQTT speaks plaintext to
a LAN broker, so its TLS feature is simply gone; hyper-proxy2 is
pinned to the upstream commit that moved to the maintained rustls 0.23
line until a release ships. Every TLS connection the box makes now
goes through one current rustls.
