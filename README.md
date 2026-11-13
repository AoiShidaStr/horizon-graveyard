# The Graveyard for Horizon

A Horizon port of [The Graveyard](https://github.com/finallion/The_Graveyard)
by finallion (GPL-3.0-only), version 4.0.0 (branch `26.3`, commit
`cfb1cc2`). On the Saison 2 server it replaces Born in Chaos: graveyards,
crypts and three of its horror mobs.

- Content mod `graveyard.hmod` (`side = "both"`): 74 blocks, 2 items, 15
  structures with their templates, pools and biome tags, the Ghoul, Revenant
  and Nightmare entity types with their loot, tags and natural spawns, sounds,
  textures, names (all of upstream's languages).
- Native trait mod `graveyard.hnmod`: the three mobs' goals, attack timings
  and special moves on the server; upstream's GeckoLib models and animations,
  glowing eyes and sounds on the client.

This is a **translation** of the GPL upstream (assets, data, models and
behaviour), not a clean-room rewrite. `NOTICE.md` lists what was copied and
what was changed.

## Install

Both halves use the id `graveyard`.

1. `graveyard.hmod` in `horizon-mods/`, `graveyard.hnmod` in
   `horizon-native-mods/`, and `mod.graveyard:true` in `horizon-mods.txt`.
2. Build the native half into the game (launcher **Mods** page, or
   `horizon-launcher native-build <folder> --trust-native-code`; a dedicated
   server needs `--server-output <dir>`).
3. The server and every client need the same `.hmod`.

```
/summon graveyard:ghoul
/summon graveyard:revenant
/summon graveyard:nightmare
/place structure graveyard:small_graveyard
/locate structure graveyard:crypt
```

## Status

Checked on 2026-11-12 against Horizon `a85af17`, on the build VPS: the
dedicated server engine was built with this mod (`tools/vps_build.sh`) and run
on scratch worlds with one bot (`tools/vps_smoke.sh flat|reload|normal|loot`).

| Part | State | How it was checked |
| --- | --- | --- |
| `graveyard.hmod` | done | `cargo horizon-mod check`: valid, content only |
| `graveyard.hnmod` | done | `cargo horizon-mod check`: valid bundle, format `native-1/a85af17…/5a64af9…` |
| Native crate, server flavour | compiles | `cargo check -p mod-graveyard` (horizon-client without default features) |
| Native crate, client flavour | compiles | `cargo check -p horizon-native-game` (default features) |
| Unit tests | 11 pass | `cargo test -p mod-graveyard` (models build, animation sampling, net messages, timings) |
| Dedicated server engine with the mod | builds, runs | `cargo build -p horizon-native-game --no-default-features`; loads the content mod, no "content mods not loaded" error |
| `/summon` ghoul, revenant, nightmare | works | upstream's health, speed and damage read back with `/attribute`; never babies |
| Mobs hunt and hit a player | works | the bot's health drops while ghoul and revenant chase it; the nightmare after being hurt by it |
| Ghoul climbs out of the ground | works (server) | new ghouls hold still for 92 ticks and get the `graveyard.emerged` tag; saved ones do not climb again |
| Nightmare ignores projectiles | works | `/damage … minecraft:arrow`: "invulnerable to the given damage type"; a player hit hurts it |
| Sunlight | works | ghoul and revenant catch fire at noon, the nightmare does not |
| Death and loot | works | `/loot spawn` of the three tables and kills drop rotten flesh, bones and `graveyard:corruption` |
| Save and reload | works | a named ghoul, a gravestone and a lit brazier come back after a restart |
| `/place structure` | works | all 15 structures generate on a flat world (large_graveyard needs its chunks loaded) |
| `/locate structure` | works | small_graveyard, crypt, medium_graveyard and dead_tree found in a default world (1.1 to 2.1 km away, seed 20261112) |
| Models, animations, eyes, sounds on a client | compiled, **not seen** | no client could be run on the VPS (no display); the drawing code compiles in the client flavour only |

Ported:

- 15 structures as vanilla jigsaws: small_grave, small_desert_grave,
  small_mountain_grave, small_savanna_grave, small_graveyard,
  small_desert_graveyard, medium_graveyard, large_graveyard, crypt, dead_tree,
  memorial_tree, mushroom_grave, giant_mushroom, and lich_prison and altar as
  set pieces (no ritual). Their structure sets, pools, biome tags and spawners.
- 74 blocks (the ones those structures place) on vanilla templates, with
  their models, textures, loot, recipes and a creative tab; 2 items
  (corruption, dark iron ingot).
- The Ghoul (nine skins, climb out of the ground, rage roar after its first
  swing, run or crawl), the Revenant and the Nightmare (stare to anger it,
  blinds and teleports towards the player who stares at it, immune to
  projectiles and sunlight): upstream's attributes, goals, attack timings,
  loot, entity tags and natural spawns (NeoForge `add_spawns` with upstream's
  default weights).

Known differences from upstream:

- Hitboxes: the mod sets upstream's sizes on each mob's copy of its type, but
  the zombie template's 0.6 x 1.95 may still be what the server uses for
  collisions; not measured.
- Animation timing comes from the server by a small network message
  (`graveyard:anim`), since Horizon's mob render state has no slot for
  upstream's synced timers. A client that misses it shows the walk/idle clip.
- The ghoul's skin is picked from its network id, not its UUID, so the same
  ghoul can change skin between sessions.
- Nightmare: upstream's random teleport in daylight and its loss of interest
  beyond 32 blocks are not ported.
- Dead mobs disappear after 20 ticks (Horizon), upstream keeps the body up to
  49; the death clip is played faster.
- Sounds: the zombie template's sounds at our mobs are swapped for
  upstream's on the client. Untested in a running client.

Missing, in priority order:

1. The Lich / Corrupted Champion boss: the altar ritual (Vial of Blood, Bone
   Dagger, Bone Staff fragments), phases, skull projectiles, boss bar, and a
   4-block-tall hitbox.
2. The other mobs: Reaper (flying), Wraith, Ghouling (companion), Skeleton
   Creeper, Falling Corpse, Tormented Villager, Acolyte and the corrupted
   illagers, the corvids (raven, crow, magpie), the Nameless Hanged trader.
3. Block entities: gravestone text, urn and sarcophagus storage and lid
   animation, brazier, the ossuary carving menu. These blocks place and save
   as plain blocks.
4. The Chronicle and the lore books.
5. haunted_house, ruins and the five overrun villages (they need upstream's
   Java processors, illagers and villagers).

## Build

```
python -I tools/port.py <upstream checkout>        # content/ from upstream
python -I tools/geckolib2rs.py <upstream checkout> # mixins/graveyard/api/src/geo_data.rs
python -I tools/pack.py                            # target/graveyard.hmod, target/graveyard.hnmod
sh tools/vps_check_hmod.sh                         # cargo horizon-mod check, on the build VPS
sh tools/vps_sync.sh                               # copy to the VPS and stage the crate
```

On the VPS (`/opt/build/graveyard/`): `check.sh full` type-checks the crate
(server flavour), runs its tests and checks the client flavour;
`build.sh` builds the dedicated server engine; `smoke.sh` runs it on a scratch
world with a bot. The upstream checkout is the GitHub tarball of the commit
above.

Pins: Horizon `a85af177a82504f357ff10d5d40d0b7230e2ac32`, native hooks API
`415`, game `26.3`.

## License

GPL-3.0-only (`LICENSE`). Credits and the record of changes are in
`NOTICE.md`.
