use cosmwasm_schema::cw_serde;
use cosmwasm_vm::{Checksum, VmError};

use cosmwasm_std::{
    ensure_eq, Addr, Binary, BlockInfo, Coin, CosmosMsg, Empty, Env, MessageInfo, Order, Reply,
    ReplyOn, SubMsg, SubMsgResponse,
};

use cw_storage_plus::Bound;
use slay3r_std::api::MsgResponse;
use slay3r_std::response::{
    CodeInfoResponse, ContractInfoResponse, ContractsByCodeResponse, ListCodesResponse,
    QueryResponse, WasmQueryResponse,
};
use slay3r_std::{
    AccountId, BankMsgData, GasError, GasMeter, Msg, MsgData, WasmMsg, WasmMsgData, WasmQuery,
};
use slay3r_storage::{
    prefixed, prefixed_read, Item, Map, PlusError, PrefixedStorage, ReadonlyPrefixedStorage,
    ReadonlyStorage, Storage,
};

use super::events::{migrate_event, reply_event};
use super::utils::build_instantiate_address;
use super::vm::VmCache;
use super::WasmError;
use crate::error::{PulsarError, PulsarResult};
use crate::sm::StateMachine;
use crate::wasm::events::{
    build_contract_events, clear_admin_event, execute_event, instantiate_event, pin_code_event,
    store_code_event, sudo_event, unpin_code_event, update_admin_event,
};
use crate::wasm::utils::build_instantiate_2_address;

pub const NAMESPACE_WASM: &[u8] = b"wasm";

// Numbers taken from wasmd, we should benchmark better
pub(crate) const LOAD_WASM_GAS: u64 = 60_000;
pub(crate) const LOAD_PINNED_WASM_GAS: u64 = 2_000;

// Contract state is kept in Storage, separate from the contracts themselves
const CODES: Map<u64, CodeInfo> = Map::new("codes");
const CONTRACTS: Map<&AccountId, ContractData> = Map::new("contracts");
const CONTRACTS_BY_CODE: Map<(u64, &AccountId), bool> = Map::new("contracts_by_code");

// list of all pinned code_ids, so you can range over them
const PINNED: Map<u64, Empty> = Map::new("pinned");
// latest code id
const CODE_ID: Item<u64> = Item::new("code_id");
// A counter of total contracts created for the v1 instantiate algorithm
const CONTRACT_COUNTER: Item<u64> = Item::new("contract_count");

const PARAMS: Item<WasmParams> = Item::new("params");

#[cw_serde]
pub struct WasmParams {
    pub gov_account: AccountId,
}

/// Contract Data includes information about contract, equivalent of `ContractInfo` in wasmd
/// interface.
#[cw_serde]
pub struct ContractData {
    /// Identifier of stored contract code
    pub code_id: u64,
    /// Address of account who initially instantiated the contract
    pub creator: AccountId,
    /// Optional address of account who can execute migrations
    pub admin: Option<AccountId>,
    /// Metadata passed while contract instantiation
    pub label: String,
    /// Blockchain height in the moment of instantiating the contract
    pub created: u64,
    // LATER: add ibc info
}

#[cw_serde]
pub struct CodeInfo {
    /// The address that initially stored the code
    pub creator: AccountId,
    /// The hash of the Wasm blob
    pub checksum: Binary,
    /// If it is pinned or not
    pub pinned: bool,
}

impl CodeInfo {
    /// This gets a proper checksum to load a wasmer vm, and charges a gas price based on whether it is pinned or not
    fn get_checksum_to_execute(&self, meter: &GasMeter) -> Result<Checksum, GasError> {
        if self.pinned {
            meter.charge(LOAD_PINNED_WASM_GAS)?;
        } else {
            meter.charge(LOAD_WASM_GAS)?;
        }
        Ok(self.get_checksum_not_executing())
    }

    /// Use this is you need Checksum to interact with the cache, but not run wasmer vm, like pin/unpin
    fn get_checksum_not_executing(&self) -> Checksum {
        Checksum::try_from(self.checksum.as_slice()).unwrap()
    }
}

#[derive(Debug)]
pub struct Wasm {
    cache: VmCache,
}

