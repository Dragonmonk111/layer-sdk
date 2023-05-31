use itertools::Itertools;
use std::collections::HashMap;
use tracing::debug_span;

use cosmwasm_std::{ensure_eq, BlockInfo, Coin, Event, Uint128};

use pulsar_std::api::MsgResponse;
use pulsar_std::response::{AllBalanceResponse, BalanceResponse, QueryResponse, SupplyResponse};
use pulsar_std::{AccountId, CoinEncode, GasMeter, WasmMsg, WasmQuery};
use pulsar_storage::{
    prefixed, prefixed_read, Map, PlusError, PlusResult, ReadonlyStorage, Storage,
};

use super::vm::VmCache;
use crate::error::{PulsarError, PulsarResult};
use crate::sm::StateMachine;
use crate::wasm::WasmError;

// store supply for each denom
// const SUPPLY: Map<&str, Uint128> = Map::new("supply");
// // each (user, denom) pair is stored separately for efficient query of one denom
// const BALANCES: Map<(&AccountId, &str), Uint128> = Map::new("balances");

pub const NAMESPACE_BANK: &[u8] = b"wasm";

#[derive(Debug)]
pub struct Wasm {
    cache: VmCache,
}

#[derive(Debug, Clone)]
pub struct WasmConfig {
    pub cache_dir: String,
}

impl Wasm {
    pub fn new(config: &WasmConfig) -> Self {
        // ensure path exists
        let path = config.cache_dir.as_str();
        std::fs::create_dir_all(path).unwrap();
        Wasm {
            cache: VmCache::init(path),
        }
    }

    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        _block: &BlockInfo,
        _sm: &StateMachine,
        signer: &AccountId,
        msg: WasmMsg,
    ) -> PulsarResult<MsgResponse> {
        todo!()
    }

    pub fn query(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        _block: &BlockInfo,
        _sm: &StateMachine,
        request: WasmQuery,
    ) -> PulsarResult<QueryResponse<PulsarError>> {
        todo!()
    }
}
