pub mod logger;

use anyhow::Result;
use logger::{SigningLoggerMiddlewareMapBody, SigningLoggerMiddlewareMapResp};

use cosmos_sdk_proto::cosmos::{base::abci::v1beta1::TxResponse, tx::v1beta1::TxBody};

pub enum SigningMiddlewareMapBody {
    Logger(SigningLoggerMiddlewareMapBody),
}

impl SigningMiddlewareMapBody {
    pub async fn map_body(&self, req: TxBody) -> Result<TxBody> {
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
    pub async fn map_resp(&self, resp: TxResponse) -> Result<TxResponse> {
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
