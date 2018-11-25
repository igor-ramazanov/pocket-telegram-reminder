#!/bin/sh

# Persistence file used for credentials storing
# Needed for saving credentials between container restarts
SETTINGS_FILE_NAME="pocket-reminder-telegram-settings.txt"
touch "$SETTINGS_FILE_NAME"
docker run \
    -d \
    --name pocket-reminder-telegram-bot \
    -v "$(pwd)"/"$SETTINGS_FILE_NAME":/settings.txt \
    -e TELEGRAM_BOT_API_TOKEN=XXXXXXXXXXXXXXXXXXXXXXXX \
    -e POCKET_API_CONSUMER_KEY=XXXXXXXXXXXXXXXXXXXXXXXX \
    igorramazanov/pocket-reminder-telegram-bot:latest