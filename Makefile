# SPDX-FileCopyrightText: 2023 Baking Bad <hello@bakingbad.dev>
#
# SPDX-License-Identifier: MIT

.PHONY: test

-include nairobi.env

BIN_DIR:=$$PWD/bin
TARGET_DIR=$$PWD/target
CARGO_BIN_PATH:=$$HOME/.cargo/bin

install:
	cargo install tezos-smart-rollup-installer
	cd $(CARGO_BIN_PATH) \
		&& wget -c https://github.com/WebAssembly/binaryen/releases/download/version_111/binaryen-version_111-x86_64-linux.tar.gz -O - | tar -xzv binaryen-version_111/bin/wasm-opt --strip-components 2 \
		&& wget -c https://github.com/WebAssembly/wabt/releases/download/1.0.31/wabt-1.0.31-ubuntu.tar.gz -O - | tar -xzv wabt-1.0.31/bin/wasm-strip wabt-1.0.31/bin/wasm2wat --strip-components 2

build-kernel:
	RUSTC_BOOTSTRAP=1 cargo build --package dsn_kernel \
		--target wasm32-unknown-unknown \
		--release \
		-Z sparse-registry \
		-Z avoid-dev-deps
	wasm-strip -o $(BIN_DIR)/kernel.wasm $(TARGET_DIR)/wasm32-unknown-unknown/release/dsn_kernel.wasm

build-installer:
	smart-rollup-installer get-reveal-installer \
		--upgrade-to $(BIN_DIR)/kernel.wasm \
		--output $(BIN_DIR)/kernel_installer.wasm \
		--preimages-dir $(BIN_DIR)/wasm_2_0_0

build-operator:
	mkdir $(BIN_DIR) || true
	$(MAKE) build-kernel
	$(MAKE) build-installer

build-sequencer:
	cargo build --package sequencer

image-operator:
	docker build -t dsn/operator:$(OCTEZ_TAG) --file ./docker/kernel/local.dockerfile \
		--build-arg OCTEZ_TAG=$(OCTEZ_TAG) \
		--build-arg OCTEZ_PROTO=$(OCTEZ_PROTO) \
		.

run-operator:
	$(MAKE) build-operator
	$(MAKE) image-operator OCTEZ_TAG=$(OCTEZ_TAG) OCTEZ_PROTO=$(OCTEZ_PROTO)
	docker stop dsn-operator || true
	docker run --rm -it \
		--name dsn-operator \
		--entrypoint=/bin/sh \
		-v $$PWD/.tezos-client:/root/.tezos-client/ \
		-v dsn-operator:/root/.tezos-smart-rollup-node \
		-v $(BIN_DIR):/root/bin -p 127.0.0.1:8932:8932 \
		-e NETWORK=$(NETWORK) \
		dsn/operator:$(OCTEZ_TAG)

run-sequencer:
	$(MAKE) build-sequencer
	RUST_LOG=debug ./target/debug/sequencer
