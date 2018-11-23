#!/usr/bin/env bash

docker run --rm -it -v "$(pwd)":/home/rust/src ekidd/rust-musl-builder cargo build --release
TAG=$(cat Cargo.toml | grep version | awk '{print$3;}' | awk -F '"' '{print $2;}')
docker build -t igorramazanov/pocket-reminder-telegram-bot:"$TAG" .
docker tag igorramazanov/pocket-reminder-telegram-bot:"$TAG" igorramazanov/pocket-reminder-telegram-bot:latest