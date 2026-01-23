NETWORK="${1:-bootstrap}"

kubectl -n ipfs-system create configmap default-startup-scripts \
  --from-file start_ipfs=charts/ipfs-operator/files/start_ipfs.sh \
  --from-file entrypoint.sh=charts/ipfs-operator/files/ipfs-cluster-entrypoint.sh
