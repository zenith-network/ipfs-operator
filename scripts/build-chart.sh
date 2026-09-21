#!/usr/bin/env bash

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <tag>"
    exit 1
fi

cargo run --bin crdgen 2> /dev/null > charts/ipfs-operator/templates/ipfsnodes.gevulot.com.yaml
mkdir -p chart_data
trap 'rm -rf chart_data' EXIT
podman run --rm -v ./chart_data:/data/ipfs docker.io/ipfs/kubo:latest init --profile server
jq '.Identity.PeerID = "" | .Identity.PrivKey = ""' \
    chart_data/config > chart_data/config.sanitized
mv chart_data/config.sanitized charts/ipfs-operator/files/ipfs-config.json
