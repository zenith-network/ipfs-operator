#!/usr/bin/env bash

set -euo pipefail

: "${IPFS_OPERATOR_WEB_USERNAME:?Set IPFS_OPERATOR_WEB_USERNAME}"

NAMESPACE="ipfs-system"
SERVICE="ipfs-operator-web"
CREDENTIALS_FILE=$(mktemp)
trap 'rm -f -- "$CREDENTIALS_FILE"' EXIT

htpasswd -cB "$CREDENTIALS_FILE" "$IPFS_OPERATOR_WEB_USERNAME"
kubectl -n "$NAMESPACE" create secret generic "${SERVICE}-basic-auth" \
    --from-file=".htpasswd=$CREDENTIALS_FILE"
