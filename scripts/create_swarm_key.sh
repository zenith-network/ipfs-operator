#!/usr/bin/env bash

set -euo pipefail

NETWORK="${1:-bootstrap}"
SWARM_KEY_FILE=$(mktemp)
trap 'rm -f -- "$SWARM_KEY_FILE"' EXIT

~/go/bin/ipfs-swarm-key-gen > "$SWARM_KEY_FILE"
kubectl -n ipfs create configmap "${NETWORK}"-swarm-key \
    --from-file="swarm.key=$SWARM_KEY_FILE"
