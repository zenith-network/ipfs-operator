NETWORK="${1:-bootstrap}"

kubectl -n ipfs create configmap "${NETWORK}"-startup-scripts --from-file start_ipfs=start_ipfs.sh
