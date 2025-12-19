echo "$(~/go/bin/ipfs-swarm-key-gen)"

NETWORK="${1:-bootstrap}"

kubectl -n ipfs create configmap "${NETWORK}"-swarm-key --from-literal swarm.key="$(~/go/bin/ipfs-swarm-key-gen)"
