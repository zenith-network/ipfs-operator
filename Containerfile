FROM rust:1-bookworm

COPY ./Cargo.* ./
COPY ./src ./src

# Add .git to crates/node in order to build version metadata into binary.
COPY ./.git ./src/.git

#RUN apt-get update && apt-get upgrade -y && apt-get install -y \
#  libssl-dev \
#  protobuf-compiler

RUN cargo build --bin=controller --release

FROM debian:bookworm

# Copy Gevulot node bin from earlier build step.
COPY --from=0 target/release/controller /controller

ENTRYPOINT ["/controller"]
