#!/usr/bin/env bash
# One-time Fly.io setup for joy: creates the app and its volume, copies your secrets from .env
# into Fly (they never touch git), deploys, and uploads your current data/joy.db.
#
#   ./scripts/fly-setup.sh            # app name from fly.toml (joy-jar)
#   APP=my-joy ./scripts/fly-setup.sh # if that name is taken (also update `app` in fly.toml)
#
# Later deploys are just: fly deploy
set -euo pipefail
cd "$(dirname "$0")/.."

APP="${APP:-$(sed -n "s/^app = '\(.*\)'/\1/p" fly.toml)}"
REGION="${REGION:-$(sed -n "s/^primary_region = '\(.*\)'/\1/p" fly.toml)}"
ORG="${ORG:-personal}"

# Read KEY from .env, dropping one pair of surrounding quotes.
get() { sed -n "s/^$1=//p" .env | tail -1 | sed -e "s/^['\"]//" -e "s/['\"]\$//"; }

if [ -z "$(get APP_PASSCODE_HASH)" ]; then
  echo "Set your passcode first:  ./target/release/joy hash-passcode  (then paste the lines into .env)" >&2
  exit 1
fi

echo "==> App $APP in $REGION"
# Capture the lists first: with pipefail, `fly … | grep -q` can fail on a broken pipe even
# when grep matched. Columns are tab-separated, so match the name plus any whitespace.
apps="$(fly apps list)"
grep -qE "^$APP[[:space:]]" <<<"$apps" || fly apps create "$APP" --org "$ORG"
volumes="$(fly volumes list -a "$APP")"
grep -q joy_data <<<"$volumes" || fly volumes create joy_data --size 1 --region "$REGION" -a "$APP" -y

echo "==> Secrets (from .env, plus a fresh session secret)"
{
  for k in APP_PASSCODE_HASH PERSON_A PERSON_B HOME_A HOME_B HOME_A_AREA HOME_B_AREA; do
    v="$(get "$k")"
    [ -n "$v" ] && printf '%s=%s\n' "$k" "$v"
  done
  printf 'SESSION_SECRET=%s\n' "$(openssl rand -hex 32)"
} | fly secrets import -a "$APP"

echo "==> Deploy"
fly deploy -a "$APP" --remote-only

echo "==> Upload your data"
snapshot="$(mktemp -t joy-upload).db"
sqlite3 data/joy.db ".backup '$snapshot'"   # consistent copy, safe while the local app runs
fly machine start -a "$APP" "$(fly machines list -a "$APP" -q | head -1)" >/dev/null 2>&1 || true
echo "put $snapshot /data/joy.db.new" | fly ssh sftp shell -a "$APP"
fly ssh console -a "$APP" -C "sh -c 'mv /data/joy.db.new /data/joy.db && rm -f /data/joy.db-wal /data/joy.db-shm'"
fly apps restart "$APP"
rm -f "$snapshot"

echo
echo "Done: https://$APP.fly.dev"
