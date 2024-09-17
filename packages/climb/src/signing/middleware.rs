pub mod logger;

use anyhow::Result;
use logger::{SigningLoggerMiddlewareMapBody, SigningLoggerMiddlewareMapResp};

pub enum SigningMiddlewareMapBody {
    Logger(SigningLoggerMiddlewareMapBody),
}

impl SigningMiddlewareMapBody {
    pub async fn map_body(&self, req: cosmrs::tx::Body) -> Result<cosmrs::tx::Body> {
        match self {
            Self::Logger(m) => m.map_body(req).await,
        }
    }
    pub fn default_list() -> Vec<Self> {
        vec![
            //Self::Logger(SigningLoggerMiddlewareMapBody::default()),
        ]
    }
}

pub enum SigningMiddlewareMapResp {
    Logger(SigningLoggerMiddlewareMapResp),
}

impl SigningMiddlewareMapResp {
    pub async fn map_resp(
        &self,
        resp: cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse,
    ) -> Result<cosmrs::proto::cosmos::base::abci::v1beta1::TxResponse> {
        match self {
            Self::Logger(m) => m.map_resp(resp).await,
        }
    }
    pub fn default_list() -> Vec<Self> {
        vec![
            //Self::Logger(SigningLoggerMiddlewareMapResp::default()),
        ]
    }
}
