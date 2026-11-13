#!/bin/bash
# Run on the build VPS: a headless smoke test of the graveyard port on a
# scratch dedicated server (never /opt/minecraft): 127.0.0.1 only, offline
# mode, one survival bot (horizon-bots), commands over stdin. Adapted from
# the sibling bosses_of_mass_destruction port's smoke script.
#
#   vps_smoke.sh flat     summon the three mobs, fight, kill, loot, sun, /place
#   vps_smoke.sh reload   restart the flat world: saved mobs and blocks come back
#   vps_smoke.sh normal   a default world: /locate the structures
#   vps_smoke.sh loot     a fresh flat world: the mobs' loot tables and drops
PHASE=${1:-flat}
G=/opt/build/graveyard
S=$G/smoke
SRV=$S/server-$([ "$PHASE" = normal ] && echo normal || ([ "$PHASE" = loot ] && echo loot) || echo flat)
RES=$G/resources
ENGINE=$G/engine/horizon-engine
LOG=$S/$PHASE.log
IN=$S/$PHASE.in
PORT=25611
mkdir -p $S
if [ ! -d $RES ]; then cp -al /opt/build/bomd-smoke/resources $RES; fi
pkill -f "[g]raveyard/resources" 2>/dev/null; pkill -f "[h]orizon-bots --server 127.0.0.1:$PORT" 2>/dev/null; sleep 1
if [ "$PHASE" != reload ]; then
  rm -rf $SRV; mkdir -p $SRV/horizon-mods
fi
cp $G/graveyard.hmod $SRV/horizon-mods/
echo "mod.graveyard:true" > $SRV/horizon-mods.txt
echo "eula=true" > $SRV/eula.txt
LEVEL='minecraft\:flat'; [ "$PHASE" = normal ] && LEVEL='minecraft\:normal'
cat > $SRV/server.properties <<P
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-type=$LEVEL
level-seed=20261112
gamemode=survival
force-gamemode=true
allow-flight=true
difficulty=normal
spawn-protection=0
# A debug engine's /locate can take minutes in one tick: no watchdog.
max-tick-time=-1
pause-when-empty-seconds=0
view-distance=4
simulation-distance=4
generate-structures=true
spawn-monsters=false
spawn-animals=false
white-list=false
enforce-whitelist=false
P
: > $IN; : > $LOG
cmd() { echo "[smoke] > $*" >> $LOG; echo "$*" >> $IN; }
note() { echo "[smoke] ===== $* =====" >> $LOG; }
cd $SRV
export HORIZON_NO_AUDIO=1
(tail -f $IN | $ENGINE --resources $RES --dedicated --nogui >> $LOG 2>&1; echo "[smoke] engine exited $?" >> $LOG) &
for i in $(seq 1 400); do grep -q "Done (" $LOG && break; grep -q "engine exited" $LOG && exit 1; sleep 1; done
echo "[smoke] server up after ${i}s" >> $LOG
/opt/build/target/debug/horizon-bots --server 127.0.0.1:$PORT --bots 1 --behaviour idle --height 0 --ground --seconds 900 --name Bot --report $S/$PHASE-bots.json > $S/$PHASE-bots.log 2>&1 &
for i in $(seq 1 60); do grep -q "Bot0 joined" $LOG && break; sleep 1; done
sleep 3
cmd "attribute Bot0 minecraft:max_health base set 1000"
cmd "effect give Bot0 minecraft:instant_health 1 10 true"
sleep 1
cmd "data get entity Bot0 Pos"

