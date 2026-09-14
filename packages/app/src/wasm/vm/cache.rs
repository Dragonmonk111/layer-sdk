use std::{collections::HashSet, fmt};

use cosmwasm_std::{Binary, Env, MessageInfo, Reply, Response};
use cosmwasm_std::Checksum;
use cosmwasm_vm::{
    call_execute, call_instantiate, call_migrate, call_query, call_reply, call_sudo,
    AnalysisReport, Cache, CacheOptions, InstanceOptions, Size, VmError,
};
use layer_std::{AccountId, GasMeter};
use layer_storage::{AppMeter, ReadonlyStorage, ScratchTx, Storage, WeakSubTx};

use crate::{wasm::keeper::contract_storage, StateMachine};

use super::backend::{make_backend, out_of_gas, VmApi, VmQuerier, VmStore};

const DEFAULT_CACHE_MB: usize = 500;
const DEFAULT_INSTANCE_MB: usize = 32;
const CAPABILITIES: &[&str] = &[
    "iterator",
    "staking",
    "stargate",
    "cosmwasm_1_1",
    "cosmwasm_1_2",
    "cosmwasm_1_3",
    "cosmwasm_1_4",
    "cosmwasm_2_0",
    // cosmwasm-std 2.3.2 emits requires_cosmwasm_2_1 / requires_cosmwasm_2_2
    // markers via feature unification; the 2.3.2 VM supports the full 2.x API
    // surface, so advertise them.
    "cosmwasm_2_1",
    "cosmwasm_2_2",
    // JunoClaw extension: BN254 (alt_bn128) host functions, ported into
    // lib/cosmwasm (v2.3.2 fork). Contracts built with the cosmwasm_2_3
    // feature emit `requires_cosmwasm_2_3` and import env.bn254_*.
    "cosmwasm_2_3",
];

// Changed by 1000 in 2.0 upgrade: https://github.com/CosmWasm/cosmwasm/pull/1884
const SDK_TO_WASMER_GAS_FACTOR: u64 = 150_000;

pub fn sdk_gas_to_wasmer(gas: u64) -> u64 {
    gas.saturating_mul(SDK_TO_WASMER_GAS_FACTOR)
}

pub fn wasmer_gas_to_sdk(gas: u64) -> u64 {
    gas / SDK_TO_WASMER_GAS_FACTOR
}

// DETERMINISM-SAFE: capabilities() is not called in certify/verify paths, only at VM
// instantiation (cache init). The HashSet returned here feeds into cosmwasm_vm's structural
// capability check — it is not iterated for consensus-affecting output. cosmwasm_vm's
// CacheOptions::new() requires impl Into<HashSet<String>>, so HashSet is kept at this boundary.
fn capabilities() -> HashSet<String> {
    CAPABILITIES.iter().map(|s| s.to_string()).collect()
}

pub struct VmCache {
    cache: Cache<VmApi, VmStore, VmQuerier>,
}

impl fmt::Debug for VmCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VmCache").finish()
    }
}

impl VmCache {
    // TODO: make more args?
    pub fn init(cache_dir: &str) -> Self {
        let cache_options = CacheOptions::new(
            cache_dir,
            capabilities(),
            Size::mebi(DEFAULT_CACHE_MB),
            Size::mebi(DEFAULT_INSTANCE_MB),
        );
        let cache = unsafe { Cache::new(cache_options).unwrap() };
        VmCache { cache }
    }

    pub fn store_code(&self, wasm: &[u8]) -> Result<(Checksum, AnalysisReport), VmError> {
        let checksum = self.cache.store_code(wasm, true, true)?;
        let analysis = self.cache.analyze(&checksum)?;
        Ok((checksum, analysis))
    }

