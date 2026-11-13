#!/bin/bash
# Run on the build VPS after vps_stage.sh: build the dedicated server engine
# (horizon-native-game without default features) with the graveyard mod and
# keep a copy of it, since the shared target's binary is overwritten by the
# other mods' builds.
. ~/.cargo/env
export CARGO_TARGET_DIR=/opt/build/target CARGO_BUILD_JOBS=4
cd /opt/build/wt/graveyard
echo "== server engine build"; date
nice -n 10 cargo build -p horizon-native-game --no-default-features --message-format short 2>&1 | grep -vE "^\s+(Compiling|Checking)"
code=${PIPESTATUS[0]}
echo "build_exit=$code"; date
if [ "$code" = 0 ]; then
  mkdir -p /opt/build/graveyard/engine
  cp /opt/build/target/debug/horizon-client /opt/build/graveyard/engine/horizon-engine
  # The copy must be ours: the mod's channel name is compiled in.
  echo "graveyard channel strings: $(grep -c 'graveyard:anim' /opt/build/graveyard/engine/horizon-engine)"
  ls -la /opt/build/graveyard/engine
fi
echo DONE
