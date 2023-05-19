use cosmwasm_std::{BlockInfo, Event, StdError};
use pulsar_std::api::{Block, GasInfo, MsgResponse, TxResponse, TxResult};
use pulsar_std::response::{QueryResponse, SimulateQueryResponse};
use pulsar_std::{AccountId, GasMeter, Msg, Query, Tx};
use pulsar_storage::{ReadonlyStorage, ScratchTx, Storage};

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
            Query::Auth(auth) => self.auth.query(storage, meter, block, self, auth),
            Query::Simulate(tx) => {
                // based on execute_tx
                let mut store = ScratchTx::new(storage);
                let result = self.query_simulate(&mut store, meter, block, tx);
                let gas = GasInfo::from_meter(meter);
                // TODO
                let _res = TxResult { gas, result };
                // Ok(QueryResponse::Simulate(res))
                Ok(QueryResponse::Simulate(SimulateQueryResponse {}))
            }
        }
    }

    fn query_simulate(
        &self,
        store: &mut dyn Storage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> PulsarResult<TxResponse> {
        // TODO: don't check signatures in validate_tx (but check format proper)
        let data = self.validate_tx(store, meter, block, tx)?;
        let resps = data
            .msgs
            .into_iter()
            .map(|msg| self.process_msg(store, meter, &data.signer, block, msg))
            .collect::<PulsarResult<Vec<_>>>()?;
        // Question: pull this out to a function? (copied from execute_tx)
        let data = resps
            .iter()
            .map(|r| r.data.clone().unwrap_or_default())
            .collect();
        let events = resps.into_iter().map(|r| r.events).collect();
        // TODO: move api types into std
        Ok(TxResponse { data, events })
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
