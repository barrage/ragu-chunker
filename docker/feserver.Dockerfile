ARG ONX_VERSION=1.20.1

FROM rust:latest AS builder

ARG ONX_VERSION

WORKDIR /app

COPY feserver ./feserver
COPY embedders ./embedders

RUN mkdir onnxruntime

RUN curl -sL \
  https://github.com/microsoft/onnxruntime/releases/download/v$ONX_VERSION/onnxruntime-linux-x64-$ONX_VERSION.tgz \
  | tar -xzf - -C ./onnxruntime

RUN curl -sL \
  https://github.com/microsoft/onnxruntime/releases/download/v$ONX_VERSION/onnxruntime-linux-x64-gpu-$ONX_VERSION.tgz \
  | tar -xzf - -C ./onnxruntime

WORKDIR /app/feserver

RUN cargo build --release --target-dir ./target

FROM nvidia/cuda:12.6.2-cudnn-devel-ubuntu22.04

ARG ONX_VERSION

WORKDIR /app

COPY --from=builder /app/feserver/target/release/feserver ./feserver

COPY --from=builder /app/onnxruntime/onnxruntime-linux-x64-${ONX_VERSION}/lib/libonnxruntime_providers_shared.so /usr/lib
COPY --from=builder /app/onnxruntime/onnxruntime-linux-x64-${ONX_VERSION}/lib/libonnxruntime.so /usr/lib
COPY --from=builder /app/onnxruntime/onnxruntime-linux-x64-gpu-${ONX_VERSION}/lib/libonnxruntime.so /usr/lib
COPY --from=builder /app/onnxruntime/onnxruntime-linux-x64-gpu-${ONX_VERSION}/lib/libonnxruntime_providers_cuda.so /usr/lib
COPY --from=builder /app/onnxruntime/onnxruntime-linux-x64-${ONX_VERSION}/lib/libonnxruntime_providers_shared.so /usr/lib

RUN apt-get update 
RUN apt-get install -y libssl3 
RUN apt clean 
RUN rm -rf /var/lib/apt/lists/*

EXPOSE 6969

ENTRYPOINT ["./feserver"]
