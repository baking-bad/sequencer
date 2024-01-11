# Distributed sequencer network for Tezos smart rollups

This is a proof of concept of a distributed sequencer network that provides ordered transactions with low latency. The ordering is verified in the smart rollup kernel. The PoC consists of:
- Sequencer node: provides public API and also posts transactions to the rollup inbox;
- Consensus node: provides total ordering with a chain of certificates produced by authorities and exposes internal API for posting transactions and subscribing to the consensus output;
- Smart rollup node: validates transaction ordering and applies them to the state.

The consensus part is based on the Narwhal codebase from [Sui](https://github.com/mystenLabs/sui) repository. Several notable changes made:
1. Narwhal part is completely decoupled from Sui crates to make the overall dependency tree smaller;
2. Instead of calling an external executor, consensus node stores pre-blocks (sub DAGs) and serves them via streaming gRPC endpoint;
3. Both primary and worker run as a single node, for simplicity;
4. No transaction validation, as we assume a closed network;

![image](https://github.com/baking-bad/sequencer/assets/44951260/7f7604c9-af1b-4c57-8daa-c2d330979b7f)

## Installation

Install Rust toolchain:
```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Add Wasm32 target:
```
rustup target add wasm32-unknown-unknown
```

Install kernel build dependencies:
```
make install
```

Install `protobuf` and `clang` system packages:
```
sudo apt install protobuf-compiler clang
```

## How to run

### Rollup operator

Start with:

```
make run-operator
```

You will end up inside the docker container shell.
Every time you call this target, kernel and docker image will be rebuilt.
Also, existing docker volume and running container will be removed.

#### Generate new keys

For convenience, your local .tezos-client folder is mapped into the container in order to preserve the keys. Upon the first launch you need to create new keypair, in order to do that inside the operator shell:

```
$ operator generate_key
```

#### Check account info

If you already have a key, check it's balance: it should be at least 10k tez to operate a rollup, otherwise top up the balance from the faucet. To get your account address:

```
$ operator account_info
```

#### Originate rollup

```
$ operator deploy_rollup
```

Rollup data is persisted meaning that you can restart the container without data loss. If you try to call this command again it will tell you that there's an existing rollup configuration. Use `--force` flag to remove all data and originate a new one.

#### Run rollup node

```
$ operator run_node
```

Runs rollup node in synchronous mode, with logs being printed to stdout.  
Also RPC is available at `127.0.0.1:8932` on your host machine.

### Local DSN cluster

In order to run 7 consensus nodes on a local machine:
```
make run-dsn
```

Not that the output would be captured. In order to stop them all, type in another terminal:
```
make kill-dsn
```

The pre-block streaming API will available at:
- `http://127.0.0.1:64011`
- `http://127.0.0.1:64021`
- `http://127.0.0.1:64031`
- `http://127.0.0.1:64041`
- `http://127.0.0.1:64051`
- `http://127.0.0.1:64061`
- `http://127.0.0.1:64071`

The transaction server API will available at:
- `http://127.0.0.1:64012`
- `http://127.0.0.1:64022`
- `http://127.0.0.1:64032`
- `http://127.0.0.1:64042`
- `http://127.0.0.1:64052`
- `http://127.0.0.1:64062`
- `http://127.0.0.1:64072`

### Consensus node


### Sequencer

Once you have both consensus and rollup nodes running, you can launch sequencer node to test the entire setup:

```
make run-sequencer
```

#### Mocked rollup

It is possible to mock rollup node and do local pre-block verification instead:

```
make build-sequencer
./target/debug/sequencer --mock-rollup
```

#### Mocked consensus

Similarly, you can make sequencer generate pre-blocks instead of connecting to a DSN

```
make build-sequencer
./target/debug/sequencer --mock-consensus
```

### DSN listener

You can also subscribe to a remote DSN and listen for incoming pre-blocks:

```
make run-listener ENDPOINT=http://127.0.0.0:64001 FROM_ID=0
```

### DSN spammer

In order to generate a transaction stream to test latency run:

```
make run-spammer ENDPOINT=http://127.0.0.0:64003 SLEEP=10
```

Every `SLEEP` milliseconds it will connect to remote DSN node and send a transaction of random size + timestamp in the beginning. If you also run a listener you will see stat messages for incoming pre-blocks.