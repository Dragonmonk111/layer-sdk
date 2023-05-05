use std::ops::Deref;

// TODO: make our own custom pulsar-storage package to extend (esp with file system backing, transactions...)
use cosmwasm_std::{BlockInfo, Storage};

use parking_lot::RwLock;
use pulsar_std::{GasMeter, Query, Tx};

use crate::api::{Block, FinalizeBlockResponse, InitChainRequest, InitChainResponse, TxResult};
use crate::error::PulsarResult;
use crate::sm::StateMachine;

const DEFAULT_QUERY_GAS: u64 = 500_000;

/// This maintains all application global state and is a framework-agnostic entrypoint for the
/// application. It *should* be able to run inside an ABCI app as well as an Avalache Subnet.
#[allow(dead_code)]
pub struct App {
    // State
    storage: RwLock<Box<dyn Storage>>,

    // Current Block
    block: RwLock<BlockInfo>,

    // State Machine Logic
    logic: StateMachine,
}

impl App {
    pub fn new(
        storage: impl Storage + 'static,
        logic: StateMachine,
        current_block: BlockInfo,
    ) -> App {
        App {
            storage: RwLock::new(Box::new(storage)),
            block: RwLock::new(current_block),
            logic,
        }
    }

    /// Called once upon blockchain startup with genesis info, before anything else is called
    pub fn init(&self, _request: InitChainRequest) -> PulsarResult<InitChainResponse> {
        todo!();
    }

    /// Returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> PulsarResult<Vec<u8>> {
        let lock = self.storage.read();
        let block = self.block.read();
        let mut meter = GasMeter::new(DEFAULT_QUERY_GAS);
        let resp = self
            .logic
            .query(lock.deref().as_ref(), &mut meter, block.deref(), request)?;
        // TODO: question on how to encode these... should convert to cosmos sdk protobuf?
        // accept some arg on which format to encode
        Ok(resp.to_cosmos()?)
    }

    pub fn check_tx(&self, _tx: Tx) -> TxResult {
        todo!();
    }

    pub fn finalize_block(&self, _block: Block) -> PulsarResult<FinalizeBlockResponse> {
        todo!();
    }
}
