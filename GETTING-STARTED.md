# High-level overview

Let's say Alice the User wants to mint a NFT with an image generated from an off-chain process. Meanwhile, Bob the Operator is ready with his beefy GPU to do the work (and earn some rewards for doing so)... how does the work get done? In other words, Alice and Bob don't know eachother, how do they communicate and agree on the work order and delivery?

Lay3r to the rescue! The process looks like this:

1. Alice submits a task to a special contract with the required work information (image size, AI prompt, etc.). 
2. Bob is scanning the chain, looking for on-chain events that encapsulate the work order.
3. Bob goes off and does the work with his beefy machine (or crayons, no limits!), ultimately writing some metadata such as the IPFS hash back onto the chain. 
4. Alice notices that the on-chain status has changed to completed work, and she can now go download her image at that IPFS hash.

There's more details of course - Bob isn't actually staring at a monitor reading blockchain events, and how does Alice know he really ran the "awesome image generation" code she requested? The short answer is Bob is running an AVS and everything is secured by the underlying consensus algorithms, but, as a developer getting started, we're more interested in what pieces need to be built in order to facilitate the flow of data here.

Lay3r is built on three parts:

1. The blockchain
2. Special contracts deployed on that blockchain
3. Actively Validated Services

Each of these work together to create the complete system. Each part can be developed independently, in fact they are in separate repos, but they only shine when fully connected. Let's walk through getting the whole thing up and running up on a local computer.

Keep in mind that it's early days, and while there may eventually be things like public Docker images or fully tested cross-platform scripts, it's a bit of the wild west right now. If you see something, please say something :)

## Blockchain

Clone the repo: https://github.com/Lay3rLabs/layer-sdk

The SDK contains the blockchain software and a few additional tools for interacting with it. First, let's get the node up and running:

[_more localnode documentation here_](localnode/README.md)

1. Build the Docker image: `scripts/build_docker.sh`
2. Reset the Volumes: `localnode/reset_volumes.sh`
3. Start the node: `localnode/run.sh` 
4. Health checks
	1. Check RPC status: `curl http://localhost:26657/status | jq`
	2. Check gRPC status: FIXME
		1. `grpcurl -plaintext localhost:9090 layer.sync.v1.QueryLatestSequenceRequest`
	3. Run JS tests
		1. cd `js`
		2. `npm install`
		3. `npm run test`

Now that it's all working, let's stop the blockchain:

```
localnode/stop.sh
```

And start it again:
```
localnode/run.sh
```

## Wallet

We're going to need a wallet with some funds. We can go ahead and use the provided faucet seed phrase for everything:

```
economy stock theory fatal elder harbor betray wasp final emotion task crumble siren bottom lizard educate guess current outdoor pair theory focus wife stone
```

However, it's probably better to use our own wallet. How do we add our wallet, when we don't even know the address? (or send more funds to it when we do). It's easy, just use the `tap-faucet` JS tool.

1. cd to `layer-sdk/js` (same place we ran JS tests above)
2. `npm run tap-faucet -- "{ADDR OR SEED PHRASE}" {amount}`
	1. So for example, `npm run tap-faucet -- "hello world ..." 500`

This will send the funds and also let you know the recipient address

## Contracts

Clone the repo: https://github.com/Lay3rLabs/lay3r-contracts

These contracts serve as the pipeline for tasks, sort-of a communication channel between users and operators.

The exact implementation of how the queue is prioritized, who can write state changes, and other details are subject to change. For now, let's deploy the core contracts needed to enable this functionality.

1. Build the contracts: `scripts/collect_wasm.sh`
2. `cd deploy`
3. add a `.env` file with the following:

```
TEST_MNEMONIC = "YOUR-TESTNET-MNEMONIC"
LOCAL_MNEMONIC = "YOUR-LOCAL-DOCKER-MNEMONIC"
CW_ORCH_MIN_BLOCK_SPEED = "1"
RUST_LOG = "info"
```


4. Deploy the contracts: `cargo run --bin avs -- --local deploy {OPERATOR_ADDR}`
	1. For now, the operator addr is hardcoded at `slay3r10dyr9899g6t0pelew4nvf4j5c3jcgv0rf3kguu`
    2.  **NOTE: USING NON-FAUCET-ADDR FAILS??**
	3. **Where does this operator addr come from??**
5. View the latest-and-greatest deploy: `cargo run --bin avs -- --local view`

You'll see output like this:

```
Task Code ID: {SOME CODE ID}
Task Contract Address: {SOME ADDR}
Verifier Contract Address: {SOME ADDR}
```

**Write down the `Task Contract Address` - this is going to be important for the next step!**

## ACTIVELY VALIDATED SERVICES

Clone the repo: https://github.com/Lay3rLabs/lay3r-avs-runners

This name is going to come up a lot, so for now on, we'll just use the industry jargon: "AVS"

Our AVS's are executed on Spin, a WASM-based runtime that can run locally and/or on a cloud platform (with a generous free tier). This isn't a hard requirement, we may move to a different host in the future. But for now, it's a prerequisite, so let's get Spin setup:

1. Get Spin installed: https://developer.fermyon.com/spin/v2/quickstart
2. Install the Task Queue Spin plugin
	1. `cd runners/task-queue`
	2. `spin plugin install pluginify
	3. `RUSTFLAGS='-C link-arg=-s' cargo build --releaseA
	4. `spin pluginify --install`	

Now that we have spin setup, we can get an example AVS up and running. Let's try out the "demo square" app:

1. `cd apps-lavs-demo-square`
2. `spin build`
3. edit `spin.toml`
	1. in the `application.trigger.lay3r-task-queue` section:
		1. change `chain_kind` to `Local`
		2. change `grpc_url` to `http://localhost:9090`
		3. comment out `faucet_url`
	2. in the `trigger.lay3r-task-queue` section:
		1. comment out `verifier_addr` (if it's there - probably delete this soon)
		2. set `task_queue_addr` to the `Task Contract Address` we got above when deploying the core contracts
4. add a `.env` with the following
```
TEST_MNEMONIC = "YOUR-TESTNET-MNEMONIC"
LOCAL_MNEMONIC = "YOUR-LOCAL-DOCKER-MNEMONIC"```
```
5. `spin up --test`

**ISN'T WORKING WITH CUSTOM MNEMONIC??**

If all went well you'll see a result like:

```
--> Testing Component: square
8^2 = 64
```

That's it! now you're all setup with a local dev environment.

## Next steps:

* Change to Testnet
* Develop your own product with a custom AVS and contracts
