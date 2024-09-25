use crate::prelude::*;

impl SigningClient {
    pub fn transfer_msg<'a>(
        &self,
        denom: impl Into<Option<&'a str>>,
        amount: u128,
        recipient: Address,
    ) -> Result<proto::bank::MsgSend> {
        let denom = denom.into().unwrap_or(&self.querier.chain_config.gas_denom);

        let amount = proto::Coin {
            amount: amount.to_string(),
            denom: denom.parse().map_err(|err| anyhow!("{}", err))?,
        };

        Ok(proto::bank::MsgSend {
            from_address: self.addr.to_string(),
            to_address: recipient.to_string(),
            amount: vec![amount],
        })
    }
}
