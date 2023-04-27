use crate::Tx;
use cosmwasm_std::Timestamp;

/*
    pub struct RequestFinalizeBlock {
    #[prost(bytes = "bytes", repeated, tag = "1")]
    pub txs: ::prost::alloc::vec::Vec<::prost::bytes::Bytes>,
    #[prost(message, optional, tag = "2")]
    pub decided_last_commit: ::core::option::Option<CommitInfo>,
    #[prost(message, repeated, tag = "3")]
    pub misbehavior: ::prost::alloc::vec::Vec<Misbehavior>,
    /// hash is the merkle root hash of the fields of the decided block.
    #[prost(bytes = "bytes", tag = "4")]
    pub hash: ::prost::bytes::Bytes,
    #[prost(int64, tag = "5")]
    pub height: i64,
    #[prost(message, optional, tag = "6")]
    pub time: ::core::option::Option<crate::google::protobuf::Timestamp>,
    #[prost(bytes = "bytes", tag = "7")]
    pub next_validators_hash: ::prost::bytes::Bytes,
    /// proposer_address is the address of the public key of the original proposer of the block.
    #[prost(bytes = "bytes", tag = "8")]
    pub proposer_address: ::prost::bytes::Bytes,
}
     */

// See https://github.com/informalsystems/tendermint-rs/blob/mikhail/cometbft-0.38/proto/src/prost/v0_38/tendermint.abci.rs#L220-L239
pub struct Block {
    pub txs: Vec<Tx>,
    pub height: u64,
    pub time: Timestamp,

    /// proposer_address is the address of the public key of the original proposer of the block.
    pub proposer_address: Vec<u8>,
    /// votes contains all validators who voted for the last block to make consensus
    pub last_votes: Vec<Validator>,
}

pub struct Validator {
    /// The first 20 bytes of SHA256(public key)
    pub address: Vec<u8>,
    /// The voting power
    pub power: i64,
}

pub struct ValidatorUpdate {
    pub pub_key: TmPubKey,
    /// The voting power
    pub power: i64,
}

/// Possible public keys of validator nodes
pub enum TmPubKey {
    Ed25519(Vec<u8>),
    Secp2556k1(Vec<u8>),
}

impl TmPubKey {
    /// The first 20 bytes of SHA256(public key)
    /// TODO: is this raw pubkey or do we need type info there serialized somehow???
    pub fn address(&self) -> Vec<u8> {
        todo!()
    }
}
