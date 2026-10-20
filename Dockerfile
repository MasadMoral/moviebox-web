FROM rust:1-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential git ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/moviebox-web /app/moviebox-web
COPY static /app/static
ENV MBW_ADDR=0.0.0.0:8787
EXPOSE 8787
CMD ["/app/moviebox-web"]
