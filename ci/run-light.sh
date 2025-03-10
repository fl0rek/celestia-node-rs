#!/bin/bash

set -euo pipefail

# a private local network
P2P_NETWORK="private"
# a light node configuration directory
CONFIG_DIR="$HOME/.celestia-light-$P2P_NETWORK"
# directory where validator will write the genesis hash
GENESIS_DIR="/genesis"
GENESIS_HASH_FILE="$GENESIS_DIR/genesis_hash"

# Wait for the validator to set up and provision us via shared dir
wait_for_provision() {
	echo "Waiting for the validator node to start"
	while [[ ! -e "$GENESIS_HASH_FILE" ]]; do
		sleep 0.1
	done

	echo "Validator is ready"
}

add_trusted_genesis() {
	local genesis_hash

	# Read the hash of the genesis block
	genesis_hash="$(cat "$GENESIS_HASH_FILE")"
	# and make it trusted in the node's config
	echo "Trusting a genesis: $genesis_hash"
	sed -i'.bak' "s/TrustedHash = .*/TrustedHash = $genesis_hash/" "$CONFIG_DIR/config.toml"
}

whitelist_localhost_nodes() {
	# to get the list of ips:
	# cargo run -- node -n private -l 0.0.0.0
	# docker compose -f ci/docker-compose.yml exec bridge-0 celestia p2p peer-info $lumina_peerid
	dasel put -f "$CONFIG_DIR/config.toml" \
		-t json -v '["172.16.0.0/12", "192.168.0.0/16"]' \
		'P2P.IPColocationWhitelist'
}

main() {
	local peer_id peer_addr
	# Initialize the light node
	celestia light init --p2p.network "$P2P_NETWORK"
	# don't allow banning nodes we create in tests by pubsub ip counting
	whitelist_localhost_nodes
	# Wait for a validator
	wait_for_provision
	# Trust the private blockchain
	add_trusted_genesis
	# give validator some time to set up
	sleep 4

	peer_id=$(celestia p2p info --token 'foo' --url 'ws://bridge-0:26658' | jq -r '.result.id')
	peer_addr="/dns/bridge-0/tcp/2121/p2p/$peer_id"

	# Start the light node
	echo "Configuration finished. Running a light node..."
	celestia light start \
		--rpc.skip-auth=true \
		--rpc.addr 0.0.0.0 \
		--core.ip validator \
		--headers.trusted-peers "$peer_addr" \
		--p2p.network "$P2P_NETWORK"
}

main
