FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 mk && mkdir /data && chown mk /data
COPY --from=build /app/target/release/masterkey-server /usr/local/bin/masterkey-server
USER mk
ENV PORT=8080 MK_DATABASE_URL=sqlite:///data/masterkey.db?mode=rwc
EXPOSE 8080
CMD ["masterkey-server"]
