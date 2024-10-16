# Developer Map

This document serves as a guide to the different repositories and components involved in the Lay3r project. It will help internal developers quickly orient themselves to the various tools, services, and repositories that form the software stack.

## Repositories Overview

### **1. Layer-SDK**
- **Purpose:** Provides server-related functionality.
- **Tools and Components:**
    - **Layer-SDK Server:** Handles the core server logic and processes.
    - **Contracts**: Contains test contracts primarily for testing purposes, not for production. The **root contract** is deployed during Layer-SDK chain initialization and is invoked by other contracts.
    - **Helpers & Utilities:** Various internal tools that simplify server operations:
      - **Localnode**: Includes configurations and scripts to set up a local development node, utilizing Docker and Docker Compose for local testing.
      - **Docker**: Contains Dockerfiles for building components like the gateway and faucet.
      - **Scripts**: Provides build and deploy scripts for docker images.
- **Related Docs:**
    - [Layer-SDK README](https://github.com/Lay3rLabs/layer-sdk/blob/main/README.md)
    - [Getting Started](https://github.com/Lay3rLabs/layer-sdk/blob/main/GETTING-STARTED.md) providing a high-level overview.
    - [Deployment](https://github.com/Lay3rLabs/layer-sdk/blob/main/DEPLOYMENT.md): Tips for deploying Layer-SDK on remote servers using Docker.
    - [Elastic Search](https://github.com/Lay3rLabs/layer-sdk/blob/main/ELASTIC_SEARCH.md): Elasticsearch queries tested with Postman.
    - [Messages](https://github.com/Lay3rLabs/layer-sdk/blob/main/MESSAGES.md): A living document providing pointers on adding messages and queries to the system.


### 2. Wasmatic (Closed-Source)
- **Purpose:** Wasmatic repository provides tools for deploying and managing WebAssembly applications with an operator API. It supports adding, testing, and removing applications triggered by cron schedules or queues.
- **Tools and Components:**
    - **Examples:**
      - **btc-avg:** the example shows how to register WebAssembly applications, like fetching Bitcoin prices, and trigger them using API calls based on a schedule or task queue.
      - **composition/http-allow-list:** defines an `HTTP` access control in a WebAssembly component, allowing outgoing requests only to specified hosts (in our case, "api.coingecko.com"). If a request is made to an unauthorized host, it is refused.
      - **square:** this defines a WebAssembly component that processes tasks from a task queue. It takes an input number, squares it, and returns the result in a serialized JSON format.
    - **Scripts:** different shell scripts that ease up developing for Lay3r.
    - **Src:** contains modules that handle app management, storage, queues, and operator control, enabling the deployment and execution of tasks for various applications.
    - **Wit:** This WIT file in this module defines the task-queue and cron-job interfaces for Lay3r AVS. The other folders contain functions to process tasks  and cron jobs, as well as imports for handling I/O, clocks, and HTTP requests.
- **Related Docs:** Internal documentation only.
    - [Authoring components](https://github.com/Lay3rLabs/wasmatic/blob/main/AUTHORING_COMPONENTS.md)
    - [Wasmatic README.md](https://github.com/Lay3rLabs/wasmatic/blob/main/README.md)

### 3. AVS-Toolkit
- **Purpose:** Open-source toolkit for deploying AVS contracts and related services.
- **Tools and Components:**
    - **Contracts:** Example contracts, that can be deployed to Lay3r or used as guides how to build your own.
    - **Packages:** 
      - **APIs:** Shared functionality, structs, trait definitions, etc. that can be reused.
      - **helpers:** Useful tools for other components of `avs-toolkit`.
      - **layer-wasi:** This module handles the construction and sending of HTTP requests, handling of responses and the interactions with WASI streams in an asynchronus way.
      - **orch:** Helpers for `[cw-orch](https://docs.rs/cw-orch/latest/cw_orch/)`.
    - **Scripts:** Different shell scripts that ease up developing for Lay3r.
    - **Tools:** 
      - **cli:** A cli tool that allows the managing of smartcontracts and blockchain tasks from the terminal. You can deploy your contracts, manage task queues, tap a faucet and other WASM operations on local and testnet.
      - **gui:** GUI tool for interacting with your smart contracts from the browser.
    - **WASI:** 
      - **oracle-example:** Example demonstrating how to build simple AVS Oracle component that queries the CoinGecko API for BTC/USD prices and calculates an average price over the past hour.
      - **py-square:** Example demonstrating how to build and deploy simple Python-based AVS Oracle component that squares an input number using WASI.
      - **square:** Same as the example above but in Rust
    - **Wit:** This WIT file in this module defines the task-queue and cron-job interfaces for Lay3r AVS. The other folders contain functions to process tasks  and cron jobs, as well as imports for handling I/O, clocks, and HTTP requests.
- **Related Docs:** 
    - [Tools/CLI](https://github.com/Lay3rLabs/avs-toolkit/blob/main/tools/cli/README.md)
    - [Tools/GUI](https://github.com/Lay3rLabs/avs-toolkit/blob/main/tools/gui/README.md)

### 4. Commitments
- **Purpose:** Set of infrastructure contracts to be deployed on the lay3r blockchain in order to provide core functionality.
- **Tools and Components:**
    - **Codegen:** Module built on top of [ts-codegen](https://github.com/CosmWasm/ts-codegen) to generate `TypeScript` types and code boilerplate for frontends.
    - **Contracts:**
      - **delegations:** smart contract responsible for delegations of a restaking mechanism. It cna connect providers and consumers, allows for staking, unstaking and delegation. Provides a set of queries for the related states.
      - **fan-in:** collects stakes from multiple providers into a single consumer, managing security levels, adding input providers, and facilitating staking, unstaking, and tracking of assets across tokens.
      - **fan-out:** distributes staked assets from a single provider to multiple consumers, managing security levels and handling flows of stakes across different contracts, with staking, unstaking, and unbonding.
      - **operators:** manages staking and unstaking with operators acting on behalf of stakers. We can assign operators and set the flow of stakes between providers and consumers.
      - **splitter:** divides stakes from a provider and sends them to multiple consumers based on predifined share percentages. It can handle staking, unstaking, and provide information about the flow of stakes between the provider and consumers.
      - **token-staking:** allows users to stake their tokens and pass the stake to another contract (consumer). Users can stake, unstake, and withdraw tokens after waiting period. The contract keeps track of staked tokens, handles requests to release them, and provides information about how much is staked, unbonded, or available for withdrawal.
      - **token-weighting:** manages voting power distribution based on staked tokens, with different tokens contributing varying power by predefined weights. It can handle the staking, unstaking, unbonding, and adjusting of voting power accordingly. The contract allows queries for individual and total voting power
    - **Deploy:** deployment scripts deploy various contracts and subsystems to `devnet`, with options for local or mainnet setups. An .env file with mnemonic keys is required. Scripts in the bin directory handle deployment, viewing, staking, and managing contracts.
    - **Docs:** elaborative explanation of the core Lay3r products.
    - **Packages:** 
      # I think the below are redundant and we will be using the ones from `avs-toolkit`
      - **APIs:** Shared functionality, structs, trait definitions, etc. that can be reused.
      - **Bindings:** _to be decided_
      - **Orch:** `[cw-orch](https://docs.rs/cw-orch/latest/cw_orch/)` helpers for lay3r contracts
    - **Scripts:** Different shell scripts that ease up developing for Lay3r.
- **Related Docs:** 
    - [Deployment Scripts](https://github.com/Lay3rLabs/commitments/blob/main/deploy/README.md)
    - [Docs](https://github.com/Lay3rLabs/commitments/tree/main/docs)

### 5. UI, Examples, and Docs
- **Purpose:** These repositories contain the user interface (UI), example code, and documentation for external developers.
- **Related Repositories:**
    - **docs-avs:** Documentation site for external developers building on the stack.
    - **UI Repository:** Contains the frontend code for interacting with Lay3r services.
    - **Example Repository:** Provides sample projects to demonstrate how to build on the AVS framework.
- **Related Docs:** [docs-avs](link-to-docs-avs)

---

## Additional Resources
- **Developer Onboarding Guide:** [Link to Onboarding Guide](link-to-onboarding-guide)
- **External Developer Documentation:** [docs-avs Site](link-to-docs-avs-site)

This `DEV_MAP.md` is intended for **internal developers** building the software stack. For developers building on the stack, please refer to [docs-avs](link-to-docs-avs-site).