if [ "$PHASE" = flat ]; then
  cmd "time set midnight"
  for mob in ghoul revenant nightmare; do
    T="@e[type=graveyard:$mob,limit=1]"
    note "$mob"
    cmd "execute at Bot0 run summon graveyard:$mob ~5 ~ ~"
    sleep 2
    cmd "data get entity $T Health"
    cmd "data get entity $T Pos"
    cmd "data get entity $T Tags"
    cmd "data get entity $T IsBaby"
    cmd "attribute $T minecraft:movement_speed get"
    cmd "attribute $T minecraft:attack_damage get"
    cmd "data get entity Bot0 Health"
    if [ $mob = nightmare ]; then
      # Neutral: it hunts only a player who stares at it or hurts it.
      sleep 5
      cmd "data get entity Bot0 Health"
      note "nightmare: projectiles do not hurt it"
      cmd "damage $T 5 minecraft:arrow"
      sleep 1
      cmd "data get entity $T Health"
      note "nightmare: hurt by the bot"
      cmd "clear Bot0"
    cmd "damage $T 1 minecraft:player_attack by Bot0"
      sleep 1
      cmd "data get entity $T Health"
    fi
    # The ghoul climbs out for 92 ticks first.
    sleep 15
    cmd "data get entity $T Pos"
    cmd "data get entity Bot0 Health"
    sleep 10
    cmd "data get entity Bot0 Health"
    # Hurt by the player first: a mob's loot needs a recent player hit here.
    cmd "clear Bot0"
    cmd "damage $T 1 minecraft:player_attack by Bot0"
    sleep 1
    cmd "damage $T 2000 minecraft:generic"
    sleep 3
    cmd "execute if entity @e[type=graveyard:$mob]"
    cmd "execute if entity @e[type=minecraft:item]"
    cmd "data get entity @e[type=minecraft:item,limit=1] Item"
    cmd "data get entity @e[type=minecraft:item,sort=furthest,limit=1] Item"
    # The bot picks up what drops next to it.
    cmd "data get entity Bot0 Inventory"
    cmd "kill @e[type=minecraft:item]"
    cmd "effect give Bot0 minecraft:instant_health 1 10 true"
    sleep 2
  done
  note "control: a vanilla zombie killed without a player hit"
  cmd "execute at Bot0 run summon minecraft:zombie ~5 ~ ~5 {IsBaby:0b}"
  sleep 1
  cmd "damage @e[type=minecraft:zombie,limit=1] 2000 minecraft:generic"
  sleep 2
  cmd "execute if entity @e[type=minecraft:item]"
  cmd "kill @e[type=minecraft:item]"
  note "sunlight: ghoul and revenant burn, the nightmare does not"
  cmd "time set noon"
  cmd "execute at Bot0 run summon graveyard:ghoul ~-12 ~ ~-12 {NoAI:1b}"
  cmd "execute at Bot0 run summon graveyard:revenant ~-12 ~ ~12 {NoAI:1b}"
  cmd "execute at Bot0 run summon graveyard:nightmare ~12 ~ ~-12 {NoAI:1b}"
  sleep 20
  cmd "data get entity @e[type=graveyard:ghoul,limit=1] Fire"
  cmd "data get entity @e[type=graveyard:revenant,limit=1] Fire"
  cmd "data get entity @e[type=graveyard:nightmare,limit=1] Fire"
  cmd "data get entity @e[type=graveyard:nightmare,limit=1] Health"
  cmd "kill @e[type=graveyard:ghoul]"
  cmd "kill @e[type=graveyard:revenant]"
  cmd "kill @e[type=graveyard:nightmare]"
  cmd "kill @e[type=minecraft:item]"
  cmd "time set midnight"
  note "structures"
  x=40
  for s in small_grave small_desert_grave small_mountain_grave small_savanna_grave small_graveyard small_desert_graveyard medium_graveyard memorial_tree dead_tree mushroom_grave giant_mushroom altar large_graveyard crypt lich_prison; do
    # Far structures are only placed in loaded chunks: take the bot there.
    cmd "tp Bot0 $x -60 0"
    cmd "forceload add $((x - 96)) -96 $((x + 96)) 96"
    sleep 8
    cmd "place structure graveyard:$s $x -60 0"
    sleep 4
    cmd "forceload remove all"
    x=$((x + 96))
  done
  cmd "tp Bot0 0.5 -60 0.5"
  sleep 6
  note "kept for reload"
  cmd "execute at Bot0 run summon graveyard:ghoul ~3 ~ ~3 {PersistenceRequired:1b,NoAI:1b,CustomName:\"keeper\"}"
  cmd "execute at Bot0 run setblock ~2 ~ ~ graveyard:gravestone"
  cmd "execute at Bot0 run setblock ~2 ~ ~1 graveyard:fire_brazier[lit=true]"
  sleep 3
  cmd "data get entity @e[type=graveyard:ghoul,limit=1] Tags"
  cmd "save-all"
  sleep 3
elif [ "$PHASE" = loot ]; then
  # The mobs' loot tables on their own, then each mob killed away from the bot.
  cmd "time set midnight"
  for mob in ghoul revenant nightmare; do
    note "loot table $mob"
    for i in 1 2 3 4 5 6; do cmd "execute at Bot0 run loot spawn ~20 ~ ~20 loot graveyard:entities/$mob"; done
    sleep 2
    cmd "execute if entity @e[type=minecraft:item]"
    cmd "kill @e[type=minecraft:item]"
    note "kill $mob x4"
    for i in 1 2 3 4; do
      cmd "execute at Bot0 run summon graveyard:$mob ~20 ~ ~-20 {NoAI:1b}"
      sleep 1
      cmd "damage @e[type=graveyard:$mob,limit=1] 2000 minecraft:player_attack by Bot0"
      sleep 3
    done
    cmd "execute if entity @e[type=graveyard:$mob]"
    cmd "execute if entity @e[type=minecraft:item]"
    cmd "data get entity @e[type=minecraft:item,limit=1] Item"
    cmd "kill @e[type=minecraft:item]"
  done
  sleep 2
elif [ "$PHASE" = reload ]; then
  note "after restart"
  cmd "execute if entity @e[type=graveyard:ghoul]"
  cmd "data get entity @e[type=graveyard:ghoul,limit=1] Tags"
  cmd "data get entity @e[type=graveyard:ghoul,limit=1] CustomName"
  cmd "execute at Bot0 if block ~2 ~ ~ graveyard:gravestone"
  cmd "execute at Bot0 if block ~2 ~ ~1 graveyard:fire_brazier[lit=true]"
  sleep 2
else
  note "locate"
  # A vanilla structure first, for the time a debug engine takes.
  for s in minecraft:village_plains graveyard:small_graveyard graveyard:crypt graveyard:medium_graveyard graveyard:dead_tree; do
    n=$(grep -c "System chat" $LOG)
    cmd "locate structure $s"
    for i in $(seq 1 400); do [ "$(grep -c "System chat" $LOG)" -gt "$n" ] && break; sleep 1; done
    echo "[smoke] locate $s answered after ${i}s" >> $LOG
  done
fi
cmd "list"
cmd "stop"
for i in $(seq 1 90); do grep -q "engine exited" $LOG && break; sleep 1; done
pkill -f "[h]orizon-bots --server 127.0.0.1:$PORT"; pkill -f "[g]raveyard/resources"; pkill -f "[t]ail -f $IN"
echo "[smoke] DONE" >> $LOG
