// TODO: make our own custom pulsar-storage package to extend (esp with file system backing, transactions...)
use cosmwasm_std::Storage;
use parking_lot::RwLock;

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
}
