use cosmwasm_std::{BlockInfo, Event, StdError};
use pulsar_std::response::QueryResponse;
use pulsar_std::{AccountId, GasMeter, Msg, Query, Tx};
use pulsar_storage::{ReadonlyStorage, Storage};

use crate::api::{Block, MsgResponse};
use crate::auth::{Auth, TxData};
use crate::bank::Bank;
use crate::error::{PulsarError, PulsarResult};
use crate::genesis::GenesisState;

/// This is an immutable State Machine logic that processes incoming transactions.
/// All mutable state held in Storage, which is passed as an argument to these methods.
pub struct StateMachine {
    pub auth: Auth,

    pub bank: Bank,
}

impl StateMachine {
    pub fn new() -> Self {
        StateMachine {
            auth: Auth::new(),
            bank: Bank::new(),
        }
    }

    pub fn init(
        &self,
        storage: &mut dyn Storage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        request: GenesisState,
    ) -> PulsarResult<()> {
        self.bank.init(storage, meter, block, request.bank, self)?;
        Ok(())
    }

    pub fn query(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        request: Query,
    ) -> Result<QueryResponse, PulsarError> {
        match request {
            Query::Raw { key } => {
                let value = storage
                    .get(meter, &key)?
                    .ok_or_else(|| StdError::not_found("raw"))?;
                Ok(QueryResponse::Raw { value })
            }
            Query::Bank(bank) => self.bank.query(storage, meter, block, self, bank),
        }
    }

    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        gas: &mut GasMeter,
        sender: &AccountId,
        block: &BlockInfo,
        msg: Msg,
    ) -> PulsarResult<MsgResponse> {
        match msg {
            Msg::Bank(bank) => self
                .bank
                .process_msg(storage, gas, block, self, sender, bank),
        }
    }

    pub fn validate_tx(
        &self,
        storage: &mut dyn Storage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> PulsarResult<TxData> {
        self.auth.validate_tx(storage, meter, block, self, tx)
    }

    /// Note: erroring here (including exceeding gas limits) will abort block execution. Be careful.
    pub fn begin_block(
        &self,
        _storage: &mut dyn Storage,
        // this is set to the gas limit for begin blockers
        _meter: &mut GasMeter,
        // here we have full block info including proposer and voters (for rewards if needed)
        _block: &Block,
    ) -> PulsarResult<Vec<Event>> {
        // FIXME: implement this later
        Ok(vec![])
    }

    /// Note: erroring here (including exceeding gas limits) will abort block execution. Be careful.
    pub fn end_block(
        &self,
        _storage: &mut dyn Storage,
        // this is set to the gas limit for end blockers
        _meter: &mut GasMeter,
        // this is just block metadata
        _block: &BlockInfo,
    ) -> PulsarResult<Vec<Event>> {
        // FIXME: implement this later
        Ok(vec![])
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}
