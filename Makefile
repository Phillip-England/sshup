PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin

.PHONY: build install test fmt lint clean

build:
	cargo build --release

install: build
	install -d "$(BINDIR)"
	install -m 0755 target/release/shhup "$(BINDIR)/shhup"

test:
	cargo test

fmt:
	cargo fmt

lint:
	cargo clippy --all-targets --all-features -- -D warnings

clean:
	cargo clean
