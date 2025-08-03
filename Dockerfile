FROM rust:1.88 AS builder

LABEL maintainer="github.com/lipenvs"
LABEL description="Rinha de Backend 2025 - API em Rust, feito por Felipe Neves"
LABEL version="1.0"

WORKDIR /app
COPY . .
RUN rm -rf target
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/rinha-backend-2025 /usr/local/bin/app
EXPOSE 3000
CMD ["app"]