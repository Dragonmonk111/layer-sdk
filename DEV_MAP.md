# Development Map

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


