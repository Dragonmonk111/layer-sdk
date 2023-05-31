use std::{collections::HashSet, fmt, path::PathBuf};

use cosmwasm_std::{Binary, Empty, Env, MessageInfo, Response};
use cosmwasm_vm::{
    call_execute, call_instantiate, call_query, Cache, CacheOptions, Checksum, InstanceOptions,
    Size, VmError,
};
use pulsar_std::GasMeter;
use pulsar_storage::{ReadonlyStorage, ScratchTx, Storage, WeakSubTx};

use crate::StateMachine;

use super::backend::{danger_will_robinson, out_of_gas, VmApi, VmQuerier, VmStore};

const DEFAULT_CACHE_MB: usize = 500;
const DEFAULT_INSTANCE_MB: usize = 32;
// const CAPABILITIES: &[&str] = &["iterator"];
const CAPABILITIES: &[&str] = &["iterator", "staking", "stargate"];
const PRINT_DEBUG: bool = false;
// TODO: what is this really?
const SDK_TO_WASMER_GAS_FACTOR: u64 = 150_000_000;

fn capabilities() -> HashSet<String> {
    CAPABILITIES.iter().map(|s| s.to_string()).collect()
}

pub struct VmCache {
    cache: Cache<VmApi, VmStore, VmQuerier>,
    print_debug: bool,
}

impl fmt::Debug for VmCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VmCache")
            .field("print_debug", &self.print_debug)
            .finish()
    }
}

impl VmCache {
    // TODO: make more args?
    pub fn init(cache_dir: &str) -> Self {
        let cache_options = CacheOptions {
            base_dir: PathBuf::from(cache_dir),
            memory_cache_size: Size::mebi(DEFAULT_CACHE_MB),
            instance_memory_limit: Size::mebi(DEFAULT_INSTANCE_MB),
            available_capabilities: capabilities(),
        };
        let cache = unsafe { Cache::new(cache_options).unwrap() };
        VmCache {
            cache,
            print_debug: PRINT_DEBUG,
        }
    }

    pub fn store_code(&self, wasm: &[u8]) -> Result<Checksum, VmError> {
        self.cache.save_wasm(wasm)
    }

    pub fn load_code(&self, checksum: &Checksum) -> Result<Vec<u8>, VmError> {
        self.cache.load_wasm(checksum)
    }

    pub fn pin(&self, checksum: &Checksum) -> Result<(), VmError> {
        self.cache.pin(checksum)
    }

