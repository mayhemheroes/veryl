# ARGS
ARG project=veryl

# Setup Fuzz-Rust Builder
FROM ghcr.io/nottirb/fuzz-rust:latest as builder
ADD . /${project}
WORKDIR /${project}
RUN cd ./fuzz && cargo +nightly hfuzz build

# Setup Fuzz-Env
FROM ubuntu:20.04
COPY --from=builder ${project}/fuzz/hfuzz_target/x86_64-unknown-linux-gnu/release/parse_sv /
COPY --from=builder ${project}/fuzz/hfuzz_target/x86_64-unknown-linux-gnu/release/parse_vl /
