FROM rust:1-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd -r -u 10001 mk && mkdir -p /data /var/data && chown mk /data /var/data
COPY --from=build /app/target/release/masterkey-server /usr/local/bin/masterkey-server
ENV PORT=8080 MK_DATABASE_URL=sqlite:///data/masterkey.db?mode=rwc
EXPOSE 8080
# A mounted disk (Render: /var/data) arrives owned by root: hand it to mk, then run as mk.
CMD ["sh", "-c", "chown mk /var/data 2>/dev/null; exec setpriv --reuid=mk --regid=mk --init-groups masterkey-server"]
