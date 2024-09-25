use crate::prelude::*;

use std::sync::Arc;

#[derive(Clone)]
pub struct SigningLoggerMiddlewareMapBody {
    pub logger_fn: Arc<dyn Fn(&proto::TxBody) + Send + Sync>,
}
impl SigningLoggerMiddlewareMapBody {
    pub fn new<F>(logger_fn: F) -> Self
    where
        F: Fn(&proto::TxBody) + Send + Sync + 'static,
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
    pub async fn map_body(&self, body: proto::TxBody) -> Result<proto::TxBody> {
        (self.logger_fn)(&body);
        Ok(body)
    }
}

pub struct SigningLoggerMiddlewareMapResp {
    pub logger_fn: Arc<dyn Fn(&proto::TxResponse) + Send + Sync>,
}
impl SigningLoggerMiddlewareMapResp {
    pub fn new<F>(logger_fn: F) -> Self
    where
        F: Fn(&proto::TxResponse) + Send + Sync + 'static,
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
    pub async fn map_resp(&self, resp: proto::TxResponse) -> Result<proto::TxResponse> {
        (self.logger_fn)(&resp);
        Ok(resp)
    }
}
