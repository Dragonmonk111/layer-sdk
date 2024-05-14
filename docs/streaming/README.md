# Data Streaming

There are a number of reasons an external process would like direct access to the internal state and history of the blockchain. Generally, it is to injest the data, map it, and index it to allow more efficient queries for a number of features of interest that are non-trivial to query on a running node.

This is not a mistake of the blockchain architecture, or "missing queries" in a contract. The blockchain should focus on minimal data to enforce correct functionality, and not add extra overhead to serve user-facing queries that are not needed by it's own business logic, or that of other contracts it interacts with.

Given this, a well-designed blockchain should provide easy and efficient means for external processes to injest the data and then do whatever processing they wish. Efficient meaning both high throughput, as well as adding minimal overhead to the running blockchain node (low latency is desirable but a secondary concern).

## Type of streaming data

There are three main kinds of data streaming:

The first is [**state streaming**](./STATE_STREAMING.md). This involves replicating all writes and deletes, so that another process may injest the internal state of the blockchain and then process it and index it. This will provide all key-value pairs stored to the DB, but provides no information aboiut transactions or blocks persay. Want to query balances over multiple tokens and compare to their voting patterns? This will provide you the raw data to index.

The second is [**transaction streaming**](./TX_STREAMING.md), also known as event streaming. This provides the full details of each transaction one after another, so that they may be easily indexed and searched. This includes the transaction itself, which block it was in, the result of the transaction, all events emitted during its execution, as well as possibly the state changes associated with the transaction (like Etherscan). Want to find all trades on a given DEX (perhaps for a historical price chart)? This will provide you the raw data to index.

The third is **block streaming**. In fact, this is the basis of any blockchain, or "replicated state machine". This provides blocks, including all transactions inside them, as well as the metadata, hashes, validator signatures, etc needed to verify their authenticity. This is built into CometBFT peer-to-peer replication and something similar is available in all blockchain networks. I mention it here in case you wish to index this data with something other than a blockchain node, then the existing peer-to-peer gossip network should allow you to stream and sync this information very effectively.