/// This can be set different on each node, outside of consensus
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

    // This sets constant params used
    pub fn init(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        _block: &BlockInfo,
        params: crate::genesis::WasmParams,
        _sm: &StateMachine,
    ) -> PulsarResult<()> {
        let mut wasm_storage = prefixed(storage, NAMESPACE_WASM);
        let validated = WasmParams {
            gov_account: AccountId::parse_string(&params.gov_account)?,
        };
        PARAMS.save(&mut wasm_storage, meter, &validated)?;
        Ok(())
    }

    /// This is v1 contract address generation
    fn generate_address(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        sender: &AccountId,
        code_id: u64,
    ) -> Result<AccountId, PulsarError> {
        let mut wasm_store = prefixed(storage, NAMESPACE_WASM);
        let counter = CONTRACT_COUNTER
            .may_load(wasm_store.as_ref(), meter)?
            .unwrap_or_default()
            + 1;
        CONTRACT_COUNTER.save(&mut wasm_store, meter, &counter)?;
        build_instantiate_address(sender, code_id, counter)
    }

    fn next_id(&self, wasm_store: &mut dyn Storage, meter: &GasMeter) -> Result<u64, PulsarError> {
        let id = CODE_ID
            .may_load(wasm_store.as_ref(), meter)?
            .unwrap_or_default()
            + 1;
        CODE_ID.save(wasm_store, meter, &id)?;
        Ok(id)
    }

    pub fn process_msg(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        signer: &AccountId,
        msg: WasmMsg,
    ) -> PulsarResult<MsgResponse> {
        let resp = match msg {
            WasmMsg::StoreCode { sender, code } => {
                ensure_eq!(signer, &sender, WasmError::Unauthorized);
                let (checksum, analysis) = self.cache.store_code(&code).map_err(map_vm_error)?;
                let info = CodeInfo {
                    creator: sender,
                    checksum: Vec::<u8>::from(checksum).into(),
                    pinned: false,
                };
                let mut wasm_store = prefixed(storage, NAMESPACE_WASM);
                let id = self.next_id(&mut wasm_store, meter)?;

                self.save_code(storage, meter, id, &info)?;
                let data = WasmMsgData::Store {
                    code_id: id,
                    checksum: info.checksum,
                };
                let event = store_code_event(id, analysis);
                MsgResponse::new(vec![event], data)
            }
            WasmMsg::Instantiate {
                sender,
                admin,
                code_id,
                msg,
                funds,
                label,
            } => {
                ensure_eq!(signer, &sender, WasmError::Unauthorized);
                let code = self.load_code(storage.as_ref(), meter, code_id)?;
                let contract_addr = self.generate_address(storage, meter, &sender, code_id)?;
                return self.do_instantiate(
                    storage,
                    meter,
                    block,
                    sm,
                    signer,
                    contract_addr,
                    code_id,
                    code,
                    admin,
                    msg,
                    funds,
                    label,
                );
            }
            WasmMsg::Instantiate2 {
                sender,
                admin,
                code_id,
                label,
                msg,
                funds,
                salt,
            } => {
                ensure_eq!(signer, &sender, WasmError::Unauthorized);
                let code = self.load_code(storage.as_ref(), meter, code_id)?;
                let contract_addr = build_instantiate_2_address(
                    &code.checksum,
                    &sender,
                    &salt,
                    &[], // we consider fix_msg to always be false, this was cosmwasm-std decision
                )?;
                return self.do_instantiate(
                    storage,
                    meter,
                    block,
                    sm,
                    signer,
                    contract_addr,
                    code_id,
                    code,
                    admin,
                    msg,
                    funds,
                    label,
                );
            }
            WasmMsg::Execute {
                sender,
                contract_addr,
                msg,
                funds,
            } => {
                ensure_eq!(signer, &sender, WasmError::Unauthorized);
                let contract = self.load_contract(storage.as_ref(), meter, &contract_addr)?;
                let code = self.load_code(storage.as_ref(), meter, contract.code_id)?;

                // send funds
                if !funds.is_empty() {
                    sm.bank.transfer(
                        storage,
                        meter,
                        sender.clone(),
                        contract_addr.clone(),
                        funds.clone(),
                    )?;
                }
                let info = build_info(&sender, funds);

                // call execute on cache
                let env = build_env(block, &contract_addr);
                let checksum = code.get_checksum_to_execute(meter)?;
                let (result, gas) = self.cache.execute(
                    &checksum,
                    &env,
                    &info,
                    &msg,
                    storage,
                    &contract_addr,
                    meter,
                    sm,
                );
                meter.charge(gas)?;
                let result = map_cache_result(result)?;

                // Build events
                let mut events =
                    build_contract_events(&contract_addr, result.events, result.attributes)?;
                let event = execute_event(&contract_addr);
                events.insert(0, event);

                // Dispatch messages
                let data = WasmMsgData::Execute {
                    data: result.data.unwrap_or_default(),
                };
                let mut response = MsgResponse::new(events, data);
                self.dispatch_response_messages(
                    storage,
                    meter,
                    block,
                    sm,
                    &contract_addr,
                    &checksum,
                    result.messages,
                    &mut response,
                )?;
                response
            }
            WasmMsg::Migrate {
                sender,
                contract_addr,
                new_code_id,
                msg,
            } => {
                // only admin can migrate
                let mut contract = self.load_contract(storage.as_ref(), meter, &contract_addr)?;
                match &contract.admin {
                    Some(admin) if admin == &sender => Ok(()),
                    _ => Err(WasmError::Unauthorized),
                }?;
                // update the code and get the new code info
                self.remove_contract_by_code(storage, meter, &contract_addr, &contract)?;
                let code = self.load_code(storage.as_ref(), meter, new_code_id)?;
                contract.code_id = new_code_id;
                self.save_contract(storage, meter, &contract_addr, &contract)?;
                self.save_contract_by_code(storage, meter, &contract_addr, &contract)?;

                // call migrate on vm
                let env = build_env(block, &contract_addr);
                let checksum = code.get_checksum_to_execute(meter)?;
                let (result, gas) =
                    self.cache
                        .migrate(&checksum, &env, &msg, storage, &contract_addr, meter, sm);
                meter.charge(gas)?;
                let result = map_cache_result(result)?;

                // Build events
                let mut events =
                    build_contract_events(&contract_addr, result.events, result.attributes)?;
                let event = migrate_event(&contract_addr, new_code_id);
                events.insert(0, event);

                // dispatch messages
                let data = WasmMsgData::Migrate {
                    data: result.data.unwrap_or_default(),
                };
                let mut response = MsgResponse::new(events, data);
                self.dispatch_response_messages(
                    storage,
                    meter,
                    block,
                    sm,
                    &contract_addr,
                    &checksum,
                    result.messages,
                    &mut response,
                )?;
                response
            }
            WasmMsg::ClearAdmin {
                sender,
                contract_addr,
            } => {
                ensure_eq!(signer, &sender, WasmError::Unauthorized);
                let mut contract = self.load_contract(storage.as_ref(), meter, &contract_addr)?;
                match &contract.admin {
                    Some(admin) if admin == &sender => Ok(()),
                    _ => Err(WasmError::Unauthorized),
                }?;
                contract.admin = None;
                self.save_contract(storage, meter, &contract_addr, &contract)?;
                let event = clear_admin_event(&contract_addr);
                MsgResponse::new(vec![event], WasmMsgData::ClearAdmin {})
            }
            WasmMsg::UpdateAdmin {
                sender,
                contract_addr,
                admin,
            } => {
                ensure_eq!(signer, &sender, WasmError::Unauthorized);
                let mut contract = self.load_contract(storage.as_ref(), meter, &contract_addr)?;
                match &contract.admin {
                    Some(admin) if admin == &sender => Ok(()),
                    _ => Err(WasmError::Unauthorized),
                }?;
                let event = update_admin_event(&contract_addr, &admin);
                contract.admin = Some(admin);
                self.save_contract(storage, meter, &contract_addr, &contract)?;
                MsgResponse::new(vec![event], WasmMsgData::UpdateAdmin {})
            }
            WasmMsg::Sudo {
                sender,
                contract_addr,
                msg,
            } => {
                // only special sender can do this - stored as param
                let WasmParams { gov_account } =
                    PARAMS.load(&prefixed_read(storage.as_ref(), NAMESPACE_WASM), meter)?;
                ensure_eq!(sender, gov_account, WasmError::Unauthorized);

                let contract = self.load_contract(storage.as_ref(), meter, &contract_addr)?;
                let code = self.load_code(storage.as_ref(), meter, contract.code_id)?;

                // call migrate on vm
                let env = build_env(block, &contract_addr);
                let checksum = code.get_checksum_to_execute(meter)?;
                let (result, gas) =
                    self.cache
                        .sudo(&checksum, &env, &msg, storage, &contract_addr, meter, sm);
                meter.charge(gas)?;
                let result = map_cache_result(result)?;

                // Build events
                let mut events =
                    build_contract_events(&contract_addr, result.events, result.attributes)?;
                let event = sudo_event(&contract_addr);
                events.insert(0, event);

                // dispatch messages
                let data = WasmMsgData::Sudo {
                    data: result.data.unwrap_or_default(),
                };
                let mut response = MsgResponse::new(events, data);
                self.dispatch_response_messages(
                    storage,
                    meter,
                    block,
                    sm,
                    &contract_addr,
                    &checksum,
                    result.messages,
                    &mut response,
                )?;
                response
            }
            WasmMsg::Pin { sender, code_id } => {
                // only special sender can do this - stored as param
                let WasmParams { gov_account } =
                    PARAMS.load(&prefixed_read(storage.as_ref(), NAMESPACE_WASM), meter)?;
                ensure_eq!(sender, gov_account, WasmError::Unauthorized);

                let mut code = self.load_code(storage.as_ref(), meter, code_id)?;
                if !code.pinned {
                    code.pinned = true;
                    self.save_code(storage, meter, code_id, &code)?;
                    self.cache
                        .pin(&code.get_checksum_not_executing())
                        .map_err(map_vm_error)?;
                    PINNED.save(
                        &mut prefixed(storage, NAMESPACE_WASM),
                        meter,
                        code_id,
                        &Empty {},
                    )?;
                }
                // add events
                let event = pin_code_event(code_id);
                MsgResponse::new(vec![event], WasmMsgData::PinCode {})
            }
            WasmMsg::Unpin { sender, code_id } => {
                // only special sender can do this - stored as param
                let WasmParams { gov_account } =
                    PARAMS.load(&prefixed_read(storage.as_ref(), NAMESPACE_WASM), meter)?;
                ensure_eq!(sender, gov_account, WasmError::Unauthorized);

                let mut code = self.load_code(storage.as_ref(), meter, code_id)?;
                if code.pinned {
                    code.pinned = false;
                    self.save_code(storage, meter, code_id, &code)?;
                    self.cache
                        .unpin(&code.get_checksum_not_executing())
                        .map_err(map_vm_error)?;
                    PINNED.remove(&mut prefixed(storage, NAMESPACE_WASM), meter, code_id)?;
                }
                // add events
                let event = unpin_code_event(code_id);
                MsgResponse::new(vec![event], WasmMsgData::UnpinCode {})
            }
        };
        Ok(resp)
    }

    /// Internal function only, combining instantiate and instantiate2 common path.
    /// I know there are way too many args, but no one should use this besides two cases
    /// right above it....
    #[allow(clippy::too_many_arguments)]
    fn do_instantiate(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        signer: &AccountId,
        contract_addr: AccountId,
        code_id: u64,
        code: CodeInfo,
        admin: Option<AccountId>,
        msg: Binary,
        funds: Vec<Coin>,
        label: String,
    ) -> PulsarResult<MsgResponse> {
        let sender = signer;

        // reserve and auth account and ensure it is not already taken
        sm.auth
            .claim_internal_account(storage, meter, &contract_addr)?;

        // save contract
        let contract = ContractData {
            code_id,
            creator: sender.clone(),
            admin,
            label,
            created: block.height,
        };
        self.save_contract(storage, meter, &contract_addr, &contract)?;
        self.save_contract_by_code(storage, meter, &contract_addr, &contract)?;

        // send funds
        if !funds.is_empty() {
            sm.bank.transfer(
                storage,
                meter,
                sender.clone(),
                contract_addr.clone(),
                funds.clone(),
            )?;
        }
        let info = build_info(sender, funds);

        // call instantiate on cache
        let env = build_env(block, &contract_addr);
        let checksum = code.get_checksum_to_execute(meter)?;
        let (result, gas) = self.cache.instantiate(
            &checksum,
            &env,
            &info,
            &msg,
            storage,
            &contract_addr,
            meter,
            sm,
        );
        meter.charge(gas)?;
        let result = map_cache_result(result)?;

        // Build events
        let mut events = build_contract_events(&contract_addr, result.events, result.attributes)?;
        let event = instantiate_event(&contract_addr, code_id);
        events.insert(0, event);

        // dispatch messages
        let data = WasmMsgData::Instantiate {
            contract: contract_addr.clone(),
            data: result.data.unwrap_or_default(),
        };
        let mut response = MsgResponse::new(events, data);
        self.dispatch_response_messages(
            storage,
            meter,
            block,
            sm,
            &contract_addr,
            &checksum,
            result.messages,
            &mut response,
        )?;
        Ok(response)
    }

    /// This dispatches all returned messages and adds events to the parent event of the original call
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_response_messages(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        contract: &AccountId,
        checksum: &Checksum,
        msgs: Vec<SubMsg<super::vm::CustomMsg>>,
        // we append events to this (later maybe overwrite the data)
        parent_response: &mut MsgResponse,
    ) -> PulsarResult<()> {
        for msg in msgs {
            // if there is a limit, and it is less than what we have left, use a sub-meter
            let limit_meter = match (msg.gas_limit, meter.remaining()) {
                (Some(limit), left) if limit < left => Some(GasMeter::new(limit)),
                _ => None,
            };
            let sub_meter = match limit_meter.as_ref() {
                Some(m) => m,
                None => meter,
            };

            let slay3r_msg = cosmwasm_msg_to_pulsar(msg.msg, contract)?;

            // ensure we charge if there is a limit_meter, even on error
            let msg_result = sm.process_msg(storage, sub_meter, contract, block, slay3r_msg);
            if let Some(limit_meter) = limit_meter {
                meter.charge(limit_meter.used())?;
            }

            // check if we want to call reply and call
            let is_success = msg_result.is_ok(); // we use this variable later
            let handle_reply = matches!(
                (msg.reply_on, is_success),
                (ReplyOn::Always, _) | (ReplyOn::Success, true) | (ReplyOn::Error, false)
            );
            if handle_reply {
                let result = match msg_result {
                    Ok(res) => {
                        // append events to parent response (only on success)
                        parent_response.events.extend(res.events.clone());
                        // and prepare a response value to call the contract
                        Ok(SubMsgResponse {
                            events: res.events,
                            data: maybe_binary(encode_cosmwasm_response(res.data).1),
                        })
                    }
                    Err(err) => Err(err.to_string()),
                };
                let reply = Reply {
                    id: msg.id,
                    result: result.into(),
                };

                // call the reply entry point
                let env = build_env(block, contract);
                let (reply_result, gas) = self
                    .cache
                    .reply(checksum, &env, &reply, storage, contract, meter, sm);
                meter.charge(gas)?;
                let reply_result = map_cache_result(reply_result)?;

                // Append reply events to the parent
                parent_response
                    .events
                    .push(reply_event(contract, is_success));
                let events =
                    build_contract_events(contract, reply_result.events, reply_result.attributes)?;
                parent_response.events.extend(events);

                // if data is set, then we override the parent data field
                if let Some(data) = reply_result.data {
                    // parent must be Execute, Instantiate(2), Migrate, or Sudo
                    // update the data field but leave the type the same
                    set_data_field(&mut parent_response.data, data);
                }

                if !reply_result.messages.is_empty() {
                    // we just run them all and add events to the parent...
                    self.dispatch_response_messages(
                        storage,
                        meter,
                        block,
                        sm,
                        contract,
                        checksum,
                        reply_result.messages,
                        parent_response,
                    )?;
                }
            } else {
                // add events to parent (when we don't use reply)
                let res = msg_result?;
                parent_response.events.extend(res.events);
            }
        }
        Ok(())
    }

    pub fn query(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        block: &BlockInfo,
        sm: &StateMachine,
        request: WasmQuery,
    ) -> PulsarResult<QueryResponse<PulsarError>> {
        let resp = match request {
            WasmQuery::Smart { contract_addr, msg } => {
                let contract = self.load_contract(storage, meter, &contract_addr)?;
                let code = self.load_code(storage, meter, contract.code_id)?;
                let checksum = code.get_checksum_to_execute(meter)?;
                let env = build_env(block, &contract_addr);
                let (result, gas) =
                    self.cache
                        .query(&checksum, &env, &msg, storage, &contract_addr, meter, sm);
                meter.charge(gas)?;
                let result = map_cache_result(result)?;
                WasmQueryResponse::Smart(result)
            }
            WasmQuery::Raw { contract_addr, key } => {
                let sub_store = read_contract_storage(storage, &contract_addr);
                let data = sub_store.get(meter, &key)?.unwrap_or_default();
                WasmQueryResponse::Raw(data.into())
            }
            WasmQuery::ContractInfo { contract_addr } => {
                let ContractData {
                    code_id,
                    creator,
                    admin,
                    label,
                    created,
                } = self.load_contract(storage, meter, &contract_addr)?;
                let CodeInfo { pinned, .. } = self.load_code(storage, meter, code_id)?;
                let resp = ContractInfoResponse {
                    addresss: contract_addr,
                    code_id,
                    creator,
                    admin,
                    ibc_port: None,
                    pinned,
                    label,
                    created,
                };
                WasmQueryResponse::ContractInfo(resp)
            }
            WasmQuery::CodeInfo { code_id } => {
                let CodeInfo {
                    creator,
                    checksum,
                    pinned,
                } = self.load_code(storage, meter, code_id)?;
                let hash = Checksum::try_from(checksum.as_slice()).map_err(map_vm_error)?;
                let data = self.cache.load_code(&hash).map_err(map_vm_error)?;
                let resp = CodeInfoResponse {
                    data: data.into(),
                    code_info: slay3r_std::response::CodeInfo {
                        code_id,
                        creator,
                        checksum,
                        pinned,
                    },
                };
                WasmQueryResponse::CodeInfo(resp)
            }
            WasmQuery::ListCodes { from, limit } => {
                let start = from.map(Bound::inclusive);
                let limit = limit.unwrap_or(100u32) as u64; // max page size // TODO: configure??
                let end = Some(Bound::exclusive(from.unwrap_or(0) + limit));

                let reader = prefixed_read(storage, NAMESPACE_WASM);
                let iter = CODES.range(&reader, meter, start, end, Order::Ascending)?;
                let code_infos = iter
                    .map(|r| {
                        r.map(|(k, v)| slay3r_std::response::CodeInfo {
                            code_id: k,
                            creator: v.creator,
                            checksum: v.checksum,
                            pinned: v.pinned,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                WasmQueryResponse::ListCodes(ListCodesResponse { code_infos })
            }
            WasmQuery::ContractsByCode { code_id } => {
                // Uses a manually tracked secondary index...
                let wasm_store = prefixed_read(storage, NAMESPACE_WASM);
                let contracts = CONTRACTS_BY_CODE
                    .prefix(code_id)
                    .range(&wasm_store, meter, None, None, Order::Ascending)?
                    .map(|r| r.map(|(k, _)| k))
                    .collect::<Result<Vec<_>, _>>()?;
                let resp = ContractsByCodeResponse { contracts };
                WasmQueryResponse::ContractsByCode(resp)
            }
        };
        Ok(QueryResponse::Wasm(resp))
    }

    fn load_contract(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        address: &AccountId,
    ) -> Result<ContractData, PlusError> {
        CONTRACTS.load(&prefixed_read(storage, NAMESPACE_WASM), meter, address)
    }

    fn save_contract(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        address: &AccountId,
        contract: &ContractData,
    ) -> Result<(), PlusError> {
        let mut wasm_store = prefixed(storage, NAMESPACE_WASM);
        CONTRACTS.save(&mut wasm_store, meter, address, contract)
    }

    fn save_contract_by_code(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        address: &AccountId,
        contract: &ContractData,
    ) -> Result<(), PlusError> {
        let mut wasm_store = prefixed(storage, NAMESPACE_WASM);
        // We need to manually remove old code id entry in migrate... but this handles a lot of the tracking
        CONTRACTS_BY_CODE.save(&mut wasm_store, meter, (contract.code_id, address), &true)
    }

    // call this on migrate of whenever a contract will have a new code_id
    fn remove_contract_by_code(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        address: &AccountId,
        contract: &ContractData,
    ) -> Result<(), GasError> {
        let mut wasm_store = prefixed(storage, NAMESPACE_WASM);
        CONTRACTS_BY_CODE.remove(&mut wasm_store, meter, (contract.code_id, address))
    }

    fn load_code(
        &self,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        id: u64,
    ) -> Result<CodeInfo, PlusError> {
        CODES.load(&prefixed_read(storage, NAMESPACE_WASM), meter, id)
    }

    fn save_code(
        &self,
        storage: &mut dyn Storage,
        meter: &GasMeter,
        id: u64,
        info: &CodeInfo,
    ) -> Result<(), PlusError> {
        CODES.save(&mut prefixed(storage, NAMESPACE_WASM), meter, id, info)
    }
}

pub fn contract_storage<'a>(storage: &'a mut dyn Storage, addr: &AccountId) -> PrefixedStorage<'a> {
    PrefixedStorage::multilevel(storage, &[NAMESPACE_WASM, addr.as_slice()])
}

pub fn read_contract_storage<'a>(
    storage: &'a dyn ReadonlyStorage,
    addr: &AccountId,
) -> ReadonlyPrefixedStorage<'a> {
    ReadonlyPrefixedStorage::multilevel(storage, &[NAMESPACE_WASM, addr.as_slice()])
}

