use cosmwasm_std::Coin;

use cw_orch_core::environment::{NodeQuerier, Querier};

pub struct Slay3rNode {}

impl Querier for Slay3rNode {
    type Error = slay3r_app::PulsarError;
}

impl NodeQuerier for Slay3rNode {
    // TODO: implement IndexResponse
    type Response = ();

    fn latest_block(&self) -> Result<cosmwasm_std::BlockInfo, Self::Error> {
        todo!()
    }

    fn block_by_height(&self, height: u64) -> Result<cosmwasm_std::BlockInfo, Self::Error> {
        todo!()
    }

    fn block_height(&self) -> Result<u64, Self::Error> {
        todo!()
    }

    fn block_time(&self) -> Result<u128, Self::Error> {
        todo!()
    }

    fn simulate_tx(&self, tx_bytes: Vec<u8>) -> Result<u64, Self::Error> {
        todo!()
    }

    fn find_tx(&self, hash: String) -> Result<Self::Response, Self::Error> {
        todo!()
    }
}
