# The Bobix relay image: the binary only, no shell, a non-root user.
#
# Build from the repository root, because the relay depends on crates/bobix-link by path:
#   docker build -f relay/Dockerfile -t bobix-relay .
#
# Settings are environment variables (see relay/README.md). Required to start:
#   RELAY_PUBLIC_URL   the address devices connect to, such as wss://relay.example
#   RELAY_ID           the id push grants name
# Already set in this image: RELAY_ADDR, REVOKED_DIR and LOG_DIR (all under /data, so mount a volume there).
# For a beta also set INVITE_IDS or INVITE_FILE (such as /data/invites.txt) and, behind a proxy you trust, CLIENT_IP_HEADER.
# Health check: GET /healthz answers "ok" (the image has no shell or curl, so the platform does the check).

# Both base images are pinned by digest (the multi-arch index). To bump: pull the new tag and copy its digest
# (`docker buildx imagetools inspect rust:1.90-bookworm`, same for the distroless tag), then rebuild and run the checks.
FROM rust:1.90-bookworm@sha256:3914072ca0c3b8aad871db9169a651ccfce30cf58303e5d6f2db16d1d8a7e58f AS build
WORKDIR /src
COPY crates/bobix-link crates/bobix-link
COPY relay relay
WORKDIR /src/relay
RUN cargo build --release --locked \
 && cp target/release/bobix-relay /relay-bin \
 && mkdir -p /data/revoked /data/logs

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
COPY --from=build /relay-bin /relay
COPY --from=build --chown=65532:65532 /data /data
ENV RELAY_ADDR=0.0.0.0:8787 \
    REVOKED_DIR=/data/revoked \
    LOG_DIR=/data/logs
VOLUME /data
EXPOSE 8787
USER 65532:65532
ENTRYPOINT ["/relay"]
