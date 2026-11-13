#!/bin/sh
# Pack the .hmod locally and run the registry's checks on it on the build VPS
# (cargo-horizon-mod is built there, from the pinned Horizon revision).
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VPS="root@<VPS>"
SSH="ssh -o BatchMode=yes -i $HOME<SSH_KEY>"
python -I "$ROOT/tools/pack.py"
scp -q -o BatchMode=yes -i "$HOME<SSH_KEY>" "$ROOT/target/graveyard.hmod" "$ROOT/target/graveyard.hnmod" "$VPS:/opt/build/graveyard/"
$SSH "$VPS" 'cd /opt/build/graveyard && for f in graveyard.hmod graveyard.hnmod; do echo "== $f"; ./cargo-horizon-mod horizon-mod check $f 2>&1 | tail -40; done'
