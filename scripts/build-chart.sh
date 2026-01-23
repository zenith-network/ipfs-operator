#!/usr/bin/env bash

if [ $# -ne 1 ]; then
    echo "Usage: $0 <tag>"
    exit 1
fi

cargo run --bin crdgen 2> /dev/null > charts/ipfs-operator/templates/ipfsnodes.gevulot.com.yaml
mkdir -p chart_data
podman run --rm -v ./chart_data:/data/ipfs docker.io/ipfs/kubo:latest init --profile server
cp chart_data/config charts/ipfs-operator/files/ipfs-config.json
rm -rf chart_data
