---
"boompi": patch
---

N64 games work now. The emulator core was being cross-compiled with a
split personality: the dynamic recompiler's sources were built for
the Pi's processor, but architecture-conditional build steps
(including the generated structure offsets the recompiler reads at
runtime) defaulted to the build server's x86 - so the JIT crashed on
its very first translated block, on every game. The build now follows
Batocera's proven Raspberry Pi recipe with the architecture pinned
explicitly, and RetroArch shares its GL context with hardware-
rendered cores (which N64 also needed). Verified on hardware: Super
Mario 64 and Ocarina of Time running with the dynamic recompiler -
no interpreter fallback.