fn build_env(block: &BlockInfo, contract: &AccountId) -> Env {
    Env {
        block: block.clone(),
        transaction: None, // review later
        contract: cosmwasm_std::ContractInfo {
            address: Addr::unchecked(contract.to_string()),
        },
    }
}

fn build_info(sender: &AccountId, funds: Vec<Coin>) -> MessageInfo {
    MessageInfo {
        sender: Addr::unchecked(sender.to_string()),
        funds,
    }
}

fn map_vm_error(err: VmError) -> PulsarError {
    match err {
        VmError::GasDepletion { .. } => GasError::OutOfGas.into(),
        // FIXME: make this deterministic
        e => WasmError::Vm(e.to_string()).into(),
    }
}

fn map_contract_error(err: String) -> PulsarError {
    WasmError::Contract(err).into()
}

fn map_cache_result<T>(result: Result<Result<T, String>, VmError>) -> Result<T, PulsarError> {
    result.map_err(map_vm_error)?.map_err(map_contract_error)
}

fn cosmwasm_msg_to_pulsar(msg: CosmosMsg, sender: &AccountId) -> Result<Msg, PulsarError> {
    let res = match msg {
        CosmosMsg::Bank(bank) => match bank {
            cosmwasm_std::BankMsg::Send { to_address, amount } => slay3r_std::BankMsg::Send {
                sender: sender.clone(),
                recipient: AccountId::parse_string(&to_address)?,
                amount,
            }
            .into(),
            cosmwasm_std::BankMsg::Burn { amount } => slay3r_std::BankMsg::Burn {
                sender: sender.clone(),
                amount,
            }
            .into(),
            x => unimplemented!("bank msg {:?}", x),
        },
        CosmosMsg::Wasm(wasm) => match wasm {
            cosmwasm_std::WasmMsg::Execute {
                contract_addr,
                msg,
                funds,
            } => slay3r_std::WasmMsg::Execute {
                contract_addr: AccountId::parse_string(&contract_addr)?,
                msg,
                sender: sender.clone(),
                funds,
            }
            .into(),
            cosmwasm_std::WasmMsg::Instantiate {
                admin,
                code_id,
                msg,
                funds,
                label,
            } => slay3r_std::WasmMsg::Instantiate {
                sender: sender.clone(),
                admin: admin.map(|x| AccountId::parse_string(&x)).transpose()?,
                code_id,
                msg,
                funds,
                label,
            }
            .into(),
            // TODO: enable feature flags and support this
            // cosmwasm_std::WasmMsg::Instantiate2 { .. } => todo!(),
            cosmwasm_std::WasmMsg::Migrate {
                contract_addr,
                msg,
                new_code_id,
            } => slay3r_std::WasmMsg::Migrate {
                contract_addr: AccountId::parse_string(&contract_addr)?,
                msg,
                sender: sender.clone(),
                new_code_id,
            }
            .into(),
            cosmwasm_std::WasmMsg::UpdateAdmin {
                contract_addr,
                admin,
            } => slay3r_std::WasmMsg::UpdateAdmin {
                sender: sender.clone(),
                contract_addr: AccountId::parse_string(&contract_addr)?,
                admin: AccountId::parse_string(&admin)?,
            }
            .into(),
            cosmwasm_std::WasmMsg::ClearAdmin { contract_addr } => {
                slay3r_std::WasmMsg::ClearAdmin {
                    sender: sender.clone(),
                    contract_addr: AccountId::parse_string(&contract_addr)?,
                }
                .into()
            }
            x => unimplemented!("wasm msg {:?}", x),
        },
        _ => todo!(),
    };
    Ok(res)
}

