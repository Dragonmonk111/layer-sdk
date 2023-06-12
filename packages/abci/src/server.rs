//! ABCI application server interface.

use rayon::ThreadPoolBuilder;
use tendermint_proto::v0_38::abci::{request::Value, Request};

use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

use tracing::{error, info};

use crate::application::RequestDispatcher;
use crate::{AbciError, Application, ServerCodec};

/// The size of the read buffer for each incoming connection to the ABCI
/// server. Should handle any block (4MB).
pub const DEFAULT_SERVER_READ_BUF_SIZE: usize = 4 * 1024 * 1024;

pub const DEFAULT_QUERY_THREADS: usize = 4;

pub const DEFAULT_CHECK_THREADS: usize = 4;

/// Allows us to configure and construct an ABCI server.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    read_buf_size: usize,
    query_threads: usize,
    check_threads: usize,
}

impl ServerConfig {
    /// Builder constructor.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_read_buf(mut self, read_buf_size: usize) -> Self {
        self.read_buf_size = read_buf_size;
        self
    }

    pub fn with_query_threads(mut self, query_threads: usize) -> Self {
        self.query_threads = query_threads;
        self
    }

    pub fn with_check_threads(mut self, check_threads: usize) -> Self {
        self.check_threads = check_threads;
        self
    }

    pub fn thread_pool_size(&self, _conn: ConnectionType) -> usize {
        // TODO: read values for larger pools
        1
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            read_buf_size: DEFAULT_SERVER_READ_BUF_SIZE,
            query_threads: DEFAULT_QUERY_THREADS,
            check_threads: DEFAULT_CHECK_THREADS,
        }
    }
}

pub struct Server<A: Application> {
    listener: TcpListener,
    app: A,
    config: ServerConfig,
}

impl<A: Application> Server<A> {
    /// Constructor for an ABCI server.
    ///
    /// Binds the server to the given address. You must subsequently call the
    /// [`Server::listen`] method in order for incoming connections' requests
    /// to be routed to the specified ABCI application.
    pub async fn bind<Addr, App>(
        self,
        config: ServerConfig,
        addr: Addr,
        app: A,
    ) -> Result<Self, AbciError>
    where
        Addr: ToSocketAddrs,
    {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?.to_string();
        info!("ABCI server running at {}", local_addr);
        Ok(Server {
            listener,
            app,
            config,
        })
    }

    /// Listen for incoming connections, and route their requests to the
    /// specified ABCI application.
    pub async fn listen(self) -> Result<(), AbciError> {
        let mut next_conn = Some(ConnectionType::new());
        loop {
            let (stream, _) = self.listener.accept().await?;
            let app = self.app.clone();
            let config = self.config.clone();
            let this_conn = next_conn.unwrap();
            next_conn = this_conn.next();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, app, config, this_conn).await {
                    error!("Error handling connection: {}", e);
                }
            });
        }
    }

    async fn handle_connection<App>(
        stream: TcpStream,
        app: App,
        config: ServerConfig,
        conn: ConnectionType,
    ) -> Result<(), AbciError>
    where
        App: Application,
    {
        let mut codec = ServerCodec::new(stream, config.read_buf_size);
        let _pool = ThreadPoolBuilder::new()
            .num_threads(config.thread_pool_size(conn))
            .build()
            .unwrap();

        info!("Listening for ABCI requests, connection: {conn:?}");
        loop {
            let request = match codec.next().await {
                Some(Ok(request)) => request,
                Some(Err(e)) => return Err(e),
                None => {
                    info!("Connection closed, connection: {conn:?}");
                    return Ok(());
                }
            };
            // assert this is valid for our connection type
            conn.assert_valid_message(&request)?;

            // TODO: send request to rayon thread pool
            let response = app.handle(request);

            codec.send(response).await?;
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ConnectionType {
    Query,
    Check,
    Process,
    Snapshot,
}

impl ConnectionType {
    pub(crate) fn new() -> Self {
        // TODO: which is the first one?
        ConnectionType::Query
    }

    pub(crate) fn next(&self) -> Option<Self> {
        // TODO: what is the order of connections?
        match self {
            ConnectionType::Query => Some(ConnectionType::Check),
            ConnectionType::Check => Some(ConnectionType::Process),
            ConnectionType::Process => Some(ConnectionType::Snapshot),
            ConnectionType::Snapshot => None,
        }
    }

    fn is_query(&self) -> bool {
        matches!(self, ConnectionType::Query)
    }

    fn is_check(&self) -> bool {
        matches!(self, ConnectionType::Check)
    }

    fn is_process(&self) -> bool {
        matches!(self, ConnectionType::Process)
    }

    fn is_snapshot(&self) -> bool {
        matches!(self, ConnectionType::Snapshot)
    }

    pub(crate) fn assert_valid_message(&self, request: &Request) -> Result<(), AbciError> {
        let (is_valid, expected_type) = match request.value.as_ref().unwrap() {
            Value::Echo(_) => (true, "Echo"),
            Value::Flush(_) => (true, "Flush"),
            Value::Info(_) => (self.is_query(), "Info"),
            Value::Query(_) => (self.is_query(), "Query"),
            Value::CheckTx(_) => (self.is_check(), "CheckTx"),
            Value::Commit(_) => (self.is_process(), "Commit"),
            Value::ExtendVote(_) => (self.is_process(), "ExtendVote"),
            Value::VerifyVoteExtension(_) => (self.is_process(), "VerifyVoteExtension"),
            Value::PrepareProposal(_) => (self.is_process(), "PrepareProposal"),
            Value::ProcessProposal(_) => (self.is_process(), "ProcessProposal"),
            Value::InitChain(_) => (self.is_process(), "InitChain"),
            Value::FinalizeBlock(_) => (self.is_process(), "FinalizeBlock"),
            Value::ListSnapshots(_) => (self.is_snapshot(), "Snapshot"),
            Value::OfferSnapshot(_) => (self.is_snapshot(), "Snapshot"),
            Value::LoadSnapshotChunk(_) => (self.is_snapshot(), "Snapshot"),
            Value::ApplySnapshotChunk(_) => (self.is_snapshot(), "Snapshot"),
        };
        if is_valid {
            Ok(())
        } else {
            Err(AbciError::InvalidMessage {
                message: expected_type,
                connection: *self,
            })
        }
    }
}
