use cosmwasm_std::Addr;
use cw_orch_core::environment::StateInterface;
use cw_orch_core::CwEnvError;

#[derive(Clone)]
pub struct TestState {}

impl StateInterface for TestState {
    fn get_address(&self, _contract_id: &str) -> Result<Addr, CwEnvError> {
        unimplemented!()
    }

    fn set_address(&mut self, _contract_id: &str, _address: &Addr) {
        unimplemented!()
    }

    fn get_code_id(&self, _contract_id: &str) -> Result<u64, CwEnvError> {
        unimplemented!()
    }

    fn set_code_id(&mut self, _contract_id: &str, _code_id: u64) {
        unimplemented!()
    }

    fn get_all_addresses(&self) -> Result<std::collections::HashMap<String, Addr>, CwEnvError> {
        unimplemented!()
    }

    fn get_all_code_ids(&self) -> Result<std::collections::HashMap<String, u64>, CwEnvError> {
        unimplemented!()
    }
}
