# Etherlink consensus node

A component of the distributed sequencer project, responsible for total (and fair) ordering of transactions.  
Based on the [Sui](https://github.com/MystenLabs/sui) codebase.

## Big picture

A consensus node is an accompanying service for the EVM node and they should be treated as one (long term they might be merged into a single binary for better performance). There is a bi-directional communication between two nodes.

![image](https://github.com/baking-bad/sequencer/assets/44951260/7675a8a0-687e-46f9-8f1d-9fda0ce8e991)

Consensus node uses EVM node for:
- Retrieving batches of pending transactions (submitted by users and pre-validated by EVM node)
- Pre-applying blueprints of blocks (filtering out invalid transactions and delayed inbox messages)

Consensus node also uses rollup node API for submitting DA batches to rollup inbox.

EVM node in its turn uses Consensus node for:
- Retrieving the recent totally ordered block blueprints

Consensus node is using a modified version of Sui Lutris — the distributed system protocol that powers Sui blockchain. It utilizes the Narwal/Bullshark consensus protocol at its core for achieving the total ordering of transactions.

## Key notes

A few key points worth noting, even if they might be obvious:
* Consensus node is dealing with opaque messages (aka extrinsics) meaning it does not know what it’s inside — EVM transaction, delayed inbox message, or something else.
* Consensus and EVM nodes mutually trust each other — as we stated earlier they are essentially two parts of a single entity.
* Block blueprints do not necessarily become L2 blocks — they can be refused for multiple reasons, thus Consensus node also does not know what “block height” is.
* Consensus node keeps the history sufficient for reconstructing the DAG (to obtain the same view over the transaction flow) and retrieving the latest ordered blueprints that haven’t made it to the inbox yet.

## Roadmap

### Step 0: Run Narwal benchmarks

Narwal benchmarks are outdated and need some fixes + adaptation to the reorganized repo. 
We need to be able to setup a local cluster and run benchmarks to get rough numbers for latency.  

Related: https://github.com/MystenLabs/sui/pull/15029

### Step 1: Adopt Sui codebase

Sui codebase is huge and has an enormous dependency tree. 
We don't need most of it, and there's also a way to completely remove Sui dependency from the Narwal implementation.  
That will also include removing things like ProtocolConfig, SuiTransaction, SuiKeyPair, etc.  

In the result we should get a relatively small codebase, decoupled from the Sui protocol.  
It will have an adequate build time and overall be more comprehensive for developers.

### Step 2: Create Narwal playground

In order to get a deep understanding of what every module does and how consensus protocol works we will create a verbose runner for tracing transactions.

### Step 3: RPC API for EVM Node

There is already an interface for broadcasting transactions and subscribing to the consensus output. 
We need to update them accordingly, derive the necessary specification, and coordinate with the EVM node team.

Ideally at this stage we should get a PoC of a distributed sequencer without validation / checkpoints / DA / governance.

### Step 4: Committee, leader election, epochs

### Step 5. Consensus output filtering

### Step 6. Checkpoints

### Step 7. DA integration

### Step 8. Full integration with EVM node

## Implementation details

### Pre-validation
Narwal provides a neat abstraction for the transaction validation which is a trait with two methods: for a single transaction validation and for batch validation. The first one is used upon a new transaction is submitted by user, the second one is when worker receives a batch of transactions from another worker.

### Networking
### Peer discovery
### Broadcasting
### RPC API
### Executor
### Randomness
### Epoch
### Reputation
### Checkpoints
### Aggregated BLS signatures
### Committee config

