# Subsquid Portal Tangle Blueprint

This blueprint enables Tangle Network operators to provide managed Subsquid Portal instances with an optional authenticated and rate-limited gateway service.

## Overview

Subsquid Portals provide efficient access to blockchain data via the Subsquid Network. This blueprint allows operators to run these portals as a service for customers who need reliable data access without managing the infrastructure themselves.

Key features:

- **Portal Provisioning:** Operators can easily deploy new `sqd-portal` instances configured for specific networks (e.g., `mainnet`).
- **Authenticated Gateway:** An optional gateway can be configured to sit in front of a portal instance, requiring API keys for access.
- **Rate Limiting:** The gateway enforces configurable rate limits per API key.
- **API Key Management:** Customers (or operators on their behalf) can generate new API keys associated with a specific gateway, with the key securely delivered via RSA encryption.

## Users & Interactions

1.  **Operator:**

    - Deploys this blueprint service on the Tangle Network.
    - Registers their service instance.
    - Manages the underlying infrastructure (Docker, host machine).
    - Can call any job on behalf of customers if needed.

2.  **Customer (Requester):**

    - Requests a portal instance from an operator (off-chain agreement).
    - Provides necessary configuration (network, RPC URLs, etc.) to the operator.
    - Requests gateway configuration (desired port) from the operator.
    - Requests API key generation, providing their RSA public key for secure delivery.
    - Uses the provided API key (after decrypting it) to access data through the operator's gateway endpoint.
    - Likely pays the operator off-chain or via a separate smart contract mechanism for the service.

3.  **Tangle Network:**
    - Facilitates job submission and result delivery.
    - Provides the secure execution environment for the blueprint.

## Jobs

This blueprint exposes the following jobs callable via the Tangle Network:

1.  **`provision_portal` (ID: 1)**

    - **Action:** Deploys a new `sqd-portal` Docker container based on the standard `subsquid/sqd-portal` image and the provided `subsquid-docker-compose.yml`.
    - **Arguments:**
      - `network`: (String) e.g., "tethys" or "mainnet".
      - `key_path_on_host`: (String) Absolute path _on the operator's machine_ to the portal's network key file.
      - `rpc_url`: (String) Primary blockchain RPC endpoint URL.
      - `l1_rpc_url`: (Optional<String>) L1 RPC endpoint URL (if applicable).
      - `boot_nodes`: (Optional<List<String>>) List of boot nodes in "<peer_id> <address>" format.
    - **Returns:** (String) Success message with the new Docker container ID.
    - **Called by:** Operator (usually based on a customer request).

2.  **`configure_gateway` (ID: 2)**

    - **Action:** Configures and starts (or restarts) the authenticated gateway proxy for a specific portal instance. Aborts any existing gateway task for that instance.
    - **Arguments:**
      - `portal_container_id`: (String) The container ID returned by `provision_portal`.
      - `listen_port`: (u16) The TCP port the gateway should listen on (e.g., 8090).
    - **Returns:** (String) Success message confirming configuration.
    - **Called by:** Operator (based on customer request to enable gateway).

3.  **`generate_api_key` (ID: 3)**
    - **Action:** Generates a new, secure API key for an existing, configured gateway, associates an optional rate limit, and adds it to the gateway's configuration.
    - **Arguments:**
      - `portal_container_id`: (String) The container ID of the portal whose gateway needs the key.
      - `limit_per_sec`: (Optional<u32>) Requests per second limit for this key (0 or None for unlimited).
      - `public_key`: (Vec<u8>) The requester's RSA public key (PKCS#1 DER format) used to encrypt the new API key.
    - **Returns:** (String) The new API key, RSA encrypted and base64 encoded.
    - **Called by:** Customer (or Operator). _Note: Payment verification might be added via a smart contract proxy._

## Setup & Running

1.  **Prerequisites:**
    - Rust toolchain
    - Docker
    - Access to a Tangle RPC endpoint
    - A funded Tangle account with keys
2.  **Build:**
    ```bash
    cargo build --release
    ```
3.  **Configure Environment:**
    - Set up environment variables for the Tangle connection, keystore, service ID, etc., typically via a `.env` file loaded by the `BlueprintEnvironment`.
4.  **Run:**
    ```bash
    ./target/release/subsquid-portal-blueprint-bin
    ```

## Development

- **Library Crate:** `subsquid-portal-blueprint-lib` contains the core logic, context, jobs, and gateway implementation.
- **Binary Crate:** `subsquid-portal-blueprint-bin` sets up the `BlueprintRunner`, router, producer, consumer, and context, then starts the blueprint service.
- **Tests:** E2E tests are located in `subsquid-portal-blueprint-lib/tests/e2e.rs` (requires Docker).
