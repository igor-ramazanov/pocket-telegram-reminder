FROM alpine

RUN ["/sbin/apk", "add", "--no-cache", "openssl"]

COPY pocket-reminder-telegram-bot /app

ENTRYPOINT ["/app"]