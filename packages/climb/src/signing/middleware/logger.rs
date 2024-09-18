use anyhow::Result;
use std::sync::Arc;

use cosmos_sdk_proto::cosmos::{base::abci::v1beta1::TxResponse, tx::v1beta1::TxBody};

#[derive(Clone)]
pub struct SigningLoggerMiddlewareMapBody {
    pub logger_fn: Arc<dyn Fn(&TxBody) + Send + Sync>,
}
impl SigningLoggerMiddlewareMapBody {
    pub fn new<F>(logger_fn: F) -> Self
    where
        F: Fn(&TxBody) + Send + Sync + 'static,
    {
        Self {
            logger_fn: Arc::new(logger_fn),
        }
    }
}
impl Default for SigningLoggerMiddlewareMapBody {
    fn default() -> Self {
        Self::new(|body| eprintln!("{:?}", body))
    }
}

impl SigningLoggerMiddlewareMapBody {
    pub async fn map_body(&self, body: TxBody) -> Result<TxBody> {
        (self.logger_fn)(&body);
        Ok(body)
    }
}

pub struct SigningLoggerMiddlewareMapResp {
    pub logger_fn: Arc<dyn Fn(&TxResponse) + Send + Sync>,
}
impl SigningLoggerMiddlewareMapResp {
    pub fn new<F>(logger_fn: F) -> Self
    where
        F: Fn(&TxResponse) + Send + Sync + 'static,
    {
        Self {
            logger_fn: Arc::new(logger_fn),
        }
    }
}
impl Default for SigningLoggerMiddlewareMapResp {
    fn default() -> Self {
        Self::new(|resp| eprintln!("{:?}", resp))
    }
}

impl SigningLoggerMiddlewareMapResp {
    pub async fn map_resp(&self, resp: TxResponse) -> Result<TxResponse> {
        (self.logger_fn)(&resp);
        Ok(resp)
    }
}