    #[allow(dead_code)]
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
        global_storage: &mut dyn Storage,
        contract: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (
        Result<Result<Response<super::CustomMsg>, String>, VmError>,
        u64,
    ) {
        let gas_limit = sdk_gas_to_wasmer(meter.remaining());
        let options = InstanceOptions { gas_limit };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let query = global_storage.as_ref();
        let mut wrap = WeakSubTx::new(query);
        let mut working = contract_storage(&mut wrap, contract);

        // SAFETY: backend consumed within this function; all pointed-to data lives on this stack frame
        let backend = unsafe { make_backend(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_instantiate(&mut instance, env, info, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = wasmer_gas_to_sdk(instance.create_gas_report().used_internally);

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = wrap.prepare();
                if let Err(e) = ops.commit(global_storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        let _ = instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        checksum: &Checksum,
        env: &Env,
        info: &MessageInfo,
        msg: &[u8],
        global_storage: &mut dyn Storage,
        contract: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (
        Result<Result<Response<super::CustomMsg>, String>, VmError>,
        u64,
    ) {
        let gas_limit = sdk_gas_to_wasmer(meter.remaining());
        let options = InstanceOptions { gas_limit };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let query = global_storage.as_ref();
        let mut wrap = WeakSubTx::new(query);
        let mut working = contract_storage(&mut wrap, contract);

        // SAFETY: backend consumed within this function; all pointed-to data lives on this stack frame
        let backend = unsafe { make_backend(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_execute(&mut instance, env, info, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = wasmer_gas_to_sdk(instance.create_gas_report().used_internally);

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = wrap.prepare();
                if let Err(e) = ops.commit(global_storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        let _ = instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn migrate(
        &self,
        checksum: &Checksum,
        env: &Env,
        msg: &[u8],
        global_storage: &mut dyn Storage,
        contract: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (
        Result<Result<Response<super::CustomMsg>, String>, VmError>,
        u64,
    ) {
        let gas_limit = sdk_gas_to_wasmer(meter.remaining());
        let options = InstanceOptions { gas_limit };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let query = global_storage.as_ref();
        let mut wrap = WeakSubTx::new(query);
        let mut working = contract_storage(&mut wrap, contract);

        // SAFETY: backend consumed within this function; all pointed-to data lives on this stack frame
        let backend = unsafe { make_backend(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_migrate(&mut instance, env, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = wasmer_gas_to_sdk(instance.create_gas_report().used_internally);

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = wrap.prepare();
                if let Err(e) = ops.commit(global_storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        let _ = instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sudo(
        &self,
        checksum: &Checksum,
        env: &Env,
        msg: &[u8],
        global_storage: &mut dyn Storage,
        contract: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (
        Result<Result<Response<super::CustomMsg>, String>, VmError>,
        u64,
    ) {
        let gas_limit = sdk_gas_to_wasmer(meter.remaining());
        let options = InstanceOptions { gas_limit };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let query = global_storage.as_ref();
        let mut wrap = WeakSubTx::new(query);
        let mut working = contract_storage(&mut wrap, contract);

        // SAFETY: backend consumed within this function; all pointed-to data lives on this stack frame
        let backend = unsafe { make_backend(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_sudo(&mut instance, env, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = wasmer_gas_to_sdk(instance.create_gas_report().used_internally);

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = wrap.prepare();
                if let Err(e) = ops.commit(global_storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        let _ = instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn reply(
        &self,
        checksum: &Checksum,
        env: &Env,
        reply: &Reply,
        global_storage: &mut dyn Storage,
        contract: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (
        Result<Result<Response<super::CustomMsg>, String>, VmError>,
        u64,
    ) {
        let gas_limit = sdk_gas_to_wasmer(meter.remaining());
        let options = InstanceOptions { gas_limit };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let query = global_storage.as_ref();
        let mut wrap = WeakSubTx::new(query);
        let mut working = contract_storage(&mut wrap, contract);

        // SAFETY: backend consumed within this function; all pointed-to data lives on this stack frame
        let backend = unsafe { make_backend(sm, &mut working, query, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_reply(&mut instance, env, reply);
        let result = result.map(|x| x.into_result());
        let gas_used = wasmer_gas_to_sdk(instance.create_gas_report().used_internally);

        // commit or abort the open WeakSubTx
        match &result {
            Ok(Ok(_)) => {
                let ops = wrap.prepare();
                if let Err(e) = ops.commit(global_storage, meter).map_err(out_of_gas) {
                    return (Err(e.into()), gas_used);
                }
            }
            _ => working.abort(),
        };
        let _ = instance.recycle();

        (result, gas_used)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn query(
        &self,
        checksum: &Checksum,
        env: &Env,
        msg: &[u8],
        global_storage: &dyn ReadonlyStorage,
        contract_addr: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> (Result<Result<Binary, String>, VmError>, u64) {
        let gas_limit = sdk_gas_to_wasmer(meter.remaining());
        let options = InstanceOptions { gas_limit };

        // Create WeakSubTx that only holds readable access, so we can query underlying storage as contract is working
        let mut scratch = ScratchTx::new(global_storage);
        let mut unmetered = contract_storage(&mut scratch, contract_addr);
        let mut contract = AppMeter::new(&mut unmetered);

        // SAFETY: backend consumed within this function; all pointed-to data lives on this stack frame
        let backend =
            unsafe { make_backend(sm, &mut contract, global_storage, meter, &env.block) };

        let mut instance = match self.cache.get_instance(checksum, backend, options) {
            Ok(i) => i,
            // No gas used yet
            Err(e) => return (Err(e), 0),
        };

        // execute the contract and get gas_used
        instance.set_storage_readonly(false);
        let result = call_query(&mut instance, env, msg);
        let result = result.map(|x| x.into_result());
        let gas_used = wasmer_gas_to_sdk(instance.create_gas_report().used_internally);

        // always abort scratch, as we don't want to commit anything
        scratch.abort();
        let _ = instance.recycle();

        (result, gas_used)
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{
        coin, from_json,
        testing::{mock_env, mock_info},
        to_json_vec, Order, Uint128,
    };
    use cw20::Cw20Coin;
    use layer_std::AccountId;
    use layer_storage::{MemoryStore, PersistentStorage};

    use crate::AppConfig;

    use super::*;

    // v1.0.1
    const CW20_BASE: &[u8] = include_bytes!("../../../fixtures/cw20_base.wasm");

    #[test]
    fn can_instatiate() {
        let path = "/tmp/slay3r/test-can-instantiate";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let vm = VmCache::init(path);
        let (checksum, _) = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let sender = AccountId::unchecked("Sillyness");
        let contract = AccountId::unchecked("My first cw20");
        let info = mock_info(&sender.to_string(), &[coin(55_000, "ujclaw")]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();

        let msg = cw20_base::msg::InstantiateMsg {
            name: "slayer".to_string(),
            symbol: "SLAY".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: sender.to_string(),
                amount: Uint128::new(1234567),
            }],
            mint: None,
            marketing: None,
        };
        let msg = to_json_vec(&msg).unwrap();

        let mut writer = store.writer();
        let (res, gas_used) = vm.instantiate(
            &checksum,
            &env,
            &info,
            &msg,
            &mut writer,
            &contract,
            &meter,
            &sm,
        );
        let res = res.unwrap().unwrap();
        assert_eq!(res.messages.len(), 0);
        assert_eq!(res.events.len(), 0);
        assert_eq!(res.attributes.len(), 0);
        assert_eq!(gas_used, 139);

        // query the state was written - token_info and total supply
        let num = writer
            .range(&meter, None, None, Order::Ascending)
            .unwrap()
            .count();
        assert_eq!(num, 3);
    }

    #[test]
    fn happy_path_create_send_query() {
        let path = "/tmp/slay3r/test-happy-path-create-send-query";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let mut vm = VmCache::init(path);
        let (checksum, _) = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let sender = AccountId::unchecked("Sillyness");
        let contract = AccountId::unchecked("My Token");
        let info = mock_info(&sender.to_string(), &[]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();
        let mut writer = store.writer();

        // instantiate
        let msg = cw20_base::msg::InstantiateMsg {
            name: "slayer".to_string(),
            symbol: "SLAY".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: sender.to_string(),
                amount: Uint128::new(1234567),
            }],
            mint: None,
            marketing: None,
        };
        let msg = to_json_vec(&msg).unwrap();
        let (res, _) = vm.instantiate(
            &checksum,
            &env,
            &info,
            &msg,
            &mut writer,
            &contract,
            &meter,
            &sm,
        );
        let _ = res.unwrap().unwrap();

        // query two addresses
        let rcpt = AccountId::unchecked("Bystander");
        let hero = query_balance(
            &mut vm,
            &checksum,
            &env,
            &sender,
            writer.as_ref(),
            &contract,
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
            &contract,
            &meter,
            &sm,
        );
        assert_eq!(zero.u128(), 0u128);

        // send some surpise tokens
        let msg = cw20_base::msg::ExecuteMsg::Transfer {
            recipient: rcpt.to_string(),
            amount: Uint128::new(23456),
        };
        let msg = to_json_vec(&msg).unwrap();
        let (res, _) = vm.execute(
            &checksum,
            &env,
            &info,
            &msg,
            &mut writer,
            &contract,
            &meter,
            &sm,
        );
        let _ = res.unwrap().unwrap();

        // query two addresses wirh new balances
        let rcpt = AccountId::unchecked("Bystander");
        let hero = query_balance(
            &mut vm,
            &checksum,
            &env,
            &sender,
            writer.as_ref(),
            &contract,
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
            &contract,
            &meter,
            &sm,
        );
        assert_eq!(zero.u128(), 23456u128);
    }

    #[test]
    fn query_with_iterator() {
        let path = "/tmp/slay3r/test-query-with-iterator";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let vm = VmCache::init(path);
        let (checksum, _) = vm.store_code(CW20_BASE).unwrap();

        // try to instantiate
        let env = mock_env();
        let one = AccountId::unchecked("One");
        let two = AccountId::unchecked("Two");
        let three = AccountId::unchecked("Xyz");
        let contract = AccountId::unchecked("Another Token");
        let info = mock_info(&one.to_string(), &[]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();
        let mut writer = store.writer();

        // instantiate
        let msg = cw20_base::msg::InstantiateMsg {
            name: "slayer".to_string(),
            symbol: "SLAY".to_string(),
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
        let msg = to_json_vec(&msg).unwrap();
        let (res, _) = vm.instantiate(
            &checksum,
            &env,
            &info,
            &msg,
            &mut writer,
            &contract,
            &meter,
            &sm,
        );
        let _ = res.unwrap().unwrap();

        // now list all accounts
        let msg = cw20_base::msg::QueryMsg::AllAccounts {
            start_after: None,
            limit: None,
        };
        let msg = to_json_vec(&msg).unwrap();
        let (res, _) = vm.query(
            &checksum,
            &env,
            &msg,
            writer.as_ref(),
            &contract,
            &meter,
            &sm,
        );
        let res = res.unwrap().unwrap();
        let cw20::AllAccountsResponse { accounts } = from_json(res).unwrap();
        assert_eq!(
            accounts,
            vec![two.to_string(), one.to_string(), three.to_string()]
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn query_balance(
        vm: &mut VmCache,
        checksum: &Checksum,
        env: &Env,
        account: &AccountId,
        global_storage: &dyn ReadonlyStorage,
        contract_addr: &AccountId,
        meter: &GasMeter,
        sm: &StateMachine,
    ) -> Uint128 {
        let msg = cw20_base::msg::QueryMsg::Balance {
            address: account.to_string(),
        };
        let msg = to_json_vec(&msg).unwrap();
        let (res, _) = vm.query(
            checksum,
            env,
            &msg,
            global_storage,
            contract_addr,
            meter,
            sm,
        );
        let res = res.unwrap().unwrap();
        let balance: cw20::BalanceResponse = from_json(res).unwrap();
        balance.balance
    }

    // ── zk-verifier (BN254 / cosmwasm_2_3) gas measurement ────────────────
    //
    // The artifact is `zk-verifier` built with `--features bn254-precompile`,
    // which enables cosmwasm-std's `cosmwasm_2_3` feature. It imports the
    // `env.bn254_*` host functions and emits metered bulk-memory ops, so it
    // only loads on the JunoClaw VM (the `cosmwasm_2_3` capability plus the
    // Gatekeeper length-aware bulk-memory metering in engine.rs). The wasm is
    // read from `ZK_VERIFIER_WASM` if set, else the sibling-repo build output;
    // the test skips (rather than fails) when the artifact isn't built.
    const ZK_VERIFIER_WASM_REL: &str =
        "/../../../junoclaw/contracts/target/wasm32-unknown-unknown/release/zk_verifier.wasm";

    // Groth16 fixtures for SquareCircuit x=3 (y=9), deterministic seed 42 —
    // produced by `cargo +1.95 run -p zk-verifier --example generate_proof`.
    const ZK_VK_B64: &str = "+7zaLtkeRoJtpwW9qmVvnM8XKq8J4eHVdwckLWfnzZaAg/nPhzWQVrH2vuTqFiR063hioTHe3uRjRE64MCijL+vSasFvl7LH3WVrj24QNztXZ6xqgz+XjmeZzAjrEFQT5066KO8NcqkABvqGELowehGmtc/1Qh63BQPs6Te78iyp1Da2e2qJ8qz8L08tkmkeAmJr2iY5qp5vS/xqZMG+K36am/FhHf+1lMz2qDQ/4EPAlCG/widWopwiR8CpXxMBoios4XXu0EO6jWqsXyM2qv4A7AHJa7vQtc9ReNkeOSMCAAAAAAAAAI1L7LGb+PVNcIAlMlW0NpbNad3lEp7FxgdIbRJzrHWi4OxrTbfuKm+axq5NCSEJ4EjViyMziVRXwfsm6DTUWYE=";
    const ZK_PROOF_B64: &str = "3s0boCztqb+oTya4AmY0uHHYXNwiL8ZD/AAK+qlm7JM4BIMkNmkzUVFEYxjPdlo3qVghx4+MRZKaIEcB7hqNK7Ou+8+ZWzPzeU+MQgiWJ8T1IG1TkTd7m25N4yF9kG6LtYI2V1JjwxNFu20+9TtnPXHuBDBxoqgZfjJGbnut7as=";
    const ZK_INPUTS_B64: &str = "CQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    #[test]
    fn zk_verifier_bn254_gas() {
        let wasm_path = std::env::var("ZK_VERIFIER_WASM").unwrap_or_else(|_| {
            format!("{}{}", env!("CARGO_MANIFEST_DIR"), ZK_VERIFIER_WASM_REL)
        });
        let Ok(wasm) = std::fs::read(&wasm_path) else {
            eprintln!("skipping zk_verifier_bn254_gas: wasm not found at {wasm_path}");
            return;
        };

        let path = "/tmp/slay3r/test-zk-verifier-bn254";
        let _ = std::fs::remove_dir_all(path);
        std::fs::create_dir_all(path).unwrap();

        let vm = VmCache::init(path);
        let (checksum, analysis) = vm.store_code(&wasm).unwrap();
        // The artifact must declare the cosmwasm_2_3 capability it relies on.
        assert!(analysis
            .required_capabilities
            .contains("cosmwasm_2_3"));

        let env = mock_env();
        let sender = AccountId::unchecked("deployer");
        let contract = AccountId::unchecked("zk-verifier");
        let info = mock_info(&sender.to_string(), &[]);
        let meter = GasMeter::infinite();
        let sm = StateMachine::new(&AppConfig::new(path));
        let store = MemoryStore::new();
        let mut writer = store.writer();

        // instantiate {"admin":null} -> admin becomes the deployer
        let (res, g_inst) = vm.instantiate(
            &checksum,
            &env,
            &info,
            br#"{"admin":null}"#,
            &mut writer,
            &contract,
            &meter,
            &sm,
        );
        res.unwrap().unwrap();

        // store the verifying key (sender is admin)
        let store_vk =
            format!(r#"{{"store_vk":{{"vk_base64":"{ZK_VK_B64}"}}}}"#).into_bytes();
        let (res, g_vk) = vm.execute(
            &checksum, &env, &info, &store_vk, &mut writer, &contract, &meter, &sm,
        );
        res.unwrap().unwrap();

        // verify the proof — this is the path that exercises bn254_scalar_mul,
        // bn254_add and bn254_pairing_equality
        let verify = format!(
            r#"{{"verify_proof":{{"proof_base64":"{ZK_PROOF_B64}","public_inputs_base64":"{ZK_INPUTS_B64}"}}}}"#
        )
        .into_bytes();
        let (res, g_verify) = vm.execute(
            &checksum, &env, &info, &verify, &mut writer, &contract, &meter, &sm,
        );
        res.unwrap().unwrap();

        eprintln!(
            "zk-verifier bn254 gas: instantiate={g_inst} store_vk={g_vk} verify_proof={g_verify}"
        );
        // Pure-Wasm verification is ~371k SDK gas; the cosmwasm_2_3 host-fn
        // path targets ~187k. Assert a generous band so the test catches a
        // regression back to the pure-Wasm path without being flaky.
        assert!(
            g_verify < 300_000,
            "expected precompile verify_proof gas < 300k, got {g_verify}"
        );
    }
}
