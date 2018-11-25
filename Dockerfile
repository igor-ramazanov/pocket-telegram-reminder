FROM alpine

RUN ["/sbin/apk", "add", "--no-cache", "openssl"]

COPY target/x86_64-unknown-linux-musl/release/pocket-reminder-telegram-bot /app

ENTRYPOINT ["/app"]