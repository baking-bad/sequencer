ARG OCTEZ_TAG
FROM tezos/tezos:${OCTEZ_TAG} AS octez

FROM rust:slim-buster AS builder
RUN apt update && apt install -y wget make libc-dev clang
RUN rustup target add wasm32-unknown-unknown
WORKDIR /build
COPY Makefile ./
RUN make install CARGO_BIN_PATH=/usr/local/cargo/bin
COPY . .
RUN make build-operator

FROM alpine:3.15 AS rollup
RUN apk --no-cache add binutils gcc gmp libgmpxx hidapi libc-dev libev libffi sudo
ARG OCTEZ_PROTO
COPY --from=octez /usr/local/bin/octez-smart-rollup-node-${OCTEZ_PROTO} /usr/bin/octez-smart-rollup-node
COPY --from=octez /usr/local/bin/octez-client /usr/bin/octez-client
COPY --from=builder /build/bin/wasm_2_0_0/ /root/wasm_2_0_0/
COPY --from=builder /build/bin/kernel_installer.wasm /root/kernel.wasm
COPY ./docker/operator/entrypoint.sh .
RUN chmod +x entrypoint.sh && ln ./entrypoint.sh /usr/bin/operator
ENTRYPOINT [ "./entrypoint.sh" ]