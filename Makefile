.PHONY: help build build-debug test test-unit test-integration test-all test-smoke fmt clippy lint clean release check-clean serve

help:
	@echo "OpenControl - Development Tasks"
	@echo ""
	@echo "Build:"
	@echo "  make build              - Build release binary"
	@echo "  make build-debug        - Build debug binary"
	@echo ""
	@echo "Test:"
	@echo "  make test               - Run unit tests"
	@echo "  make test-all           - Run all tests (unit + integration)"
	@echo "  make test-unit          - Run unit tests only"
	@echo "  make test-integration   - Run integration tests only"
	@echo "  make test-smoke         - Quick smoke tests (fmt, clippy, tests)"
	@echo ""
	@echo "Code Quality:"
	@echo "  make fmt                - Format code (cargo fmt)"
	@echo "  make clippy             - Lint code (cargo clippy)"
	@echo "  make lint               - Run all checks (fmt + clippy)"
	@echo ""
	@echo "Release:"
	@echo "  make release VERSION=0.2.0 - Create release (requires VERSION)"
	@echo ""
	@echo "Other:"
	@echo "  make clean              - Remove build artifacts"
	@echo "  make check-clean        - Check if git working directory is clean"
	@echo "  make help               - Show this help"

build:
	cargo build --release

build-debug:
	cargo build

test: test-unit

test-all: test-unit test-integration

test-unit:
	cargo test --release --lib

test-integration:
	cargo test --release --test integration_test

test-smoke:
	@echo "Running smoke tests..."
	cargo fmt -- --check
	cargo clippy --release -- -D warnings
	cargo test --release --lib
	@echo "✓ All smoke tests passed"

fmt:
	cargo fmt

clippy:
	cargo clippy --release -- -D warnings

lint: fmt clippy

clean:
	cargo clean

check-clean:
	@git status --porcelain
	@if [ -z "$$(git status --porcelain)" ]; then \
		echo "✓ Working directory is clean"; \
	else \
		echo "✗ Working directory has changes"; \
		exit 1; \
	fi

release: check-clean
	@if [ -z "$(VERSION)" ]; then \
		echo "Error: VERSION is required"; \
		echo "Usage: make release VERSION=0.2.0"; \
		exit 1; \
	fi
	@echo "Creating release v$(VERSION)..."
	@sed -i 's/version = "[^"]*"/version = "$(VERSION)"/' Cargo.toml
	cargo test --release
	cargo build --release
	git add Cargo.toml
	git commit -m "chore: release v$(VERSION)"
	git tag -a v$(VERSION) -m "Release $(VERSION)"
	@echo "✓ Release v$(VERSION) created"
	@echo "Push to origin with: git push origin main && git push origin v$(VERSION)"

.DEFAULT_GOAL := help
