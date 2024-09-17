use anyhow::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct SigningLoggerMiddlewareMapBody {
    pub logger_fn: Arc<dyn Fn(&cosmrs::tx::Body) + Send + Sync>,
}
impl SigningLoggerMiddlewareMapBody {
    pub fn new<F>(logger_fn: F) -> Self
    where
        F: Fn(&cosmrs::tx::Body) + Send + Sync + 'static,
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
    pub async fn map_body(&self, body: cosmrs::tx::Body) -> Result<cosmrs::tx::Body> {
        (self.logger_fn)(&body);
        Ok(body)
    }
}

pub struct SigningLoggerMiddlewareMapResp {
    pub logger_fn:
        Arc<dyn Fn(&cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse) + Send + Sync>,
}
impl SigningLoggerMiddlewareMapResp {
    pub fn new<F>(logger_fn: F) -> Self
    where
        F: Fn(&cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse) + Send + Sync + 'static,
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
    pub async fn map_resp(
        &self,
        resp: cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        (self.logger_fn)(&resp);
        Ok(resp)
    }
}
