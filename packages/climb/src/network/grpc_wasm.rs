use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
use tonic_web_wasm_client::Client;

use crate::ChainConfig;

static GRPC_CLIENT_CACHE: LazyLock<GrpcClientCache> = LazyLock::new(GrpcClientCache::new);

struct GrpcClientCache {
    clients: Mutex<HashMap<String, Client>>,
}

impl GrpcClientCache {
    fn new() -> Self {
        Self {
            clients: Mutex::new(HashMap::new()),
        }
    }
}

pub trait ChainConfigGrpcExt {
    fn get_grpc_client(&self) -> impl std::future::Future<Output = Result<Client>> + Send;
}

impl ChainConfigGrpcExt for ChainConfig {
    async fn get_grpc_client(&self) -> Result<Client> {
        // try to get the channel from the cache
        let client = {
            // give the lock its own scope so it can be definitively dropped before the await
            let lock = GRPC_CLIENT_CACHE.clients.lock().unwrap();
            lock.get(&self.grpc_endpoint).cloned()
        };

        match client {
            Some(client) => Ok(client),
            None => {
                let client = Client::new(self.grpc_endpoint.clone());

                GRPC_CLIENT_CACHE
                    .clients
                    .lock()
                    .unwrap()
                    .insert(self.grpc_endpoint.clone(), client.clone());

                Ok(client)
            }
        }
    }
}
