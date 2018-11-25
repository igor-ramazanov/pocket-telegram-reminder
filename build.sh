#!/usr/bin/env bash

docker run --rm -it -v "$(pwd)":/home/rust/src ekidd/rust-musl-builder cargo build --release
TAG=$(cat Cargo.toml | grep version | awk '{print$3;}' | awk -F '"' '{print $2;}')
cp Dockerfile target/x86_64-unknown-linux-musl/release/
docker build -t igorramazanov/pocket-reminder-telegram-bot:"$TAG" target/x86_64-unknown-linux-musl/release/
docker tag igorramazanov/pocket-reminder-telegram-bot:"$TAG" igorramazanov/pocket-reminder-telegram-bot:latest