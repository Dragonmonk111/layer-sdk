use std::ops::Deref;
// TODO: make our own custom pulsar-storage package to extend (esp with file system backing, transactions...)
use crate::error::PulsarError;
use cosmwasm_std::Storage;
use parking_lot::RwLock;
use pulsar_std::{Block, Query, Tx};

use crate::sm::StateMachine;

/// This maintains all application global state and is a framework-agnostic entrypoint for the
/// application. It *should* be able to run inside an ABCI app as well as an Avalache Subnet.
#[allow(dead_code)]
pub struct App {
    // State
    storage: RwLock<Box<dyn Storage>>,

    // State Machine Logic
    logic: StateMachine,
}

impl App {
    pub fn new(storage: impl Storage + 'static, logic: StateMachine) -> App {
        App {
            storage: RwLock::new(Box::new(storage)),
            logic,
        }
    }

    /*
        pub struct RequestInitChain {
        #[prost(message, optional, tag = "1")]
        pub time: ::core::option::Option<crate::google::protobuf::Timestamp>,
        #[prost(string, tag = "2")]
        pub chain_id: ::prost::alloc::string::String,
        #[prost(message, optional, tag = "3")]
        pub consensus_params: ::core::option::Option<super::types::ConsensusParams>,
        #[prost(message, repeated, tag = "4")]
        pub validators: ::prost::alloc::vec::Vec<ValidatorUpdate>,
        #[prost(bytes = "bytes", tag = "5")]
        pub app_state_bytes: ::prost::bytes::Bytes,
        #[prost(int64, tag = "6")]
        pub initial_height: i64,
    }

    https://github.com/informalsystems/tendermint-rs/blob/mikhail/cometbft-0.38/proto/src/prost/v0_38/tendermint.abci.rs#L72-L85
         */

    /**
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ResponseInitChain {
        #[prost(message, optional, tag = "1")]
        pub consensus_params: ::core::option::Option<super::types::ConsensusParams>,
        #[prost(message, repeated, tag = "2")]
        pub validators: ::prost::alloc::vec::Vec<ValidatorUpdate>,
        #[prost(bytes = "bytes", tag = "3")]
        pub app_hash: ::prost::bytes::Bytes,
    }
    **/
    /// Called once upon blockchain startup with genesis info, before anything else is called
    pub fn init(&self /* ??? */) -> Result<(), PulsarError> {
        todo!();
    }

    // returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> Result<Vec<u8>, PulsarError> {
        let lock = self.storage.read();
        self.logic.query(lock.deref().as_ref(), request)
    }

    /**
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ResponseCheckTx {
        #[prost(uint32, tag = "1")]
        pub code: u32,
        #[prost(bytes = "bytes", tag = "2")]
        pub data: ::prost::bytes::Bytes,
        /// nondeterministic
        #[prost(string, tag = "3")]
        pub log: ::prost::alloc::string::String,
        /// nondeterministic
        #[prost(string, tag = "4")]
        pub info: ::prost::alloc::string::String,
        #[prost(int64, tag = "5")]
        pub gas_wanted: i64,
        #[prost(int64, tag = "6")]
        pub gas_used: i64,
        #[prost(message, repeated, tag = "7")]
        pub events: ::prost::alloc::vec::Vec<Event>,
        #[prost(string, tag = "8")]
        pub codespace: ::prost::alloc::string::String,
    }
    */
    pub fn check_tx(&self, _tx: Tx) -> Result<(), PulsarError> {
        todo!();
    }

    /**
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ResponseFinalizeBlock {
        /// set of block events emmitted as part of executing the block
        #[prost(message, repeated, tag = "1")]
        pub events: ::prost::alloc::vec::Vec<Event>,
        /// the result of executing each transaction including the events
        /// the particular transction emitted. This should match the order
        /// of the transactions delivered in the block itself
        #[prost(message, repeated, tag = "2")]
        pub tx_results: ::prost::alloc::vec::Vec<ExecTxResult>,
        /// a list of updates to the validator set. These will reflect the validator set at current height + 2.
        #[prost(message, repeated, tag = "3")]
        pub validator_updates: ::prost::alloc::vec::Vec<ValidatorUpdate>,
        /// updates to the consensus params, if any.
        #[prost(message, optional, tag = "4")]
        pub consensus_param_updates: ::core::option::Option<super::types::ConsensusParams>,
        /// app_hash is the hash of the applications' state which is used to confirm that execution of the transactions was deterministic. It is up to the application to decide which algorithm to use.
        #[prost(bytes = "bytes", tag = "5")]
        pub app_hash: ::prost::bytes::Bytes,
    }

    /// Event allows application developers to attach additional information to
    /// ResponseFinalizeBlock and ResponseCheckTx.
    /// Later, transactions may be queried using these events.
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Event {
        #[prost(string, tag = "1")]
        pub r#type: ::prost::alloc::string::String,
        #[prost(message, repeated, tag = "2")]
        pub attributes: ::prost::alloc::vec::Vec<EventAttribute>,
    }
    /// EventAttribute is a single key-value pair, associated with an event.
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct EventAttribute {
        #[prost(string, tag = "1")]
        pub key: ::prost::alloc::string::String,
        #[prost(string, tag = "2")]
        pub value: ::prost::alloc::string::String,
        /// nondeterministic
        #[prost(bool, tag = "3")]
        pub index: bool,
    }
    /// ExecTxResult contains results of executing one individual transaction.
    ///
    /// * Its structure is equivalent to #ResponseDeliverTx which will be deprecated/deleted
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct ExecTxResult {
        #[prost(uint32, tag = "1")]
        pub code: u32,
        #[prost(bytes = "bytes", tag = "2")]
        pub data: ::prost::bytes::Bytes,
        /// nondeterministic
        #[prost(string, tag = "3")]
        pub log: ::prost::alloc::string::String,
        /// nondeterministic
        #[prost(string, tag = "4")]
        pub info: ::prost::alloc::string::String,
        #[prost(int64, tag = "5")]
        pub gas_wanted: i64,
        #[prost(int64, tag = "6")]
        pub gas_used: i64,
        /// nondeterministic
        #[prost(message, repeated, tag = "7")]
        pub events: ::prost::alloc::vec::Vec<Event>,
        #[prost(string, tag = "8")]
        pub codespace: ::prost::alloc::string::String,
    }
    **/
    pub fn finalize_block(&self, _block: Block) -> Result<(), PulsarError> {
        todo!();
    }
}
