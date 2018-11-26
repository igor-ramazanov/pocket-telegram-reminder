#!/bin/sh

# Persistence file used for credentials storing
# Needed for saving credentials between container restarts
SETTINGS_FILE_NAME="pocket-reminder-telegram-settings.txt"

# Telegram Bot API token, more on -> https://core.telegram.org/bots#3-how-do-i-create-a-bot
TELEGRAM_BOT_API_TOKEN="FIX ME FIX ME FIX ME"

# Pocket API consumer key, more on -> https://getpocket.com/developer/docs/authentication
POCKET_API_CONSUMER_KEY="FIX ME FIX ME FIX ME"

touch "$SETTINGS_FILE_NAME"
docker run \
    -d \
    --name pocket-reminder-telegram-bot \
    -v "$(pwd)"/"$SETTINGS_FILE_NAME":/settings.txt \
    -e TELEGRAM_BOT_API_TOKEN="$TELEGRAM_BOT_API_TOKEN" \
    -e POCKET_API_CONSUMER_KEY="$POCKET_API_CONSUMER_KEY" \
    igorramazanov/pocket-reminder-telegram-bot:latest