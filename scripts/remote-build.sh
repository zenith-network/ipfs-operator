#!/usr/bin/env bash

if [ $# -ne 1 ]; then
    echo "Usage: $0 <tag>"
    exit 1
fi

cargo run --bin crdgen 2> /dev/null > charts/ipfs-operator/templates/ipfsnodes.gevulot.com.yaml
podman -c remote-dev build -t quay.io/gevulot/ipfs-operator:${1} .
podman -c remote-dev push quay.io/gevulot/ipfs-operator:${1}
