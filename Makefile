PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
RELEASE_DIR ?= dist
RELEASE_TARGETS ?= x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin

.PHONY: build install release test fmt lint clean

build:
	cargo build --release

install: build
	install -d "$(BINDIR)"
	install -m 0755 target/release/sshup "$(BINDIR)/sshup"

release:
	rm -rf "$(RELEASE_DIR)"
	install -d "$(RELEASE_DIR)"
	@set -e; \
	for target in $(RELEASE_TARGETS); do \
		echo "Building $$target"; \
		rustup target list --installed | grep -qx "$$target" || rustup target add "$$target"; \
		cargo build --release --target "$$target"; \
		archive="$(RELEASE_DIR)/sshup-$$target.tar.gz"; \
		tar -czf "$$archive" -C "target/$$target/release" sshup; \
		echo "Wrote $$archive"; \
	done

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets --all-features -- -D warnings

clean:
	cargo clean
