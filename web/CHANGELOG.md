# boompi

## 2.1.0

### Minor Changes

- 54c0de6: Built-in software updates. The speaker checks GitHub Releases for new
  OS versions (a "bleeding edge" toggle follows every green dev build
  instead of tagged releases), shows the running version in both settings
  UIs, and installs updates itself: assets stream straight into the
  inactive A/B slot, are sha256-verified, and boot through the usual
  fail-safe trial. Boot is also quiet now - no more console text on the
  panel during startup.
