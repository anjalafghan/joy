# Build
FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations
RUN cargo build --release

# Run
FROM debian:trixie-slim
WORKDIR /app
COPY --from=build /src/target/release/joy /usr/local/bin/joy
COPY static ./static
ENV DATABASE_URL="sqlite:///data/joy.db?mode=rwc" PORT=3000
VOLUME /data
EXPOSE 3000
CMD ["joy"]
