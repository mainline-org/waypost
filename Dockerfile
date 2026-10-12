FROM rust:1.96-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY dashboard ./dashboard
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN groupadd --gid 10001 waypost && useradd --uid 10001 --gid waypost --no-create-home waypost && mkdir /data && chown waypost:waypost /data
COPY --from=build /app/target/release/waypost /usr/local/bin/waypost
USER waypost
VOLUME ["/data"]
EXPOSE 8787
ENTRYPOINT ["waypost"]
CMD ["serve", "--data", "/data", "--listen", "0.0.0.0:8787"]
