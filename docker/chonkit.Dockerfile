ARG ONX_VERSION=1.20.1

FROM rust:latest AS builder

ARG ONX_VERSION
ARG FEATURES="fe-remote qdrant weaviate openai"

WORKDIR /app

COPY chonkit ./chonkit
COPY chunx ./chunx
COPY embedders ./embedders
COPY sqlx-data.json ./chonkit/sqlx-data.json

RUN mkdir pdfium && curl -sL https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F6666/pdfium-linux-x64.tgz | tar -xzf - -C ./pdfium
RUN mkdir onnxruntime && curl -sL https://github.com/microsoft/onnxruntime/releases/download/v${ONX_VERSION}/onnxruntime-linux-x64-${ONX_VERSION}.tgz | tar -xzf - -C ./onnxruntime

WORKDIR /app/chonkit

RUN echo "FEATURES: ${FEATURES}"
RUN cargo build --no-default-features -F "${FEATURES}" --release --target-dir ./target

FROM debian:latest

ARG ONX_VERSION

WORKDIR /app

# Create upload directory
RUN mkdir upload

COPY --from=builder /app/chonkit/target/release/chonkit ./chonkit
COPY --from=builder /app/chonkit/migrations ./migrations
COPY --from=builder /app/pdfium/lib/libpdfium.so /usr/lib
COPY --from=builder /app/onnxruntime/onnxruntime-linux-x64-${ONX_VERSION}/lib/libonnxruntime.so /usr/lib

RUN apt-get update && apt-get install -y libssl3 && apt clean && rm -rf /var/lib/apt/lists/*

EXPOSE 42069

ENTRYPOINT ["./chonkit"]
