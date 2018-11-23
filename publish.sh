#!/usr/bin/env bash

TAG=$(cat Cargo.toml | grep version | awk '{print$3;}' | awk -F '"' '{print $2;}')
echo "$DOCKER_PASSWORD" | docker login -u "$DOCKER_USERNAME" --password-stdin
docker push igorramazanov/pocket-reminder-telegram-bot:"$TAG"
docker push igorramazanov/pocket-reminder-telegram-bot:latest