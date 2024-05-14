use abstract_cw_multi_test::AppResponse;

use cw_orch_core::environment::{NodeQuerier, Querier};

use crate::Slay3rTube;

pub struct Slay3rNode {
    tube: Slay3rTube,
}

impl Slay3rNode {
    pub fn new(tube: &Slay3rTube) -> Self {
        Self { tube: tube.clone() }
    }
}

impl Querier for Slay3rNode {
    type Error = slay3r_app::PulsarError;
}

impl NodeQuerier for Slay3rNode {
    // TODO: implement IndexResponse
    type Response = AppResponse;

    fn latest_block(&self) -> Result<cosmwasm_std::BlockInfo, Self::Error> {
        Ok(self.tube.block_info())
    }

    fn block_height(&self) -> Result<u64, Self::Error> {
        let info = self.tube.block_info();
        Ok(info.height)
    }

    fn block_time(&self) -> Result<u128, Self::Error> {
        let info = self.tube.block_info();
        Ok(info.time.nanos().into())
    }

    fn simulate_tx(&self, tx_bytes: Vec<u8>) -> Result<u64, Self::Error> {
        todo!()
    }

    fn block_by_height(&self, height: u64) -> Result<cosmwasm_std::BlockInfo, Self::Error> {
        unimplemented!("Not supported")
    }

    fn find_tx(&self, hash: String) -> Result<Self::Response, Self::Error> {
        unimplemented!("Not supported")
    }
}
