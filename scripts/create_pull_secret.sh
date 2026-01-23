#!/usr/bin/env bash

#AUTH_FILE=/run/user/1000/containers/auth.json
AUTH_FILE=${HOME}/.config/containers/auth.json

for namespace in ipfs-system ipfs; do
kubectl create secret generic quay-read-only \
    --namespace ${namespace} \
    --from-file=.dockerconfigjson=${AUTH_FILE} \
    --type=kubernetes.io/dockerconfigjson
done
