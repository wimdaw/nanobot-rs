.PHONY: build release run check fmt clean test

build:
	cargo build

release:
	cargo build --release

check:
	cargo check

test:
	cargo test

fmt:
	cargo fmt

sync-check:
	cargo run -- sync check

sync-pull:
	cargo run -- sync pull-assets

clean:
	cargo clean
