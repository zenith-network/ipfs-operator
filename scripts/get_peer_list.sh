for NODE in bootstrap-0 bootstrap-1 bootstrap-2; do
  kubectl -n ipfs get cm bootstrap-identities bootstrap-external-addresses -o json | jq --arg node "${NODE}" -r '.items | "/ip4/\(.[1].data[$node])/tcp/4001/ipfs/\(.[0].data[$node] | fromjson | .peer_id)"'
done
