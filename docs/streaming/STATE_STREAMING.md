# State Streaming

State Streaming provides an event stream of the state changes on the blockchain node, grouped by blocks as they are committed. It allows a data sink to mirror the state of a running node, and perform any desired transformation on the data to provide new APIs
and query services to off-chain clients.

## Goals

* Provide an efficient way to copy the current state
* Provide an efficient way to stream all state changes as the blocks are committed
* Provide significantly higher throughput than the blockchain writes, so consumers don't fall out of sync 
* A consumer should be able to lose connection (eg crash and restart) and resume streaming
from their last location.
* Crash or slowdown of consumer shouldn't impact the blockchain node (unlike unix named pipes)
* Allow the consumer to live on a different machine (network protocol)

## Architecture

While this may seem revolutionary to blockchain developers, similar streaming architectures are nothing new. PostgreSQL, for example, has had [logical replication](https://www.postgresql.org/docs/current/logical-replication.html) for over a decade, with concepts of [replication slots](https://www.postgresql.org/docs/current/warm-standby.html#STREAMING-REPLICATION-SLOTS) tracking the state of an individual consumer, ensuring no data ever gets dropped. (It also has stuff like [row filtering](https://www.postgresql.org/docs/current/logical-replication-row-filter.html)), so only data of interest is replicated, which may be use exploring later.

Rather than build something from scratch, it would make sense to try to reuse (or repurpose) existing solutions that are compatible with our tech stack. However, in blockchain implementations, we use embedded databases, like RocksDB, rather than PostgreSQL for performance and simpler operation. PostgreSQL is great for the state consumers who wish to provide end-user queries, not for rapdily executing smart contracts. Looking for similar concepts in RocksDB, I discovered [Rocksplicator](https://medium.com/pinterest-engineering/open-sourcing-rocksplicator-a-real-time-rocksdb-data-replicator-558cd3847a9d), which does such streaming replication between Master-Slave RocksDB instances... or at least it did until it was archived two years ago.

However, the concepts it was built on are solid and it shows that RocksDB offers access to its WAL in a similar (but simpler) version of PostgreSQL's logical replication. We can use the WAL as a shared replication slot, with some auto-cleanup, which hopefully is set to a relatively large window.

There are two key methods here: [`latest_sequence_number`](https://docs.rs/rocksdb/latest/rocksdb/struct.DBCommon.html#method.latest_sequence_number) and [`get_updates_since`](https://docs.rs/rocksdb/latest/rocksdb/struct.DBCommon.html#method.get_updates_since). You can find the last sequence (monotonically increasing, similar but different than block height), and given the last sequence you have seen, you can get a stream of all state updates (writes and deletes) since then, grouped in [write batches](https://docs.rs/rocksdb/latest/rocksdb/struct.DBWALIterator.html#associatedtype.Item), one per sequence. This provides enough functionality to implement state streaming.

There is also another useful function [`DBCommon::iterate`](https://docs.rs/rocksdb/latest/rocksdb/struct.DBCommon.html#method.iterator), which allows us to iterate over all data. Amazingly enough, it even [provides a consistent snapshot](https://github.com/EighteenZi/rocksdb_wiki/blob/master/Iterator.md), so that all data is taken at the same sequence number.

## Correct Syncing

Rather than try to perform some "exactly once delivery", let's look at the minimum consistency requirements we need from our stream in order to ensure that the consumer will end up with a correct copy of the blockchain state. The simpler the requirements, the easier to implement.

> If we have the state at A and apply all changes between A and B (aka apply a stream), we will have the correct state at B.

Once we have some blockchain state, we only need a stream to maintain a copy of the state.

> If we sync to B, then re-apply the stream from A to B (not missing any steps), we will have the correct state at B.

That is, as long as the streams go forward and don't miss any items, we can stop one stream and restart at an earlier point, and it will be correct once it syncs up to the highest sequence we were at. This also means we need "at least once" delivery and "ordered delivery". When in doubt, replaying from earlier is safe.

> If the database's keys are from different heights, between A and B, and we play the stream from A to B, then the database will return to the correct state at B

This may be less intuitive, but it follows from the above, if we consider every key individually, as a database unto itself. This will be very valuable when we try to copy current state from a running chain, so we don't need to re-sync from genesis every time. Even if some process died mid-stream, we can just replay from the last state we were sequence was synced.

### Minimal Algorithm

We expose three methods to consumers:

* GetLatestSequence()
* StreamCurrentState()
* StreamChangesSince(sequence)

To get a fresh copy of the database at current height:

* GetLatestSequence() -> A
* StreamCurrentState() -> Store locally
  * Stream is broken? Start over
* StreamChangesSince(A) -> Store locally

If you had synced data and your process crashes and restarts:

* load most recent sequence you are sure you committed -> B
* StreamChangesSince(B) -> Store locally

## API

Given the above methods in RocksDB and the relatively simple algorithm it gives rise to, we could see how we could run this in-process. The question becomes how to expose it to another process securely, efficiently, and with minimal hurdles for the developers.

gRPC uses an efficient and well-known encoding format (Protobuf), has tooling in many languages to convert `.proto` files in client APIs or server stubs, and supports [server-streaming](https://grpc.io/docs/what-is-grpc/core-concepts/#server-streaming-rpc), which is the process we want. The consumer should make a call and get a stream of data back.

There are numerous [examples of Go implementations](https://github.com/pramonow/go-grpc-server-streaming-example), and we can provide client bindings as a library, which should lower the barrier for integration with other developers.
