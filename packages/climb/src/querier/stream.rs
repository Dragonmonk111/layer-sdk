use std::time::Duration;

use super::QueryClient;
use anyhow::Result;
use futures::Stream;

pub struct BlockEvents {
    pub height: u64,
    pub events: Vec<tendermint::abci::Event>,
}

impl QueryClient {
    pub async fn stream_block_events(
        &self,
        sleep_duration: Option<Duration>,
    ) -> Result<impl Stream<Item = Result<BlockEvents>> + '_> {
        let start_height = self.block_height().await?;

        Ok(futures::stream::unfold(
            start_height,
            move |block_height| async move {
                match self
                    .wait_until_block_height(block_height, sleep_duration)
                    .await
                {
                    Ok(_) => match self.fetch_block_events(block_height).await {
                        Err(err) => Some((Err(err), block_height)),
                        Ok(events) => Some((
                            Ok(BlockEvents {
                                height: block_height,
                                events,
                            }),
                            block_height + 1,
                        )),
                    },
                    Err(err) => Some((Err(err), block_height)),
                }
            },
        ))
    }
}
