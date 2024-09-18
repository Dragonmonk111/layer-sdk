use crate::Address;

use super::{QueryClient, QueryRequest};
use anyhow::{anyhow, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

impl QueryClient {
    pub async fn contract_smart<'a, D: DeserializeOwned + Send + std::fmt::Debug + Sync>(
        &self,
        address: &Address,
        msg: ContractMessage<'a, impl Serialize>,
    ) -> Result<D> {
        self.run_with_middleware(ContractSmartReq {
            address: address.clone(),
            msg: msg.try_into_vec()?,
            _phantom: std::marker::PhantomData,
        })
        .await
    }
    pub async fn contract_smart_raw_response<'a>(
        &self,
        address: &Address,
        msg: ContractMessage<'a, impl Serialize>,
    ) -> Result<Vec<u8>> {
        self.run_with_middleware(ContractSmartRawReq {
            address: address.clone(),
            msg: msg.try_into_vec()?,
        })
        .await
    }

    pub async fn contract_code_info(
        &self,
        code_id: u64,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::CodeInfoResponse> {
        self.run_with_middleware(ContractCodeInfoReq { code_id })
            .await
    }
    pub async fn contract_info(
        &self,
        address: &Address,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::QueryContractInfoResponse> {
        self.run_with_middleware(ContractInfoReq {
            address: address.clone(),
        })
        .await
    }
}

#[derive(Debug)]
struct ContractSmartReq<D> {
    pub address: Address,
    pub msg: Vec<u8>,
    _phantom: std::marker::PhantomData<D>,
}

impl<D> Clone for ContractSmartReq<D> {
    fn clone(&self) -> Self {
        Self {
            address: self.address.clone(),
            msg: self.msg.clone(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<D: DeserializeOwned + Send + std::fmt::Debug + Sync> QueryRequest for ContractSmartReq<D> {
    type QueryResponse = D;

    async fn request(&self, client: QueryClient) -> Result<D> {
        let res = ContractSmartRawReq {
            address: self.address.clone(),
            msg: self.msg.clone(),
        }
        .request(client)
        .await?;

        let res = cosmwasm_std::from_json(res).context("couldn't deserialize response")?;

        Ok(res)
    }
}

#[derive(Clone, Debug)]
struct ContractSmartRawReq {
    pub address: Address,
    pub msg: Vec<u8>,
}

impl QueryRequest for ContractSmartRawReq {
    type QueryResponse = Vec<u8>;

    async fn request(&self, client: QueryClient) -> Result<Vec<u8>> {
        let mut query_client = cosmrs::proto::cosmwasm::wasm::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let res = query_client
            .smart_contract_state(
                cosmrs::proto::cosmwasm::wasm::v1::QuerySmartContractStateRequest {
                    address: self.address.to_string(),
                    query_data: self.msg.clone(),
                },
            )
            .await
            .map(|res| res.into_inner())?;

        Ok(res.data)
    }
}

#[derive(Clone, Debug)]
struct ContractCodeInfoReq {
    pub code_id: u64,
}

impl QueryRequest for ContractCodeInfoReq {
    type QueryResponse = cosmrs::proto::cosmwasm::wasm::v1::CodeInfoResponse;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::CodeInfoResponse> {
        let mut query_client = cosmrs::proto::cosmwasm::wasm::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let res = query_client
            .code(cosmrs::proto::cosmwasm::wasm::v1::QueryCodeRequest {
                code_id: self.code_id,
            })
            .await
            .map(|res| res.into_inner())?;

        res.code_info.context("no code info found")
    }
}

#[derive(Clone, Debug)]
pub struct ContractInfoReq {
    pub address: Address,
}

impl QueryRequest for ContractInfoReq {
    type QueryResponse = cosmrs::proto::cosmwasm::wasm::v1::QueryContractInfoResponse;

    async fn request(
        &self,
        client: QueryClient,
    ) -> Result<cosmrs::proto::cosmwasm::wasm::v1::QueryContractInfoResponse> {
        let mut query_client = cosmrs::proto::cosmwasm::wasm::v1::query_client::QueryClient::new(
            client.grpc_channel.clone(),
        );

        let res = query_client
            .contract_info(
                cosmrs::proto::cosmwasm::wasm::v1::QueryContractInfoRequest {
                    address: self.address.to_string(),
                },
            )
            .await
            .map(|res| res.into_inner())?;

        Ok(res)
    }
}

#[derive(Clone)]
pub enum ContractMessage<'a, T: Serialize> {
    SerdeRef(&'a T),
    SerdeOwned(T),
    Raw(Vec<u8>),
    Empty,
}

// The typical use-cases are for converting any serializable type into a ContractMessage
impl<'a, T: Serialize> From<T> for ContractMessage<'a, T> {
    fn from(s: T) -> Self {
        ContractMessage::SerdeOwned(s)
    }
}

impl<'a, T: Serialize> From<&'a T> for ContractMessage<'a, T> {
    fn from(s: &'a T) -> Self {
        ContractMessage::SerdeRef(s)
    }
}

impl<'a, T: Serialize> ContractMessage<'a, T> {
    pub fn new_serde_ref(s: &'a T) -> Self {
        ContractMessage::SerdeRef(s)
    }
    pub fn new_serde_owned(s: T) -> Self {
        ContractMessage::SerdeOwned(s)
    }
}

// But - sometimes we don't want to convert it as json, we just want to pass it raw
impl<'a> ContractMessage<'a, Vec<u8>> {
    pub fn new_raw(s: Vec<u8>) -> Self {
        ContractMessage::Raw(s)
    }

    pub fn new_raw_str(s: impl AsRef<str>) -> Self {
        ContractMessage::Raw(s.as_ref().as_bytes().to_vec())
    }
}

impl<'a, T: Serialize> ContractMessage<'a, T> {
    pub fn try_into_vec(self) -> Result<Vec<u8>> {
        match self {
            ContractMessage::SerdeRef(s) => {
                cosmwasm_std::to_json_vec(s).map_err(|err| anyhow!("{}", err))
            }
            ContractMessage::SerdeOwned(s) => {
                cosmwasm_std::to_json_vec(&s).map_err(|err| anyhow!("{}", err))
            }
            ContractMessage::Raw(s) => Ok(s),
            ContractMessage::Empty => Ok(b"{}".to_vec()),
        }
    }

    pub fn try_into_string(self) -> Result<String> {
        match self {
            ContractMessage::SerdeRef(s) => {
                cosmwasm_std::to_json_string(s).map_err(|err| anyhow!("{}", err))
            }
            ContractMessage::SerdeOwned(s) => {
                cosmwasm_std::to_json_string(&s).map_err(|err| anyhow!("{}", err))
            }
            ContractMessage::Raw(s) => String::from_utf8(s).map_err(|err| anyhow!("{}", err)),
            ContractMessage::Empty => Ok("{}".to_string()),
        }
    }
}
