# Motion: bundle web/ (react-spring) into static/motion.js. Only needed while building.
FROM oven/bun:1 AS web
WORKDIR /web
COPY web/package.json web/bun.lock ./
RUN bun install --frozen-lockfile
COPY web ./
RUN mkdir -p /static && bun run build

# Build
FROM rust:1-slim-trixie AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations
# static/ is hashed into the build id (asset URLs + service worker version)
COPY static ./static
COPY --from=web /static/motion.js ./static/motion.js
RUN cargo build --release

# Run
FROM debian:trixie-slim
# tzdata so TZ works (dates, months, calendar invites in local time); CA certs for the weather API.
RUN apt-get update \
 && apt-get install -y --no-install-recommends tzdata ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/joy /usr/local/bin/joy
COPY --from=build /src/static ./static
ENV DATABASE_URL="sqlite:///data/joy.db?mode=rwc" PORT=3000
VOLUME /data
EXPOSE 3000
CMD ["joy"]
