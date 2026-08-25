---
"boompi": patch
---

Boxes now identify themselves consistently across every discovery
path: the BLE advert carries the stable box id as manufacturer data
(alongside the existing name), matching the mDNS TXT `id`, and
`Hello` gained an `id` field. The iOS app uses the join to show one
row per physical speaker instead of one per transport - a box seen
over both Bluetooth and Wi-Fi collapses into the Wi-Fi row - and to
auto-connect the remembered speaker over the best visible pipe, so
a box last used over Bluetooth upgrades to Wi-Fi the moment its
network advert appears. Boxes too old to advertise an id keep the
previous one-row-per-pipe behavior.
