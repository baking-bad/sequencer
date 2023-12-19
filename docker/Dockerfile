# Build application
#
# Copy in all crates, Cargo.toml and Cargo.lock unmodified,
# and build the application.
FROM rust:1.73-bullseye AS builder
ARG PROFILE=release
ARG GIT_REVISION
ENV GIT_REVISION=$GIT_REVISION
WORKDIR "$WORKDIR/sequencer"
RUN apt-get update && apt-get install -y cmake clang

COPY Cargo.toml ./
COPY crates crates
COPY executor executor
COPY launcher launcher
COPY playground playground

RUN cargo build --profile ${PROFILE} --bin narwhal-node
RUN cargo build --profile ${PROFILE} --features=benchmark --bin narwhal-benchmark-client

# Production Image
FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y libjemalloc-dev
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
ARG PROFILE=release
WORKDIR "$WORKDIR/sequencer"

# Copy the Narwhal node binary to bin folder
COPY --from=builder sequencer/target/${PROFILE}/narwhal-node bin/

# This is used for testing a cluster by generating load.
COPY --from=builder sequencer/target/${PROFILE}/narwhal-benchmark-client bin/

# Copy the entry point file
COPY docker/entry.sh ./

# Now add the entry point
CMD ./entry.sh

ARG BUILD_DATE
ARG GIT_REVISION
LABEL build-date=$BUILD_DATE
LABEL git-revision=$GIT_REVISION