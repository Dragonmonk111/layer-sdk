//! ABCI application server interface.

use std::collections::VecDeque;
use std::sync::Arc;

use rayon::{ThreadPool, ThreadPoolBuilder};
use tendermint_proto::v0_38::abci::{request::Value, Request, Response};

use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::select;

use tokio_rayon::AsyncRayonHandle;
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

    pub async fn bind<Addr, App>(self, addr: Addr, app: App) -> Result<Server<App>, AbciError>
    where
        Addr: ToSocketAddrs,
        App: Application + Send + Sync,
    {
        Server::bind::<Addr, App>(self, addr, app).await
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

pub struct MultiThreadedDispatcher<A: Application> {
    query: ThreadPool,
    check: ThreadPool,
    process: ThreadPool,
    snapshot: ThreadPool,
    app: A,
}

impl<A: Application> MultiThreadedDispatcher<A> {
    fn build(app: A, config: &ServerConfig) -> Self {
        let n = config.query_threads;
        info!("Starting query pool with {n} threads");
        let query = ThreadPoolBuilder::new().num_threads(n).build().unwrap();
        let n = config.check_threads;
        info!("Starting check pool with {n} threads");
        let check = ThreadPoolBuilder::new().num_threads(n).build().unwrap();
        info!("Starting process pool with 1 threads");
        let process = ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        info!("Starting snapshot pool with 1 threads");
        let snapshot = ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        Self {
            query,
            check,
            process,
            snapshot,
            app,
        }
    }

    fn pool<'a>(&'a self, request: &Request) -> &'a ThreadPool {
        match request.value.as_ref().unwrap() {
            // TODO: which three do these go to??
            Value::Echo(_) => &self.query,
            Value::Flush(_) => &self.query,
            Value::Info(_) => &self.query,
            Value::Query(_) => &self.query,
            Value::CheckTx(_) => &self.check,
            Value::Commit(_) => &self.process,
            Value::ExtendVote(_) => &self.process,
            Value::VerifyVoteExtension(_) => &self.process,
            Value::PrepareProposal(_) => &self.process,
            Value::ProcessProposal(_) => &self.process,
            Value::InitChain(_) => &self.process,
            Value::FinalizeBlock(_) => &self.process,
            Value::ListSnapshots(_) => &self.snapshot,
            Value::OfferSnapshot(_) => &self.snapshot,
            Value::LoadSnapshotChunk(_) => &self.snapshot,
            Value::ApplySnapshotChunk(_) => &self.snapshot,
        }
    }

    // dispatch anything only here
    fn dispatch(&self, request: Request) -> AsyncRayonHandle<Response> {
        let call_app = self.app.clone();
        let pool = self.pool(&request);
        pool.install(move || tokio_rayon::spawn_fifo(move || call_app.handle(request)))
    }

    // TODO: allow grpc to dispatch queries
}

pub struct Server<A: Application> {
    listener: TcpListener,
    config: ServerConfig,
    dispatcher: Arc<MultiThreadedDispatcher<A>>,
}

impl<A: Application + Send + Sync> Server<A> {
    /// Constructor for an ABCI server.
    ///
    /// Binds the server to the given address. You must subsequently call the
    /// [`Server::listen`] method in order for incoming connections' requests
    /// to be routed to the specified ABCI application.
    pub async fn bind<Addr, App>(
        config: ServerConfig,
        addr: Addr,
        app: A,
    ) -> Result<Self, AbciError>
    where
        Addr: ToSocketAddrs,
    {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?.to_string();
        let dispatcher = Arc::new(MultiThreadedDispatcher::build(app, &config));
        info!("ABCI server running at {}", local_addr);
        Ok(Server {
            listener,
            dispatcher,
            config,
        })
    }

    /// Listen for incoming connections, and route their requests to the
    /// specified ABCI application.
    pub async fn listen(self) -> Result<(), AbciError> {
        let mut handles = vec![];
        // limit to four connections (in any order)
        for _ in 0..4 {
            let (stream, _) = self.listener.accept().await.unwrap();
            let config = self.config.clone();
            let dispatcher = self.dispatcher.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, dispatcher, config).await {
                    error!("Error handling connection: {}", e);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
        Ok(())
    }

    async fn handle_connection<App>(
        stream: TcpStream,
        dispatcher: Arc<MultiThreadedDispatcher<App>>,
        config: ServerConfig,
    ) -> Result<(), AbciError>
    where
        App: Application,
    {
        let mut codec = ServerCodec::new(stream, config.read_buf_size);
        let mut pending = VecDeque::<AsyncRayonHandle<Response>>::with_capacity(32);

        info!("Listening for ABCI requests");
        loop {
            let step: Step = if let Some(processed) = pending.front_mut() {
                select! {
                    res = processed => {
                        let _ = pending.pop_front();
                        Step::Output(res)
                    }
                    input = codec.next() => Step::Input(input),
                }
            } else {
                Step::Input(codec.next().await)
            };
            match step {
                Step::Input(input) => {
                    let request = match input {
                        Some(Ok(request)) => request,
                        Some(Err(e)) => return Err(e),
                        None => {
                            info!("Connection closed");
                            return Ok(());
                        }
                    };

                    // send request to rayon thread pool and add to response queue
                    let response = dispatcher.dispatch(request);
                    pending.push_back(response);
                }
                Step::Output(response) => {
                    codec.send(response).await?;
                }
            }
        }
    }
}

enum Step {
    Input(Option<Result<Request, AbciError>>),
    Output(Response),
}
