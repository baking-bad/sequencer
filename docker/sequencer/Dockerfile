FROM rust:1.73-bullseye AS builder
ARG PROFILE=release
WORKDIR /build
RUN apt-get update && apt-get install -y cmake clang protobuf-compiler
COPY . ./
RUN cargo build --profile ${PROFILE} --bin narwhal-node
RUN cargo build --profile ${PROFILE} --bin sequencer

FROM debian:bullseye-slim AS runtime
# Use jemalloc as memory allocator
RUN apt-get update && apt-get install -y libjemalloc-dev
ENV LD_PRELOAD /usr/lib/x86_64-linux-gnu/libjemalloc.so
ARG PROFILE=release
COPY --from=builder build/target/${PROFILE}/narwhal-node /usr/bin/narwhal-node
COPY --from=builder build/target/${PROFILE}/sequencer /usr/bin/sequencer
COPY ./launcher/defaults/* /narwhal/
COPY ./docker/sequencer/entrypoint.sh .
RUN chmod +x entrypoint.sh && ln ./entrypoint.sh /usr/bin/narwhal
ENTRYPOINT [ "./entrypoint.sh" ]