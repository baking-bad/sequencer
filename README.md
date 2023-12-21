# Distributed sequencer network for smart rollups

This is a proof of concept of a distributed sequencer network that provides ordered transactions with low latency. The ordering is verified in the smart rollup kernel. The PoC consists of:
- Sequencer node: provides public API and also posts transactions to the rollup inbox;
- Consensus node: provides total ordering with a chain of certificates produced by authorities and exposes internal API for posting transactions and subscribing to the consensus output;
- Smart rollup node: validates transaction ordering and applies them to the state.



