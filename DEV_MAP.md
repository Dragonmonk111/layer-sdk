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
- **Purpose:** Contains closed-source server components.
- **Tools and Components:**
    - **Wasm Runner:** Manages the execution of WebAssembly tasks on the server.
    - **Wasm API:** Provides the interface for communication with Wasm processes.
- **Related Docs:** Internal documentation only.

### 3. AVS-Toolkit
- **Purpose:** Open-source toolkit for deploying AVS contracts and related services.
- **Tools and Components:**
    - **Contract Deployer:** Simplifies deployment of actively validated services (AVS) contracts.
    - **CLI Tools:** A set of command-line utilities for interacting with the AVS ecosystem.
- **Related Docs:** [AVS-Toolkit README](link-to-avs-toolkit-readme)

### 4. Lay3r-Contracts (Soon to Be Renamed, Closed-Source)
- **Purpose:** Contains closed-source contract and deployment tools.
- **Tools and Components:**
    - **Contract Templates:** Predefined smart contract templates for rapid deployment.
    - **Deployment Scripts:** Automated scripts for deploying contracts to various environments.
- **Related Docs:** Internal documentation only.

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