    pub fn unpin(&self, checksum: &Checksum) -> Result<(), VmError> {
        self.cache.unpin(checksum)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn instantiate(
        &self,
        checksum: &Checksum,
        env: &Env,
        info: &MessageInfo,
        msg: &[u8],
        storage: &mut dyn Storage,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (Result<Result<Response<Empty>, String>, VmError>, u64) {
        let gas_limit = meter.remaining().saturating_mul(SDK_TO_WASMER_GAS_FACTOR);
        let options = InstanceOptions {
            gas_limit,
            print_debug: self.print_debug,
        };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let mut working = WeakSubTx::new(storage.as_ref());
        let query = storage.as_ref();

        // This is where we fake all the lifetimes....
        let backend = unsafe { danger_will_robinson(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_instantiate(&mut instance, env, info, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = instance.create_gas_report().used_internally / SDK_TO_WASMER_GAS_FACTOR;

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = working.prepare();
                if let Err(e) = ops.commit(storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        checksum: &Checksum,
        env: &Env,
        info: &MessageInfo,
        msg: &[u8],
        storage: &mut dyn Storage,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (Result<Result<Response<Empty>, String>, VmError>, u64) {
        let gas_limit = meter.remaining().saturating_mul(SDK_TO_WASMER_GAS_FACTOR);
        let options = InstanceOptions {
            gas_limit,
            print_debug: self.print_debug,
        };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let mut working = WeakSubTx::new(storage.as_ref());
        let query = storage.as_ref();

        // This is where we fake all the lifetimes....
        let backend = unsafe { danger_will_robinson(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_execute(&mut instance, env, info, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = instance.create_gas_report().used_internally / SDK_TO_WASMER_GAS_FACTOR;

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = working.prepare();
                if let Err(e) = ops.commit(storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn query(
        &self,
        checksum: &Checksum,
        env: &Env,
        msg: &[u8],
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (Result<Result<Binary, String>, VmError>, u64) {
        let gas_limit = meter.remaining().saturating_mul(SDK_TO_WASMER_GAS_FACTOR);
        let options = InstanceOptions {
            gas_limit,
            print_debug: self.print_debug,
        };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let mut scratch = ScratchTx::new(storage);

        // This is where we fake all the lifetimes....
        let backend = unsafe { danger_will_robinson(sm, &mut scratch, storage, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_query(&mut instance, env, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = instance.create_gas_report().used_internally / SDK_TO_WASMER_GAS_FACTOR;

        // always abort scratch, as we don't want to commit anything
        scratch.abort();
        instance.recycle();

        (result, gas_used)
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        coin, from_slice,
        testing::{mock_env, mock_info},
        to_vec, Order, Uint128,
    };
    use cw20::Cw20Coin;
    use pulsar_std::AccountId;
    use pulsar_storage::{MemoryStore, PersistentStorage};

    use crate::AppConfig;

    use super::*;

    // v1.0.1
    const CW20_BASE: &[u8] = include_bytes!("../../../fixtures/cw20_base.wasm");

    #[test]
    fn can_instatiate() {
        let path = "/tmp/pulsar/test-can-instantiate";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let vm = VmCache::init(path);
        let checksum = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let sender = AccountId::unchecked("Sillyness");
        let info = mock_info(&sender.to_string(), &[coin(55_000, "upulse")]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();

        let msg = cw20_base::msg::InstantiateMsg {
            name: "pulsar".to_string(),
            symbol: "PLS".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: sender.to_string(),
                amount: Uint128::new(1234567),
            }],
            mint: None,
            marketing: None,
        };
        let msg = to_vec(&msg).unwrap();

        let mut writer = store.writer();
        let (res, gas_used) =
            vm.instantiate(&checksum, &env, &info, &msg, &mut writer, &meter, &sm);
        let res = res.unwrap().unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.events.len(), 0);
        assert_eq!(res.attributes.len(), 0);
        assert_eq!(gas_used, 58);

        // query the state was written - token_info and total supply
        let num = writer
            .range(&meter, None, None, Order::Ascending)
            .unwrap()
            .count();
        assert_eq!(num, 3);
    }

    #[test]
    fn happy_path_create_send_query() {
        let path = "/tmp/pulsar/test-happy-path-create-send-query";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let mut vm = VmCache::init(path);
        let checksum = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let sender = AccountId::unchecked("Sillyness");
        let info = mock_info(&sender.to_string(), &[]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();
        let mut writer = store.writer();

        // instantiate
        let msg = cw20_base::msg::InstantiateMsg {
            name: "pulsar".to_string(),
            symbol: "PLS".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: sender.to_string(),
                amount: Uint128::new(1234567),
            }],
            mint: None,
            marketing: None,
        };
        let msg = to_vec(&msg).unwrap();
        let (res, _) = vm.instantiate(&checksum, &env, &info, &msg, &mut writer, &meter, &sm);
        let _ = res.unwrap().unwrap();

        // query two addresses
        let rcpt = AccountId::unchecked("Bystander");
        let hero = query_balance(
            &mut vm,
            &checksum,
            &env,
            &sender,
            writer.as_ref(),
            &meter,
            &sm,
        );
        assert_eq!(hero.u128(), 1234567u128);
        let zero = query_balance(
            &mut vm,
            &checksum,
            &env,
            &rcpt,
            writer.as_ref(),
            &meter,
            &sm,
        );
        assert_eq!(zero.u128(), 0u128);

        // send some surpise tokens
        let msg = cw20_base::msg::ExecuteMsg::Transfer {
            recipient: rcpt.to_string(),
            amount: Uint128::new(23456),
        };
        let msg = to_vec(&msg).unwrap();
        let (res, _) = vm.execute(&checksum, &env, &info, &msg, &mut writer, &meter, &sm);
        let _ = res.unwrap().unwrap();

        // query two addresses wirh new balances
        let rcpt = AccountId::unchecked("Bystander");
        let hero = query_balance(
            &mut vm,
            &checksum,
            &env,
            &sender,
            writer.as_ref(),
            &meter,
            &sm,
        );
        assert_eq!(hero.u128(), 1211111u128);
        let zero = query_balance(
            &mut vm,
            &checksum,
            &env,
            &rcpt,
            writer.as_ref(),
            &meter,
            &sm,
        );
        assert_eq!(zero.u128(), 23456u128);
    }

    #[test]
    fn query_with_iterator() {
        let path = "/tmp/pulsar/test-query-with-iterator";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let vm = VmCache::init(path);
        let checksum = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let one = AccountId::unchecked("One");
        let two = AccountId::unchecked("Two");
        let three = AccountId::unchecked("Xyz");
        let info = mock_info(&one.to_string(), &[]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();
        let mut writer = store.writer();

        // instantiate
        let msg = cw20_base::msg::InstantiateMsg {
            name: "pulsar".to_string(),
            symbol: "PLS".to_string(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: one.to_string(),
                    amount: Uint128::new(1234567),
                },
                Cw20Coin {
                    address: two.to_string(),
                    amount: Uint128::new(7654321),
                },
                Cw20Coin {
                    address: three.to_string(),
                    amount: Uint128::new(818818),
                },
            ],
            mint: None,
            marketing: None,
        };
        let msg = to_vec(&msg).unwrap();
        let (res, _) = vm.instantiate(&checksum, &env, &info, &msg, &mut writer, &meter, &sm);
        let _ = res.unwrap().unwrap();

        // now list all accounts
        let msg = cw20_base::msg::QueryMsg::AllAccounts {
            start_after: None,
            limit: None,
        };
        let msg = to_vec(&msg).unwrap();
        let (res, _) = vm.query(&checksum, &env, &msg, writer.as_ref(), &meter, &sm);
        let res = res.unwrap().unwrap();
        let cw20::AllAccountsResponse { accounts } = from_slice(&res).unwrap();
        assert_eq!(
            accounts,
            vec![two.to_string(), one.to_string(), three.to_string()]
        );
    }

    fn query_balance(
        vm: &mut VmCache,
        checksum: &Checksum,
        env: &Env,
        account: &AccountId,
        storage: &dyn ReadonlyStorage,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> Uint128 {
        let msg = cw20_base::msg::QueryMsg::Balance {
            address: account.to_string(),
        };
        let msg = to_vec(&msg).unwrap();
        let (res, _) = vm.query(checksum, env, &msg, storage, meter, sm);
        let res = res.unwrap().unwrap();
        let balance: cw20::BalanceResponse = from_slice(&res).unwrap();
        balance.balance
    }
}
