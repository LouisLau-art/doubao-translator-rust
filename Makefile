BINARY_NAME := translator

.PHONY: dev

dev:
	cargo run

.PHONY: build
build:
	cargo build

.PHONY: build-prod
build-prod:
	cargo build --release

.PHONY: serve
serve: build-prod
	./target/release/$(BINARY_NAME)

.PHONY: fmt
fmt:
	cargo fmt

.PHONY: lint
lint:
	cargo clippy -- -D warnings

.PHONY: test
test:
	cargo test

.PHONY: install-service
install-service: build-prod
	@echo "Installing systemd service (requires sudo)..."
	bash scripts/install-service.sh

.PHONY: uninstall-service
uninstall-service:
	@echo "Removing systemd service (requires sudo)..."
	bash scripts/uninstall-service.sh
