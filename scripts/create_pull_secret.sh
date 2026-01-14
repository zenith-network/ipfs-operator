#!/usr/bin/env bash

#AUTH_FILE=/run/user/1000/containers/auth.json
AUTH_FILE=${HOME}/.config/containers/auth.json

kubectl create secret generic quay-read-only \
    --namespace ipfs-system \
    --from-file=.dockerconfigjson=${AUTH_FILE} \
    --type=kubernetes.io/dockerconfigjson
