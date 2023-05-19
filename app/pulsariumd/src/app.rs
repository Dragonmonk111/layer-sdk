use core::panic;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{debug, info, instrument};

use tendermint_abci::Application;
use tendermint_proto::abci::{
    response_process_proposal, RequestApplySnapshotChunk, RequestCheckTx, RequestEcho,
    RequestFinalizeBlock, RequestInfo, RequestInitChain, RequestLoadSnapshotChunk,
    RequestOfferSnapshot, RequestPrepareProposal, RequestProcessProposal, RequestQuery,
    ResponseApplySnapshotChunk, ResponseCheckTx, ResponseCommit, ResponseEcho,
    ResponseFinalizeBlock, ResponseFlush, ResponseInfo, ResponseInitChain, ResponseListSnapshots,
    ResponseLoadSnapshotChunk, ResponseOfferSnapshot, ResponsePrepareProposal,
    ResponseProcessProposal, ResponseQuery,
};

use pulsar_app::{App, AppLoadError, StateMachine};
use pulsar_storage::{MemoryStore, PersistentStorage};

use crate::{
    decode::{
        decode_check_response, decode_finalize_response, decode_init_response,
        decode_query_response,
    },
    encode::{
        encode_check_request, encode_finalize_request, encode_init_request, encode_query_request,
    },
};

#[derive(Debug)]
pub struct Pulsarium<T: PersistentStorage + 'static> {
    // Applications are `Send` + `Clone` + `'static` because they are cloned for
    // each incoming connection to the ABCI [`Server`]. It is up to the
    // application developer to manage shared state between these clones of their
    // application.
    app: Arc<RwLock<App<T>>>,
}

impl Default for Pulsarium<MemoryStore> {
    fn default() -> Self {
        let store = MemoryStore::new();
        Self::new(store)
    }
}

impl<T: PersistentStorage + 'static> Clone for Pulsarium<T> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
        }
    }
}

unsafe impl<T: PersistentStorage> Send for Pulsarium<T> {}

impl<T: PersistentStorage + 'static> Pulsarium<T> {
    /// Creates a new app and tries to load state from storage.
    /// If the storage is empty, the app will be left in an uninitialized state
    /// and init_chain must be called before any other method.
    pub fn new(store: T) -> Self {
        let logic = StateMachine::new();
        let mut app = App::new(store, logic);
        match app.load_from_storage() {
            Ok(_) => {
                // FIXME: proper logging when we have proper tracing
                let height = app.info().height;
                println!("Initialized app from storage at height {}", height);
            }
            Err(AppLoadError::NoStoredState) => {
                // FIXME: proper logging when we have proper tracing
                println!("No stored state, app is uninitialized");
            }
            Err(e) => panic!("Error loading app from storage: {}", e),
        };

        Self {
            app: Arc::new(RwLock::new(app)),
        }
    }
}