use cosmos_sdk_proto::traits::{Message, TypeUrl};

pub fn encode_cosmwasm_response(data: MsgData) -> (&'static str, Vec<u8>) {
    match data {
        MsgData::Bank(bank) => match bank {
            BankMsgData::Send {} => (
                cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend::TYPE_URL,
                cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSendResponse {}.encode_to_vec(),
            ),
            BankMsgData::Burn {} => unknown_cosmwasm_response(),
        },
        MsgData::Wasm(wasm) => match wasm {
            WasmMsgData::Store { code_id, checksum } => (
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgStoreCode::TYPE_URL,
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgStoreCodeResponse {
                    code_id,
                    checksum: checksum.to_vec(),
                }
                .encode_to_vec(),
            ),
            WasmMsgData::Execute { data } => (
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgExecuteContract::TYPE_URL,
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgExecuteContractResponse {
                    data: data.to_vec(),
                }
                .encode_to_vec(),
            ),
            WasmMsgData::Instantiate { contract, data } => (
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgInstantiateContract::TYPE_URL,
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgInstantiateContractResponse {
                    address: contract.to_string(),
                    data: data.to_vec(),
                }
                .encode_to_vec(),
            ),
            WasmMsgData::Instantiate2 { contract, data } => (
                "/cosmwasm.wasm.v1.MsgInstantiateContract2", // missing in cosmos-sdk-proto
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgInstantiateContract2Response {
                    address: contract.to_string(),
                    data: data.to_vec(),
                }
                .encode_to_vec(),
            ),
            WasmMsgData::Migrate { data } => (
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgMigrateContract::TYPE_URL,
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgMigrateContractResponse {
                    data: data.to_vec(),
                }
                .encode_to_vec(),
            ),
            WasmMsgData::UpdateAdmin {} => (
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgUpdateAdmin::TYPE_URL,
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgUpdateAdminResponse {}.encode_to_vec(),
            ),
            WasmMsgData::ClearAdmin {} => (
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgClearAdmin::TYPE_URL,
                cosmos_sdk_proto::cosmwasm::wasm::v1::MsgClearAdminResponse {}.encode_to_vec(),
            ),
            WasmMsgData::Sudo { data: _ } => unknown_cosmwasm_response(),
            WasmMsgData::PinCode {} => unknown_cosmwasm_response(),
            WasmMsgData::UnpinCode {} => unknown_cosmwasm_response(),
        },
    }
}

