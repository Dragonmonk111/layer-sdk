use tracing::{
    debug_span,
    field::{debug, Empty},
    info_span, trace_span,
};

use cosmwasm_std::{BlockInfo, Event, StdError};
use pulsar_std::api::{Block, GasInfo, MsgResponse, TxResponse, TxResult};
use pulsar_std::response::QueryResponse;
use pulsar_std::{AccountId, GasMeter, Msg, Query, Tx};
use pulsar_storage::{ReadonlyStorage, ScratchTx, Storage};

use crate::auth::{Auth, TxData};
use crate::bank::Bank;
use crate::error::{PulsarError, PulsarResult};
use crate::genesis::GenesisState;

/// This is an immutable State Machine logic that processes incoming transactions.
/// All mutable state held in Storage, which is passed as an argument to these methods.
#[derive(Debug, Clone)]
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
        genesis: GenesisState,
    ) -> PulsarResult<()> {
        info_span!("sm.init", ?genesis);
        self.bank.init(storage, meter, block, genesis.bank, self)?;
        Ok(())
    }

    pub fn query(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        request: Query,
    ) -> Result<QueryResponse<PulsarError>, PulsarError> {
        let _span = trace_span!("sm.query").entered();
        let result = match request {
            Query::Raw { key } => {
                let value = storage
                    .get(meter, &key)?
                    .ok_or_else(|| StdError::not_found("raw"))?;
                Ok(QueryResponse::Raw { key, value })
            }
            Query::Bank(bank) => self.bank.query(storage, meter, block, self, bank),
            Query::Auth(auth) => self.auth.query(storage, meter, block, self, auth),
            Query::Simulate(tx) => {
                // based on execute_tx
                let mut store = ScratchTx::new(storage);
                let result = self.query_simulate(&mut store, meter, block, tx);
                let gas = GasInfo::from_meter(meter);
                let res = TxResult { gas, result };
                Ok(QueryResponse::Simulate(res))
            }
        };
        result
    }

    fn query_simulate(
        &self,
        store: &mut dyn Storage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> PulsarResult<TxResponse> {
        let _span = debug_span!("sm.query_simulate").entered();
        let data = self
            .auth
            .validate_tx(store, meter, block, self, tx, false)?;
        // charge some gas for the skipped steps, so simulation value works for auto-gas
        // FIXME: figure out a cleaner way to handle this
        meter.charge(2500)?;

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
        let span = debug_span!("sm.process_msg", ?msg, success = Empty, error = Empty).entered();
        let res = match msg {
            Msg::Bank(bank) => self
                .bank
                .process_msg(storage, gas, block, self, sender, bank),
        };
        match &res {
            Ok(response) => span.record("success", debug(&response.events)),
            Err(error) => span.record("error", debug(error)),
        };
        res
    }

    pub fn validate_tx(
        &self,
        storage: &mut dyn Storage,
        meter: &mut GasMeter,
        block: &BlockInfo,
        tx: Tx,
    ) -> PulsarResult<TxData> {
        self.auth.validate_tx(storage, meter, block, self, tx, true)
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