impl<T: PersistentStorage + 'static> Application for Pulsarium<T> {
    #[instrument(skip_all)]
    fn echo(&self, request: RequestEcho) -> ResponseEcho {
        info!("abci echo");
        ResponseEcho {
            message: request.message,
        }
    }

    /// Provide information about the ABCI application.
    #[instrument(skip_all)]
    fn info(&self, _request: RequestInfo) -> ResponseInfo {
        info!("abci info");
        let app = self.app.read();
        let block = app.info();
        let app_hash = app.app_hash();
        ResponseInfo {
            data: format!("{} {}", env!("CARGO_BIN_NAME"), env!("CARGO_PKG_VERSION")),
            version: "".to_string(),
            app_version: 1,                         // FIXME: what to put here?
            last_block_height: block.height as i64, // ugly api :(
            last_block_app_hash: app_hash.into(),
        }
    }

    /// Called once upon genesis.
    #[instrument(skip_all)]
    fn init_chain(&self, request: RequestInitChain) -> ResponseInitChain {
        info!("abci init_chain");
        let request = encode_init_request(request);
        // This requires we are in WaitingInit state, otherwise panic
        let res = self.app.write().init(request).unwrap();
        decode_init_response(res)
    }

    /// Query the application for data at the current or past height.
    #[instrument(skip_all)]
    fn query(&self, request: RequestQuery) -> ResponseQuery {
        info!("abci query");
        let request = encode_query_request(request);
        let res = self.app.read().query(request);
        decode_query_response(res)
    }

    /// Check the given transaction before putting it into the local mempool.
    #[instrument(skip_all)]
    fn check_tx(&self, request: RequestCheckTx) -> ResponseCheckTx {
        info!("abci check_tx");
        let request = encode_check_request(request);
        let res = self.app.read().check_tx(request);
        decode_check_response(res)
    }

    #[instrument(skip_all)]
    fn finalize_block(&self, request: RequestFinalizeBlock) -> ResponseFinalizeBlock {
        info!("abci finalize_block");
        let request = encode_finalize_request(request);
        // FIXME: crash node on finalize block error?
        let res = self.app.write().finalize_block(request).unwrap();
        decode_finalize_response(res)
    }

    /// Signals that messages queued on the client should be flushed to the server.
    #[instrument(skip_all)]
    fn flush(&self) -> ResponseFlush {
        debug!("abci flush");
        ResponseFlush {}
    }

    /// Commit the current state at the current height.
    #[instrument(skip_all)]
    fn commit(&self) -> ResponseCommit {
        // Note: pulsar commits data in finalize_block. unsure why there is a different command,
        // and separating them causes issues with lifetimes and static analysis.
        info!("abci commit");
        // TODO: get app data
        Default::default()
    }

    /// Used during state sync to discover available snapshots on peers.
    #[instrument(skip_all)]
    fn list_snapshots(&self) -> ResponseListSnapshots {
        info!("abci list_snapshots");
        Default::default()
    }

    /// Called when bootstrapping the node using state sync.
    #[instrument(skip_all)]
    fn offer_snapshot(&self, _request: RequestOfferSnapshot) -> ResponseOfferSnapshot {
        info!("abci offer_snapshot");
        Default::default()
    }

    /// Used during state sync to retrieve chunks of snapshots from peers.
    #[instrument(skip_all)]
    fn load_snapshot_chunk(&self, _request: RequestLoadSnapshotChunk) -> ResponseLoadSnapshotChunk {
        info!("abci load_snapshot_chunk");
        Default::default()
    }

    /// Apply the given snapshot chunk to the application's state.
    #[instrument(skip_all)]
    fn apply_snapshot_chunk(
        &self,
        _request: RequestApplySnapshotChunk,
    ) -> ResponseApplySnapshotChunk {
        info!("abci apply_snapshot_chunk");
        Default::default()
    }

    /// A stage where the application can modify the list of transactions
    /// in the preliminary proposal.
    ///
    /// The default implementation implements the required behavior in a
    /// very naive way, removing transactions off the end of the list
    /// until the limit on the total size of the transaction is met as
    /// specified in the `max_tx_bytes` field of the request, or there are
    /// no more transactions. It's up to the application to implement
    /// more elaborate removal strategies.
    ///
    /// This method is introduced in ABCI++.
    #[instrument(skip_all)]
    fn prepare_proposal(&self, request: RequestPrepareProposal) -> ResponsePrepareProposal {
        info!("abci prepare_proposal");
        // Per the ABCI++ spec: if the size of RequestPrepareProposal.txs is
        // greater than RequestPrepareProposal.max_tx_bytes, the Application
        // MUST remove transactions to ensure that the
        // RequestPrepareProposal.max_tx_bytes limit is respected by those
        // transactions returned in ResponsePrepareProposal.txs.
        let RequestPrepareProposal {
            mut txs,
            max_tx_bytes,
            ..
        } = request;
        let max_tx_bytes: usize = max_tx_bytes.try_into().unwrap_or(0);
        let mut total_tx_bytes: usize = txs
            .iter()
            .map(|tx| tx.len())
            .fold(0, |acc, len| acc.saturating_add(len));
        while total_tx_bytes > max_tx_bytes {
            if let Some(tx) = txs.pop() {
                total_tx_bytes = total_tx_bytes.saturating_sub(tx.len());
            } else {
                break;
            }
        }
        ResponsePrepareProposal { txs }
    }

    /// A stage where the application can accept or reject the proposed block.
    ///
    /// The default implementation returns the status value of `ACCEPT`.
    ///
    /// This method is introduced in ABCI++.
    #[instrument(skip_all)]
    fn process_proposal(&self, _request: RequestProcessProposal) -> ResponseProcessProposal {
        info!("abci process_proposal");
        ResponseProcessProposal {
            status: response_process_proposal::ProposalStatus::Accept as i32,
        }
    }
}
