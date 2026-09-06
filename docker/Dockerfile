# Control-plane API image. Build from the repository root:
#   docker build -f docker/Dockerfile -t micp-api:local .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY configs ./configs
RUN cargo build -p micp-api --release

FROM debian:bookworm-slim
RUN useradd --system --home /var/empty --shell /usr/sbin/nologin micp
COPY --from=build /src/target/release/micp-api /usr/local/bin/micp-api
COPY configs /etc/micp
USER micp
ENV MICP_CONFIG=/etc/micp/default.toml
ENV MICP_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/micp-api"]
