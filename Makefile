MANIFEST := rust/Cargo.toml
# Backend the UI connects to; override for a real box:
#   make ui BACKEND=ws://boombox.local:3001/ws
BACKEND ?= ws://127.0.0.1:3001/ws

.PHONY: check test fmt clippy sim ui deploy image-pi3 image-pi4

check:
	cargo check --manifest-path $(MANIFEST) --workspace

test:
	cargo test --manifest-path $(MANIFEST) --workspace

fmt:
	cargo fmt --manifest-path $(MANIFEST) --all

clippy:
	cargo clippy --manifest-path $(MANIFEST) --workspace -- -D warnings

# Run the backend with simulated sources (no hardware needed).
sim:
	cargo run --manifest-path $(MANIFEST) -p boompid -- --sim

# Run the UI locally. Pair with `make sim` in another terminal, or point
# BACKEND at a real boombox.
ui:
	cargo run --manifest-path $(MANIFEST) -p boompi-ui -- --backend $(BACKEND)

# ---- cross-compilation for the Pi (see scripts/cross-build.sh) ------------
# One-time: brew install zig cargo-zigbuild && rustup target add aarch64-unknown-linux-gnu
PI ?= pi@boompi-dev.local
SYSROOT ?= $(HOME)/boompi-sysroot

# Pull headers + libs from the running Pi (the C deps Slint links against).
sysroot:
	mkdir -p $(SYSROOT)/usr/lib $(SYSROOT)/usr/share
	rsync -a $(PI):/usr/include $(SYSROOT)/usr/
	rsync -a $(PI):/usr/lib/aarch64-linux-gnu $(SYSROOT)/usr/lib/
	rsync -a $(PI):/usr/share/pkgconfig $(SYSROOT)/usr/share/

cross-kms-test:
	scripts/cross-build.sh kms-test --no-default-features --features kms

cross-kms-test-gl:
	scripts/cross-build.sh kms-test --no-default-features --features kms-gl

cross-boompid:
	scripts/cross-build.sh boompid

# Skia renderer: GPU (GLES/EGL) + color emoji. Needs mesa on the box.
cross-ui:
	scripts/cross-build.sh boompi-ui --no-default-features --features kms-skia

# Software-renderer fallback variant (no GL, monochrome emoji only).
cross-ui-soft:
	scripts/cross-build.sh boompi-ui --no-default-features --features kms

deploy:
	@echo "TODO(Phase 1): cross-build boompid+ui, scp to the box, restart services"
	@exit 1

image-pi3 image-pi4:
	@echo "TODO(Phase 4): Buildroot image build (see buildroot/README.md)"
	@exit 1
