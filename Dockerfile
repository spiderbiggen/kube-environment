# syntax=docker/dockerfile:1
FROM --platform=$BUILDPLATFORM rust:1.67.1 as builder

ARG TARGETPLATFORM

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
ENV CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
ENV CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++

RUN <<-EOF
case "$TARGETPLATFORM" in
  "linux/arm64")
    apt-get update && apt-get install -qq g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev
    echo vendored > /rust_features.txt
    echo aarch64-unknown-linux-gnu > /rust_target.txt ;;
  "linux/amd64")
    echo default > /rust_features.txt
    echo x86_64-unknown-linux-gnu > /rust_target.txt ;;
  *)
    exit 1 ;;
esac
EOF

RUN rustup target add $(cat /rust_target.txt)

WORKDIR /app/builder
COPY . ./
RUN cargo build --release --target "$(cat /rust_target.txt)" --features "$(cat /rust_features.txt)"
RUN cp ./target/$(cat /rust_target.txt)/release/kube-environment /kube-environment

FROM gcr.io/distroless/cc as application

COPY --from=builder /kube-environment /

EXPOSE 8000
ENTRYPOINT ["./kube-environment"]
