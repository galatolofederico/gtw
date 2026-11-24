# FableFlight multiplayer relay: a static binary in an empty image.
# Build from the repository root (the relay shares src/mp_proto.rs with the game):
#   docker build -f relay/Dockerfile -t fableflight-relay .
FROM rust:1-alpine AS build
WORKDIR /src
COPY relay/Cargo.toml relay/Cargo.lock* relay/
COPY relay/src relay/src
COPY src/mp_proto.rs src/mp_proto.rs
RUN cd relay && cargo build --release && cp target/release/fableflight-relay /fableflight-relay

FROM scratch
COPY --from=build /fableflight-relay /fableflight-relay
USER 65534:65534
# UDP for the players, TCP for the status page (put that behind HTTPS).
EXPOSE 7480/udp 7481/tcp
ENTRYPOINT ["/fableflight-relay"]
