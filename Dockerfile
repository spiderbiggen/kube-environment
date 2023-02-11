# syntax=docker/dockerfile:1
FROM --platform=$BUILDPLATFORM rust:1.67.1 as builder

ARG TARGETPLATFORM
RUN <<-EOF
case "$TARGETPLATFORM" in
  "linux/arm64")
    apt-get update && apt-get install -qq g++-aarch64-linux-gnu libc6-dev-arm64-cross
    export CARGO_BUILD_TARGET=aarch64-unknown-linux-gnu
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
    export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
    export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ ;;
  "linux/amd64")
    export CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu ;;
  *)
    exit 1 ;;
esac
EOF

RUN rustup target add "$CARGO_BUILD_TARGET"

WORKDIR /app/builder
COPY . ./
RUN cargo build --release
RUN cp "/app/builder/target/release/kube-environment /kube-environment"

FROM gcr.io/distroless/cc as application

COPY --from=builder /kube-environment /

EXPOSE 8000
ENTRYPOINT ["./kube-environment"]
