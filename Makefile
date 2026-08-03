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

deploy:
	@echo "TODO(Phase 1): cross-compile for aarch64, scp to the box, restart services"
	@exit 1

image-pi3 image-pi4:
	@echo "TODO(Phase 4): Buildroot image build (see buildroot/README.md)"
	@exit 1
