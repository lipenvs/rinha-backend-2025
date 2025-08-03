FROM rust:1.88 AS builder
WORKDIR /app
COPY . .
RUN rm -rf target
RUN cargo build --release
