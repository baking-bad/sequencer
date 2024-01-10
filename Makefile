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
		--preimages-dir $(BIN_DIR)/wasm_2_0_0 \
		--setup-file $(BIN_DIR)/kernel_config.yaml

build-operator:
	mkdir $(BIN_DIR) || true
	$(MAKE) build-kernel
	$(MAKE) build-installer

build-sequencer:
	cargo build --bin sequencer

build-narwhal:
	cargo build --bin narwhal-node
	cp ./target/debug/narwhal-node ./bin/

image-operator:
	docker build -t dsn/operator:$(OCTEZ_TAG) --file ./docker/operator/local.dockerfile \
		--build-arg OCTEZ_TAG=$(OCTEZ_TAG) \
		--build-arg OCTEZ_PROTO=$(OCTEZ_PROTO) \
		.

image-narwhal:
	docker build -t dsn/narwhal:latest --file ./docker/sequencer/local.dockerfile .

run-operator:
	$(MAKE) build-operator
	$(MAKE) image-operator OCTEZ_TAG=$(OCTEZ_TAG) OCTEZ_PROTO=$(OCTEZ_PROTO)
	docker stop dsn-operator || true
	docker volume rm dsn-operator
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
	RUST_LOG=info ./target/debug/sequencer

run-dsn:
	rm -rf ./db
	./target/debug/launcher --id 1 --log-level 2 &
	./target/debug/launcher --id 2 --log-level 0 &
	./target/debug/launcher --id 3 --log-level 0 &
	./target/debug/launcher --id 4 --log-level 0 &
	./target/debug/launcher --id 5 --log-level 0 &
	./target/debug/launcher --id 6 --log-level 0 &
	./target/debug/launcher --id 7 --log-level 0 &

kill-dsn:
	killall narwhal-node

start-dsn-min:
	$(MAKE) build-narwhal
	$(MAKE) image-narwhal
	cd docker/setup/dsn-min-4-1 && docker-compose up -d

stop-dsn-min:
	cd docker/setup/dsn-min-4-1 && docker-compose down -v

broadcast:
	curl -d '{"data":"deadbeef"}' -H "Content-Type: application/json" -X POST http://localhost:8080/broadcast

run-listener:
	cargo build --bin simple-listener
	RUST_LOG=info ./target/debug/simple-listener --endpoint $(ENDPOINT) --from-id $(FROM_ID)

run-spammer:
	cargo build --bin simple-spammer
	RUST_LOG=info ./target/debug/simple-spammer --endpoint $(ENDPOINT) --sleep $(SLEEP)