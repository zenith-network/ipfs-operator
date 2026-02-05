#!/usr/bin/env bash

if [ $# -eq 0 ]; then
    echo "Usage: $0 <tag>"
    exit 1
fi

BIN=${2:-ipfs-operator}

if [ ${BIN} = "ipfs-operator" ]; then
  cargo run --bin crdgen 2> /dev/null > charts/ipfs-operator/templates/ipfsnodes.gevulot.com.yaml
  podman -c remote-dev build --target ipfs-operator -t quay.io/gevulot/ipfs-operator:${1} .
  podman -c remote-dev push quay.io/gevulot/ipfs-operator:${1}
elif [ ${BIN} = "bootstrap-web" ]; then
  podman -c remote-dev build --target bootstrap-web -t quay.io/gevulot/bootstrap-web:${1} .
  podman -c remote-dev push quay.io/gevulot/bootstrap-web:${1}
fi
