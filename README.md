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