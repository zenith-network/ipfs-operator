#!/usr/bin/env bash

NAMESPACE="ipfs-system"
O="Gevulot Oy"
CN="bootstrap.zenith.network"
CA="zenith.network"

# Create root certificate
if [ ! -f ${CA}.key ]; then
  openssl req -x509 -sha256 -nodes -days 365 -newkey rsa:2048 \
    -subj "/O=${O}/CN=${CA}" \
    -keyout ${CA}.key \
    -out ${CA}.crt
fi

# Create certificate and key
openssl req -out ${CN}.csr -newkey rsa:2048 -nodes \
  -subj "/CN=${CN}/O=${O}" \
  -keyout ${CN}.key

openssl x509 -req -days 365 -CA ${CA}.crt -CAkey ${CA}.key -set_serial 0 \
  -in ${CN}.csr \
  -out ${CN}.crt

kubectl -n ${NAMESPACE} create secret tls ${CN}-cert \
  --key=${CN}.key \
  --cert=${CN}.crt
