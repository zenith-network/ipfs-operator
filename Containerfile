# Suffix of the tag of the Rust image
# Also tag of the Alpine image for the final stage
# Note: because of the Rust tags convention, no patch version
# can be specified here. E.g. ALPINE_TAG=3.23.0 will fail to build.
ARG ALPINE_TAG=3.23

# Version included into tag of the Rust image
ARG RUST_VERSION=1.93.0

# Tag of the Rust version
# By default constructed from Rust version and Alpine tag
ARG RUST_TAG=$RUST_VERSION-alpine$ALPINE_TAG

# Define stages from base images, so they can be refereced in COPY commands
# in the next stages
FROM rust:$RUST_TAG AS rust

# Stage 3: full build environment (Rust, Buf, Protobuf code generators)
# This stage can be used as a dev environment for zenith-cli (e.g. from dev containers)
FROM rust AS dev

# Install other build deps
RUN apk update && apk add --no-cache \
    build-base \
    bash \
    rsync \
    protobuf-dev \
    openssl-dev \
    openssl-libs-static \
    libgcrypt-static \
    git

# Build zenith-cli
FROM dev AS build-ipfs-operator

WORKDIR /app

# Copy Rust sources
COPY Cargo.toml Cargo.lock /app/
COPY crates /app/crates

# Build the zenith-cli
# We don't cleanup target and CARGO_HOME here, because we want to
# be able to mount them.
# RUN cargo build --release --locked --package ipfs-operator \
#     && cp target/release/controller /usr/local/bin/controller

# Alternative build command, which utilizes cache mounts
# If you are only working with local builds, you can comment the RUN above
# and use this instead. Because cache mounts are not safe to transfer
# across machines, we can't utilize them for caching elsewhere (e.g. in CI).
# That's why by default we use the RUN above.
# To speed up local re-builds use this command instead:

RUN --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --release --locked --package ipfs-operator \
    && cp target/release/controller /usr/local/bin/controller

# Final lighweight image for zenith-cli
FROM alpine:$ALPINE_TAG AS ipfs-operator

# Copy the binary
COPY --from=build-ipfs-operator /usr/local/bin/controller /usr/local/bin/controller

ENTRYPOINT ["/usr/local/bin/controller"]

FROM dev AS build-bootstrap-web

WORKDIR /app

# Copy Rust sources
COPY Cargo.toml Cargo.lock /app/
COPY crates /app/crates

# Build the zenith-cli
# We don't cleanup target and CARGO_HOME here, because we want to
# be able to mount them.
# RUN cargo build --release --locked --package ipfs-operator \
#     && cp target/release/controller /usr/local/bin/controller

# Alternative build command, which utilizes cache mounts
# If you are only working with local builds, you can comment the RUN above
# and use this instead. Because cache mounts are not safe to transfer
# across machines, we can't utilize them for caching elsewhere (e.g. in CI).
# That's why by default we use the RUN above.
# To speed up local re-builds use this command instead:

RUN --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --release --locked --package bootstrap-web \
    && cp target/release/bootstrap-web /usr/local/bin/bootstrap-web

# Final lighweight image for zenith-cli
FROM alpine:$ALPINE_TAG AS bootstrap-web

# Copy the binary
COPY --from=build-bootstrap-web /usr/local/bin/bootstrap-web /usr/local/bin/bootstrap-web

ENTRYPOINT ["/usr/local/bin/bootstrap-web"]
