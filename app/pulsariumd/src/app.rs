use tracing::{debug, info, instrument};

use tendermint_abci::Application;

use tendermint_proto::v0_37::abci::{
    response_process_proposal, RequestApplySnapshotChunk, RequestBeginBlock, RequestCheckTx,
    RequestDeliverTx, RequestEcho, RequestEndBlock, RequestInfo, RequestInitChain,
    RequestLoadSnapshotChunk, RequestOfferSnapshot, RequestPrepareProposal, RequestProcessProposal,
    RequestQuery, ResponseApplySnapshotChunk, ResponseBeginBlock, ResponseCheckTx, ResponseCommit,
    ResponseDeliverTx, ResponseEcho, ResponseEndBlock, ResponseFlush, ResponseInfo,
    ResponseInitChain, ResponseListSnapshots, ResponseLoadSnapshotChunk, ResponseOfferSnapshot,
    ResponsePrepareProposal, ResponseProcessProposal, ResponseQuery,
};

#[derive(Default, Debug)]
pub struct Pulsarium {
    // Applications are `Send` + `Clone` + `'static` because they are cloned for
    // each incoming connection to the ABCI [`Server`]. It is up to the
    // application developer to manage shared state between these clones of their
    // application.
}

impl Clone for Pulsarium {
    fn clone(&self) -> Self {
        debug!("clone Pulsarium");
        Pulsarium::default()
    }
}

unsafe impl Send for Pulsarium {}

impl Application for Pulsarium {
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
        Default::default()
    }

    /// Called once upon genesis.
    #[instrument(skip_all)]
    fn init_chain(&self, _request: RequestInitChain) -> ResponseInitChain {
        info!("abci init_chain");
        Default::default()
    }

    /// Query the application for data at the current or past height.
    #[instrument(skip_all)]
    fn query(&self, _request: RequestQuery) -> ResponseQuery {
        info!("abci query");
        Default::default()
    }

    /// Check the given transaction before putting it into the local mempool.
    #[instrument(skip_all)]
    fn check_tx(&self, _request: RequestCheckTx) -> ResponseCheckTx {
        info!("abci check_tx");
        Default::default()
    }

    /// Signals the beginning of a new block, prior to any `DeliverTx` calls.
    #[instrument(skip_all)]
    fn begin_block(&self, _request: RequestBeginBlock) -> ResponseBeginBlock {
        info!("abci begin_block");
        Default::default()
    }

    /// Apply a transaction to the application's state.
    #[instrument(skip_all)]
    fn deliver_tx(&self, _request: RequestDeliverTx) -> ResponseDeliverTx {
        info!("abci deliver_tx");
        Default::default()
    }

    /// Signals the end of a block.
    #[instrument(skip_all)]
    fn end_block(&self, _request: RequestEndBlock) -> ResponseEndBlock {
        info!("abci end_block");
        Default::default()
    }

    /// Signals that messages queued on the client should be flushed to the server.
    #[instrument]
    fn flush(&self) -> ResponseFlush {
        debug!("abci flush");
        ResponseFlush {}
    }

    /// Commit the current state at the current height.
    #[instrument]
    fn commit(&self) -> ResponseCommit {
        info!("abci commit");
        Default::default()
    }

    /// Used during state sync to discover available snapshots on peers.
    #[instrument]
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