/// This is a helper response for encode_cosmwasm_response for those who have no
/// Cosmos SDK message corresponding to them
fn unknown_cosmwasm_response() -> (&'static str, Vec<u8>) {
    (
        cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSend::TYPE_URL,
        cosmos_sdk_proto::cosmos::bank::v1beta1::MsgSendResponse {}.encode_to_vec(),
    )
}

// convert vec to binary if non-empty, else None
fn maybe_binary(data: Vec<u8>) -> Option<Binary> {
    if data.is_empty() {
        None
    } else {
        Some(data.into())
    }
}

/// This modifies the data field on the parent one, but keep the type.
/// If this MsgData doesn't have such a field, do nothing
fn set_data_field(parent_data: &mut MsgData, new_data: Binary) {
    if let MsgData::Wasm(wasm) = parent_data {
        match wasm {
            WasmMsgData::Execute { data } => *data = new_data,
            WasmMsgData::Instantiate { data, .. } => *data = new_data,
            WasmMsgData::Instantiate2 { data, .. } => *data = new_data,
            WasmMsgData::Migrate { data } => *data = new_data,
            WasmMsgData::Sudo { data } => *data = new_data,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properly_set_data_field() {
        let mut parent = MsgData::Wasm(WasmMsgData::Execute {
            data: b"initial".into(),
        });
        set_data_field(&mut parent, b"updated".into());
        assert_eq!(
            parent,
            MsgData::Wasm(WasmMsgData::Execute {
                data: b"updated".into()
            })
        );
    }
}
