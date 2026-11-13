#!/bin/sh
# From this PC: copy the mod's sources to the build VPS and stage them.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VPS="root@<VPS>"
KEY="$HOME<SSH_KEY>"
ssh -o BatchMode=yes -i "$KEY" "$VPS" 'rm -rf /opt/build/mods/graveyard && mkdir -p /opt/build/mods/graveyard'
tar -C "$ROOT" --exclude=./target --exclude=./.build --exclude=__pycache__ -cf - . | \
  ssh -o BatchMode=yes -i "$KEY" "$VPS" 'tar --no-same-owner -C /opt/build/mods/graveyard -xf - && bash /opt/build/mods/graveyard/tools/vps_stage.sh'
