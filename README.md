![docs.rs](https://img.shields.io/docsrs/lumina-node?label=lumina-node&link=https%3A%2F%2Fdocs.rs%2Flumina-node%2Flatest%2Flumina_node%2F)

# Lumina

Rust implementation of Celestia's [data availability node](https://github.com/celestiaorg/celestia-node) able to run natively and in browser-based environments.

Supported features:
- [x] Synchronize and verify `ExtendedHeader`s from genesis to the network head
- [x] Header exchange (`header-ex`) client and server
- [x] Listening for, verifying and redistributing extended headers on gossip protocol (`header-sub`)
- [x] Persistent store for Headers
- [x] Integration tests with Go implementation
- [ ] Data Availability Sampling
- [ ] Creating, distributing, and listening for Fraud proofs

## Installing the node

### Installing with cargo

Install the node. Note that currently to serve lumina to run it from the browser, you need to compile `lumina-cli` manually.
```bash
cargo install lumina-cli --locked
```
Run the node
```bash
lumina node --network mocha
```

### Building from source

Install common dependencies

```bash
# install dependencies
sudo apt-get install -y build-essential curl git protobuf-compiler

# install rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# open a new terminal or run
source "$HOME/.cargo/env"

# clone the repository
git clone https://github.com/eigerco/lumina
cd lumina

# install lumina
cargo install --path cli
```

### Building wasm-node

To build `lumina-cli` with support for serving wasm-node to browsers, currently
you need to compile wasm node manually. Follow these additional steps:

```bash
# install wasm-pack
cargo install wasm-pack

# compile lumina to wasm
wasm-pack build --target web node-wasm

# install lumina-cli
cargo install --path cli --features browser-node
```

## Running the node

### Running the node natively

```bash
# run lumina node
lumina node --network mocha

# check out help for more configuration options
lumina node --help
```

### Building and serving node-wasm

```bash
# serve lumina node on default localhost:9876
lumina browser

# check out help from more configuration options
lumina browser --help
```

#### WebTransport and Secure Contexts

For security reasons, browsers only allow WebTransport to be used in [Secure Context](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts). When running Lumina in a browser make sure to access it either locally or over HTTPS.

## Running Go celestia node for integration

Follow [this guide](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry#authenticating-with-a-personal-access-token-classic)
to authorize yourself in github's container registry.

Starting a celestia network with single validator and bridge
```bash
docker compose -f ci/docker-compose.yml up --build --force-recreate -d
# and to stop it
docker compose -f ci/docker-compose.yml down
```
> **Note:**
> You can run more bridge nodes by uncommenting/copying the bridge service definition in `ci/docker-compose.yml`.

To get a JWT token for a topped up account (coins will be transferred in block 2):
```bash
export CELESTIA_NODE_AUTH_TOKEN=$(docker compose -f ci/docker-compose.yml exec bridge-0 celestia bridge auth admin --p2p.network private)
```

Accessing json RPC api with Go `celestia` cli:
```bash
celestia rpc blob Submit 0x0c204d39600fddd3 '"Hello world"' --print-request
```

Extracting blocks for test cases:
```bash
celestia rpc header GetByHeight 27 | jq .result
```

## Running integration tests with celestia node

Make sure you have the celestia network running inside docker compose from the section above.

Generate authentication tokens
```bash
./tools/gen_auth_tokens.sh
```

Run tests
```bash
cargo test
```
