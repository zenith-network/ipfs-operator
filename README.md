# IPFS Operator

1. IPFS Operator Namespaces

```
kubectl create namespace ipfs-operator
kubectl create namespace ipfs
```

2. Deploy pull secret

```
./scripts/create_pull_secret.sh
```

2. Envoy Gateway

```
helm install \
  eg oci://docker.io/envoyproxy/gateway-helm \
  --version v1.6.3 \
  --namespace envoy-gateway-system \
  --create-namespace
```

4. After deploying Envoy get the external IP and update cloudflare

```
kubectl -n ipfs-system get gateway/eg -o jsonpath='{.status.addresses[0].value}'
```

3. Create certificate

```
./scripts/create_certificate.sh
```

4. Create credentials

```
IPFS_OPERATOR_WEB_USERNAME=your-user ./scripts/create_credentials.sh
```

# Bootstrap cluster

./scripts/create_swarm_key.sh
kubectl apply -f bootstrap.yaml
