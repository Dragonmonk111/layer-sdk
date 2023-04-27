use std::ops::Deref;
// TODO: make our own custom pulsar-storage package to extend (esp with file system backing, transactions...)
use crate::error::PulsarError;
use cosmwasm_std::Storage;
use parking_lot::RwLock;
use pulsar_std::Query;

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

    // returns serialized response to the query that can be passed back verbatum
    pub fn query(&self, request: Query) -> Result<Vec<u8>, PulsarError> {
        let lock = self.storage.read();
        self.logic.query(lock.deref().as_ref(), request)
    }

    pub fn check_tx(&self /* ??? */) -> Result<(), PulsarError> {
        todo!();
    }

    pub fn execute_block(&self /* ??? */) -> Result<(), PulsarError> {
        todo!();
    }
}
