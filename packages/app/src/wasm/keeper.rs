use cosmwasm_schema::cw_serde;
use cosmwasm_vm::{Checksum, VmError};
use sha2::{Digest, Sha256};

use cosmwasm_std::{ensure_eq, Addr, Binary, BlockInfo, Coin, Empty, Env, MessageInfo};

use pulsar_std::api::MsgResponse;
use pulsar_std::response::{
    CodeInfoResponse, ContractInfoResponse, QueryResponse, WasmQueryResponse,
};
use pulsar_std::{AccountId, GasError, GasMeter, WasmMsg, WasmQuery};
use pulsar_storage::{
    prefixed, prefixed_read, Item, Map, PlusError, PrefixedStorage, ReadonlyPrefixedStorage,
    ReadonlyStorage, Storage,
};

use super::vm::VmCache;
use super::WasmError;
use crate::error::{PulsarError, PulsarResult};
use crate::sm::StateMachine;
use crate::wasm::events::{
    build_contract_events, clear_admin_event, execute_event, instantiate_event, pin_code_event,
    store_code_event, unpin_code_event, update_admin_event,
};

pub const NAMESPACE_WASM: &[u8] = b"wasm";

// Contract state is kept in Storage, separate from the contracts themselves
const CONTRACTS: Map<&AccountId, ContractData> = Map::new("contracts");
const CODES: Map<u64, CodeInfo> = Map::new("codes");

// list of all pinned code_ids, so you can range over them
const PINNED: Map<u64, Empty> = Map::new("pinned");

const CODE_ID: Item<u64> = Item::new("code_id");
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
    fn to_checksum(&self) -> Checksum {
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
        let mut prehash = Vec::with_capacity(sender.len() + 8 + 8);
        prehash.extend_from_slice(sender.as_slice());
        prehash.extend(code_id.to_be_bytes());
        prehash.extend(counter.to_be_bytes());
        let raw = Sha256::digest(prehash);
        Ok(AccountId::new(&raw)?)
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
                let event = store_code_event(id, analysis);
                MsgResponse::events(vec![event])
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

                // save contract
                let contract = ContractData {
                    code_id,
                    creator: sender.clone(),
                    admin,
                    label,
                    created: block.time.seconds(),
                };
                self.save_contract(storage, meter, &contract_addr, &contract)?;

                // send funds
                let info = build_info(&sender, funds.clone());
                if !funds.is_empty() {
                    sm.bank
                        .transfer(storage, meter, sender, contract_addr.clone(), funds)?;
                }

                // call instantiate on cache
                let mut sub_store = self.contract_storage(storage, &contract_addr);
                let env = build_env(block, &contract_addr);
                let (result, gas) = self.cache.instantiate(
                    &code.to_checksum(),
                    &env,
                    &info,
                    &msg,
                    &mut sub_store,
                    meter,
                    sm,
                );
                meter.charge(gas)?;
                let result = map_cache_result(result)?;

                // Build events
                let mut events =
                    build_contract_events(&contract_addr, result.events, result.attributes)?;
                let event = instantiate_event(&contract_addr, code_id);
                events.insert(0, event);

                // FIXME: handle messages
                MsgResponse::new(events, result.data.unwrap_or_default().into())
            }
            WasmMsg::Instantiate2 { .. } => todo!(),
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
                let info = build_info(&sender, funds.clone());
                if !funds.is_empty() {
                    sm.bank
                        .transfer(storage, meter, sender, contract_addr.clone(), funds)?;
                }

                // call execute on cache
                let mut sub_store = self.contract_storage(storage, &contract_addr);
                let env = build_env(block, &contract_addr);
                let (result, gas) = self.cache.execute(
                    &code.to_checksum(),
                    &env,
                    &info,
                    &msg,
                    &mut sub_store,
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

                // FIXME: handle messages
                MsgResponse::new(events, result.data.unwrap_or_default().into())
            }
            WasmMsg::Migrate { .. } => todo!(),
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
                MsgResponse::events(vec![event])
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
                MsgResponse::events(vec![event])
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
                    self.cache.pin(&code.to_checksum()).map_err(map_vm_error)?;
                    PINNED.save(
                        &mut prefixed(storage, NAMESPACE_WASM),
                        meter,
                        code_id,
                        &Empty {},
                    )?;
                }
                // add events
                let event = pin_code_event(code_id);
                MsgResponse::events(vec![event])
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
                        .unpin(&code.to_checksum())
                        .map_err(map_vm_error)?;
                    PINNED.remove(&mut prefixed(storage, NAMESPACE_WASM), meter, code_id)?;
                }
                // add events
                let event = unpin_code_event(code_id);
                MsgResponse::events(vec![event])
            }
        };
        Ok(resp)
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
                let checksum = code.to_checksum();
                let sub_store = self.read_contract_storage(storage, &contract_addr);
                let env = build_env(block, &contract_addr);
                let (result, gas) = self
                    .cache
                    .query(&checksum, &env, &msg, &sub_store, meter, sm);
                meter.charge(gas)?;
                let result = map_cache_result(result)?;
                WasmQueryResponse::Smart(result)
            }
            WasmQuery::Raw { contract_addr, key } => {
                let sub_store = self.read_contract_storage(storage, &contract_addr);
                let data = sub_store.get(meter, &key)?.unwrap_or_default();
                WasmQueryResponse::Raw(data.into())
            }
            WasmQuery::ContractInfo { contract_addr } => {
                let ContractData {
                    code_id,
                    creator,
                    admin,
                    ..
                } = self.load_contract(storage, meter, &contract_addr)?;
                let CodeInfo { pinned, .. } = self.load_code(storage, meter, code_id)?;
                let resp = ContractInfoResponse {
                    code_id,
                    creator,
                    admin,
                    ibc_port: None,
                    pinned,
                };
                WasmQueryResponse::ContractInfo(resp)
            }
            WasmQuery::CodeInfo { code_id } => {
                let CodeInfo {
                    creator,
                    checksum,
                    pinned,
                } = self.load_code(storage, meter, code_id)?;
                let resp = CodeInfoResponse {
                    code_id,
                    creator,
                    checksum,
                    pinned,
                };
                WasmQueryResponse::CodeInfo(resp)
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
        CONTRACTS.save(
            &mut prefixed(storage, NAMESPACE_WASM),
            meter,
            address,
            contract,
        )
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

    fn contract_storage<'a>(
        &self,
        storage: &'a mut dyn Storage,
        addr: &AccountId,
    ) -> PrefixedStorage<'a> {
        PrefixedStorage::multilevel(storage, &[NAMESPACE_WASM, addr.as_slice()])
    }

    fn read_contract_storage<'a>(
        &self,
        storage: &'a dyn ReadonlyStorage,
        addr: &AccountId,
    ) -> ReadonlyPrefixedStorage<'a> {
        ReadonlyPrefixedStorage::multilevel(storage, &[NAMESPACE_WASM, addr.as_slice()])
    }
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
