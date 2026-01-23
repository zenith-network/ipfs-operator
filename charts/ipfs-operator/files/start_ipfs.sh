#!/bin/sh
set -e

user=ipfs
repo="$IPFS_PATH"

if [ "$(id -u)" -eq 0 ]; then
  echo "Changing user to $user"
  # ensure folder is writable
  gosu "$user" test -w "$repo" || chown -R -- "$user" "$repo"
  # restart script with new privileges
  exec gosu "$user" "$0" "$@"
fi

# 2nd invocation with regular user
ipfs version

echo "Using hostname: ${HOSTNAME}"

exec ipfs "$@" --init --init-config=/var/lib/ipfs/configs/${HOSTNAME}
