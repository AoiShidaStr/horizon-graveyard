//! The server half: the mobs' goals (upstream's `registerGoals`), their attack
//! and rage timers (`customServerAiStep`), the ghoul's climb out of the
//! ground, the nightmare's stare, and the zombie template's habits that
//! upstream's mobs do not have.
//!
//! A translation of The Graveyard's GhoulEntity, RevenantEntity,
//! NightmareEntity and their melee goals (finallion, GPL-3.0-only), changed
//! for Horizon's mob API; see NOTICE.md.
use crate::mobs::{
    EMERGED_TAG, GHOUL_EMERGE_TICKS, GHOUL_RAGE_COOLDOWN, GHOUL_RAGE_TICKS, Kind,
};
use crate::net::{Message, Op};
use horizon_client::mobs::{
    Mob, Species, Who,
    goals::{
        self, Cx, Float, Goal, HurtByTarget, LookAtPlayer, NearestTarget, RandomLookAround,
        Stroll,
    },
};
use horizon_client::mod_api::{
    Compound, EntityTickListenedEvent, EntityTickPostEvent, Flow, Host, LivingConversionPreEvent,
    LivingDeathEvent, LivingIncomingDamageEvent, NativeEffect, ParticleOptions, ParticleType,
    RandomSource, RegisterGoalsEvent, Tag,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

/// What the server keeps of one of our mobs between ticks (upstream's synced
/// entity data). It is not saved: a mob read back from a save starts with no
/// swing or rage running, as upstream's do (their timers are not saved
/// either), and the ghoul's climb is saved as a scoreboard tag.
#[derive(Clone, Debug, Default)]
pub(crate) struct MobState {
    /// `ATTACK_ANIM_TIMER`, counted down each tick.
    pub attack_timer: i32,
    /// Ghoul: `RAGE_ANIM_TIMER`, counted down each tick.
    pub rage_timer: i32,
    /// Ghoul: `IS_RAGING`.
    pub raging: bool,
    /// Ghoul: `SPAWN_TIMER`, the ticks of climbing left.
    pub emerge: i32,
    /// Its first tick has fixed what the zombie template rolled.
    pub ready: bool,
    /// Nightmare: blind the player it teleported towards.
    pub blind: bool,
    /// Messages for the clients near it, sent after its tick.
    pub outbox: Vec<(Op, u16)>,
}

fn states() -> &'static Mutex<HashMap<u32, MobState>> {
    static STATES: OnceLock<Mutex<HashMap<u32, MobState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Runs `f` on mob `id`'s state (made on first use).
pub(crate) fn with_state<R>(id: u32, f: impl FnOnce(&mut MobState) -> R) -> R {
    let mut map = states().lock().unwrap_or_else(|e| e.into_inner());
    f(map.entry(id).or_default())
}

fn kind_of(m: &Mob) -> Option<Kind> {
    Kind::from_id(&m.kind.id)
}

// --------------------------------------------------------------------------
// Goals.

/// Upstream's `registerGoals` for the three mobs, in place of the zombie's.
/// Also asked for a client's copies, where goals never tick; the box change
/// below matters on both sides.
pub fn register_goals(event: &mut RegisterGoalsEvent<'_>) {
    let mob = &mut *event.mob;
    let Some(kind) = kind_of(mob) else {
        return;
    };
    // Upstream's box and eye height, and the nightmare's immunity to the sun
    // (its config's canBurnInSunlight is false), on this mob's own copy of
    // its type: the zombie template's are 0.6 x 1.95 and burning.
    let mut ty = (*mob.kind).clone();
    let (w, h, eye) = kind.size();
    ty.width = w;
    ty.height = h;
    ty.eye_height = eye;
    if kind == Kind::Nightmare {
        ty.flags.burns_in_day = false;
    }
    mob.kind = Arc::new(ty);

    let ai = &mut mob.ai;
    ai.goals.clear();
    ai.targets.clear();
    match kind {
        Kind::Ghoul | Kind::Revenant => {
            ai.goals.add(0, Hold);
            ai.goals.add(1, Float);
            ai.goals.add(2, GraveyardMelee::new(kind));
            ai.goals.add(5, Stroll::avoiding_water(kind.stroll_speed()));
            ai.goals.add(6, LookAtPlayer::new(8.0));
            ai.goals.add(6, RandomLookAround::default());
            ai.targets.add(1, HurtByTarget::new());
            ai.targets.add(2, NearestTarget::player(true));
            ai.targets.add(
                3,
                NearestTarget::species(&[Species::Villager, Species::WanderingTrader], false),
            );
            ai.targets.add(3, NearestTarget::species(&[Species::IronGolem], true));
        }
        Kind::Nightmare => {
            ai.goals.add(0, Float);
            ai.goals.add(1, FreezeWhenStared);
            ai.goals.add(2, GraveyardMelee::new(kind));
            let mut stroll = Stroll::avoiding_water(kind.stroll_speed());
            stroll.water_avoiding = Some(0.0);
            ai.goals.add(7, stroll);
            ai.goals.add(8, LookAtPlayer::new(8.0));
            ai.goals.add(8, RandomLookAround::default());
            ai.targets.add(1, NightmareStare::default());
            ai.targets.add(2, HurtByTarget::new());
        }
    }
}

/// Keeps a climbing or raging ghoul in place: it holds the move, look and jump
/// controls, so no lower goal walks or turns it (upstream: `isImmobile` while
/// the spawn timer runs, the rage's -0.3 speed modifier and its melee goal
/// skipping attacks).
#[derive(Clone, Debug)]
struct Hold;

impl Goal for Hold {
    fn flags(&self) -> u8 {
        goals::MOVE | goals::LOOK | goals::JUMP
    }
    fn can_use(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        with_state(m.id, |s| s.emerge > 0 || s.raging)
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        m.nav_stop();
    }
    fn tick(&mut self, m: &mut Mob, _: &mut Cx) {
        m.nav_stop();
    }
    fn box_clone(&self) -> Box<dyn Goal> {
        Box::new(self.clone())
    }
}

/// `AbstractGraveyardMeleeGoal` with the ghoul's, revenant's and nightmare's
/// `attack`: chase the target, start a swing in reach (the client plays the
/// attack clip), and land the blow `damage_at` ticks into it.
#[derive(Clone, Debug)]
pub(crate) struct GraveyardMelee {
    kind: Kind,
    speed: f64,
    path_target: [f64; 3],
    recalc: i32,
    cooldown: i32,
    ticker: i32,
    can_finish: bool,
    last_check: i64,
}

impl GraveyardMelee {
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            kind,
            speed: 1.0,
            path_target: [0.0; 3],
            recalc: 0,
            cooldown: 0,
            // The goals' constructors start their tickers past the hit.
            ticker: match kind {
                Kind::Ghoul => 14,
                Kind::Revenant => 15,
                Kind::Nightmare => 20,
            },
            can_finish: false,
            last_check: 0,
        }
    }

    /// One tick of `attack(target, squaredDistance)`.
    fn attack(&mut self, m: &mut Mob, who: Who, d2: f64, cx: &mut Cx) {
        let kind = self.kind;
        let reach = kind.reach_sqr();
        let started = with_state(m.id, |s| {
            if kind == Kind::Ghoul && s.raging {
                self.can_finish = false;
                return false;
            }
            if d2 > reach || self.cooldown > 0 {
                return false;
            }
            let free = s.attack_timer <= kind.attack_sync() - kind.attack_duration();
            // The ghoul only resets its cooldown with a swing, the others on
            // every try in reach.
            if kind != Kind::Ghoul {
                self.cooldown = 20;
            }
            if !free {
                return false;
            }
            self.cooldown = 20;
            s.attack_timer = kind.attack_sync();
            s.outbox.push((Op::Attack, kind.attack_sync() as u16));
            // GhoulEntity.customServerAiStep: a swing starts the rage countdown.
            if kind == Kind::Ghoul && s.rage_timer <= 0 {
                s.rage_timer = GHOUL_RAGE_COOLDOWN;
            }
            true
        });
        if started {
            self.ticker = kind.attack_duration();
            self.can_finish = true;
        }
        if self.can_finish && self.ticker == kind.damage_at() {
            if let Some(t) = cx.w.target_info(who)
                && m.distance_sqr(t.position) <= reach + 1.0
            {
                horizon_client::mobs::ai::do_hurt_target(m, who, cx);
            }
            self.can_finish = false;
        }
    }
}

impl Goal for GraveyardMelee {
    fn flags(&self) -> u8 {
        goals::MOVE | goals::LOOK
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let Some(t) = m.target_info(cx.w) else {
            return false;
        };
        // Upstream checks again at once when the target is in reach.
        let in_reach = m.distance_sqr(t.position) <= self.kind.reach_sqr();
        if !in_reach && cx.w.game_time - self.last_check < 20 {
            return false;
        }
        self.last_check = cx.w.game_time;
        if !t.alive {
            return false;
        }
        let at = [t.position.x, t.position.y, t.position.z];
        m.create_path_to_entity(cx.w, at, 0).is_some() || in_reach
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let Some(t) = m.target_info(cx.w) else {
            return false;
        };
        // pauseWhenMobIdle is false for all three: keep on while walking.
        t.alive && !m.ai.nav.is_done()
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(t) = m.target_info(cx.w) {
            let at = [t.position.x, t.position.y, t.position.z];
            m.nav_move_to_entity(cx.w, at, 0, self.speed);
        }
        m.ai.aggressive = true;
        self.recalc = 0;
        self.cooldown = 0;
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(t) = m.target_info(cx.w)
            && t.is_player
            && t.creative
        {
            m.ai.target = None;
        }
        m.ai.aggressive = false;
        m.nav_stop();
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let Some(who) = m.ai.target else {
            return;
        };
        let Some(t) = cx.w.target_info(who) else {
            return;
        };
        m.look_at_speed([t.position.x, t.eye_y, t.position.z], 30.0, 30.0);
        let d2 = m.distance_sqr(t.position);
        self.recalc = (self.recalc - 1).max(0);
        let moved = (t.position.x - self.path_target[0]).powi(2)
            + (t.position.y - self.path_target[1]).powi(2)
            + (t.position.z - self.path_target[2]).powi(2)
            >= 1.0;
        if m.sees(cx.w, who)
            && self.recalc <= 0
            && (self.path_target == [0.0; 3] || moved || m.random.next_float() < 0.05)
        {
            self.path_target = [t.position.x, t.position.y, t.position.z];
            self.recalc = 4 + m.random.next_int_bound(7).unwrap_or(0);
            if d2 > 1024.0 {
                self.recalc += 10;
            } else if d2 > 256.0 {
                self.recalc += 5;
            }
            if !m.nav_move_to_entity(cx.w, self.path_target, 0, self.speed) {
                self.recalc += 15;
            }
        }
        self.cooldown = (self.cooldown - 1).max(0);
        self.ticker -= 1;
        self.attack(m, who, d2, cx);
    }
    fn box_clone(&self) -> Box<dyn Goal> {
        Box::new(self.clone())
    }
}

/// `NightmareEntity.ChasePlayerGoal`: stand still and stare back at the
/// player it hunts while that player looks at it.
#[derive(Clone, Debug)]
struct FreezeWhenStared;

impl Goal for FreezeWhenStared {
    fn flags(&self) -> u8 {
        goals::JUMP | goals::MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if m.ai.target != Some(Who::Player) {
            return false;
        }
        let Some(p) = &cx.w.player else {
            return false;
        };
        m.distance_sqr(p.position) <= 256.0 && horizon_client::mobs::enderman::stared_at(m, cx.w)
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        m.nav_stop();
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(p) = &cx.w.player {
            m.look_at([p.position.x, p.eye[1], p.position.z]);
        }
    }
    fn box_clone(&self) -> Box<dyn Goal> {
        Box::new(self.clone())
    }
}

/// `NightmareEntity.TeleportTowardsPlayerGoal`: a player who stares at it
/// becomes its target after a moment; while that player keeps staring from
/// further than ~5 blocks (and within ~20) it blinds them and teleports
/// towards them, and it closes in by teleporting when it is not looked at
/// and more than 16 blocks away.
#[derive(Clone, Debug, Default)]
struct NightmareStare {
    pending: bool,
    warmup: i32,
    unseen: i32,
}

fn huntable(cx: &Cx) -> bool {
    cx.w.player
        .as_ref()
        .is_some_and(|p| p.alive && !p.creative && !p.spectator)
}

impl Goal for NightmareStare {
    fn flags(&self) -> u8 {
        goals::TARGET
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let Some(p) = &cx.w.player else {
            return false;
        };
        let range = m.attrs.follow_range;
        self.pending = huntable(cx)
            && m.distance_sqr(p.position) <= range * range
            && horizon_client::mobs::enderman::stared_at(m, cx.w);
        self.pending
    }
    fn start(&mut self, _: &mut Mob, _: &mut Cx) {
        self.warmup = goals::reduced(5);
        self.unseen = 0;
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        self.pending = false;
        m.ai.target = None;
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let Some(p) = cx.w.player.clone() else {
            return false;
        };
        if self.pending {
            if !horizon_client::mobs::enderman::stared_at(m, cx.w) {
                return false;
            }
            m.turn_to(p.eye, 10.0, 10.0);
            return true;
        }
        huntable(cx) && m.ai.target == Some(Who::Player)
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let Some(p) = cx.w.player.clone() else {
            return;
        };
        if self.pending {
            self.warmup -= 1;
            if self.warmup <= 0 {
                self.pending = false;
                m.ai.target = Some(Who::Player);
            }
            return;
        }
        if m.ai.target != Some(Who::Player) {
            return;
        }
        let d2 = m.distance_sqr(p.position);
        if horizon_client::mobs::enderman::stared_at(m, cx.w) {
            if d2 <= 384.0 && d2 > 24.0 {
                with_state(m.id, |s| s.blind = true);
                teleport_towards(m, cx, [p.position.x, p.position.y, p.position.z], p.eye[1]);
            }
            self.unseen = 0;
        } else if d2 > 256.0 {
            let waited = self.unseen;
            self.unseen += 1;
            if waited >= goals::reduced(80)
                && teleport_towards(m, cx, [p.position.x, p.position.y, p.position.z], p.eye[1])
            {
                self.unseen = 0;
            }
        }
    }
    fn box_clone(&self) -> Box<dyn Goal> {
        Box::new(self.clone())
    }
}

/// `NightmareEntity.teleportTo(entity)`: a spot 16 blocks from where it
/// stands, towards the target, give or take a block.
fn teleport_towards(m: &mut Mob, cx: &mut Cx, at: [f64; 3], eye_y: f64) -> bool {
    let mut d = [
        m.position.x - at[0],
        m.position.y + f64::from(m.height()) * 0.5 - eye_y,
        m.position.z - at[2],
    ];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0e-9);
    d = d.map(|c| c / len);
    let x = m.position.x + (m.random.next_double() - 0.5) * 2.0 - d[0] * 16.0;
    let y = m.position.y + f64::from(m.random.next_int_bound(4).unwrap_or(2) - 2) - d[1] * 16.0;
    let z = m.position.z + (m.random.next_double() - 0.5) * 2.0 - d[2] * 16.0;
    horizon_client::mobs::enderman::teleport_to(m, cx.w, x, y, z, cx.events)
}

// --------------------------------------------------------------------------
// Ticks.

/// The game tells entity ticks only to a mod that asks.
pub fn entity_tick_listened(_host: &mut dyn Host, event: &mut EntityTickListenedEvent) {
    event.listened = true;
}

/// One of our mobs after its tick: its timers (`customServerAiStep`), the
/// zombie template's rolls undone on its first tick, and the messages and
/// effects its goals asked for.
pub fn entity_tick_post(host: &mut dyn Host, event: &mut EntityTickPostEvent<'_>) {
    let Some(kind) = Kind::from_id(event.kind) else {
        return;
    };
    let id = event.entity;
    let Some(handle) = host.entity(id) else {
        return;
    };
    let Some(pos) = handle.position(host) else {
        return;
    };

    if !with_state(id, |s| s.ready) {
        first_tick(host, handle, kind, id);
    }

    // The timers, as customServerAiStep runs them.
    enum Change {
        None,
        RageStart,
        RageEnd,
    }
    let (change, emerging, outbox, blind) = with_state(id, |s| {
        let mut change = Change::None;
        if kind == Kind::Ghoul && s.rage_timer == GHOUL_RAGE_TICKS {
            s.raging = true;
            s.outbox.push((Op::Rage, GHOUL_RAGE_TICKS as u16));
            change = Change::RageStart;
        }
        if s.attack_timer > 0 {
            s.attack_timer -= 1;
        }
        if s.rage_timer > 0 {
            s.rage_timer -= 1;
        }
        if s.rage_timer == 0 && s.raging {
            s.raging = false;
            change = Change::RageEnd;
        }
        let emerging = s.emerge > 0;
        if emerging {
            s.emerge -= 1;
        }
        let blind = std::mem::take(&mut s.blind);
        (change, emerging, std::mem::take(&mut s.outbox), blind)
    });

    match change {
        Change::RageStart => {
            let _ = host.play_sound("graveyard:entity.ghoul.roar", pos, 5, 1.0, 1.0);
            // The rage's -0.3 speed modifier takes the ghoul's 0.175 below 0.
            let _ = host.entity_set_attribute_base(handle, "minecraft:movement_speed", 0.0);
        }
        Change::RageEnd => {
            let _ = host.entity_set_attribute_base(handle, "minecraft:movement_speed", kind.speed());
        }
        Change::None => {}
    }

    if emerging {
        emerge_particles(host, event.dimension, pos);
    }

    if blind {
        // The player it teleported towards: the nearest one it may hunt.
        let players = host.server_players();
        if let Some(player) = players
            .iter()
            .filter(|p| p.dimension == event.dimension && matches!(p.game_mode, 0 | 2))
            .min_by(|a, b| dist2(a.position, pos).total_cmp(&dist2(b.position, pos)))
            .filter(|p| dist2(p.position, pos) < 48.0 * 48.0)
        {
            let _ = host.add_effect(
                player.uuid,
                &NativeEffect {
                    id: "minecraft:blindness".to_owned(),
                    amplifier: 0,
                    duration: 100,
                    ambient: false,
                    particles: true,
                },
            );
        }
    }

    if !outbox.is_empty() {
        let players = host.server_players();
        for (op, ticks) in outbox {
            let message = Message { op, entity: id, ticks }.encode();
            for player in players
                .iter()
                .filter(|p| p.dimension == event.dimension && dist2(p.position, pos) < 96.0 * 96.0)
            {
                let _ = host.net_send_to_player(player.uuid, crate::net::CHANNEL, &message);
            }
        }
    }
}

/// The first tick this server sees mob `id`: undo what the zombie template
/// rolled in `finalizeSpawn` (a baby, picking up loot, breaking doors,
/// reinforcements of its own kind when hurt), and start a new ghoul's climb.
fn first_tick(host: &mut dyn Host, handle: horizon_client::mod_api::EntityRef, kind: Kind, id: u32) {
    let _ = host.entity_set_attribute_base(handle, "minecraft:spawn_reinforcements", 0.0);
    let mut nbt = Compound::new();
    nbt.insert("IsBaby".into(), Tag::Byte(0));
    nbt.insert("CanPickUpLoot".into(), Tag::Byte(0));
    nbt.insert("CanBreakDoors".into(), Tag::Byte(0));
    let mut emerge = false;
    if kind == Kind::Ghoul {
        // GhoulEntity: every new ghoul climbs out of the ground (its spawn
        // timer starts at SPAWN_ANIMATION_DURATION); a saved one does not.
        let mut tags: Vec<Tag> = match host.entity_nbt(handle).map(Tag::Compound) {
            Some(root) => match root.get("Tags") {
                Some(Tag::List(list)) => list.clone(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
        let done = tags
            .iter()
            .any(|t| matches!(t, Tag::String(s) if s.to_string_lossy() == EMERGED_TAG));
        if !done {
            tags.push(Tag::String(EMERGED_TAG.into()));
            nbt.insert("Tags".into(), Tag::List(tags));
            emerge = true;
        }
    }
    let _ = host.entity_merge_nbt(handle, nbt);
    with_state(id, |s| {
        s.ready = true;
        if emerge {
            s.emerge = GHOUL_EMERGE_TICKS;
            s.outbox.push((Op::Emerge, GHOUL_EMERGE_TICKS as u16));
        }
    });
}

/// GhoulEntity.tick while climbing: crumbs of the block it stands on.
fn emerge_particles(host: &mut dyn Host, dimension: &str, pos: [f64; 3]) {
    let below = [pos[0].floor() as i32, (pos[1] - 0.2).floor() as i32, pos[2].floor() as i32];
    let Some(level) = host.level(dimension) else {
        return;
    };
    let Some(block) = host.level_block(level, below) else {
        return;
    };
    if block == "minecraft:air" {
        return;
    }
    let Some(state) = host.default_state(&block) else {
        return;
    };
    let options = ParticleOptions::simple(ParticleType::Block);
    let _ = host.send_particles(&options, Some(state), [pos[0], pos[1] + 0.1, pos[2]], [0.35, 0.0, 0.35], 0.0, 10);
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Upstream's mobs never turn into another mob: the zombie template would
/// become a drowned underwater.
pub fn living_conversion_pre(event: &mut LivingConversionPreEvent<'_>) -> Flow {
    if Kind::from_id(event.kind).is_some() {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// `NightmareEntity.hurtServer`: projectiles do not hurt the nightmare.
pub fn living_incoming_damage(event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
    if Kind::from_id(event.kind) == Some(Kind::Nightmare) && is_projectile(event.damage_type) {
        return Flow::Cancel;
    }
    Flow::Continue
}

/// `#minecraft:is_projectile`, vanilla 26.3.
fn is_projectile(damage_type: &str) -> bool {
    matches!(
        damage_type,
        "minecraft:arrow"
            | "minecraft:trident"
            | "minecraft:mob_projectile"
            | "minecraft:unattributed_fireball"
            | "minecraft:fireball"
            | "minecraft:wither_skull"
            | "minecraft:thrown"
            | "minecraft:wind_charge"
            | "minecraft:spit"
    )
}

/// A dead mob's state goes with it.
pub fn living_death(event: &mut LivingDeathEvent<'_>) -> Flow {
    if Kind::from_id(event.kind).is_some()
        && let Ok(mut map) = states().lock()
    {
        map.remove(&event.entity);
    }
    Flow::Continue
}

/// One of our mobs left its level without dying (despawned, discarded, its
/// chunk unloaded, gone through a portal): its state goes too, or a long
/// running server would keep one entry per despawned mob. A mob that comes
/// back with its chunk starts afresh, as one read from a save does.
pub fn entity_leave_level(entity: u32, kind: &str) {
    if Kind::from_id(kind).is_some()
        && let Ok(mut map) = states().lock()
    {
        map.remove(&entity);
    }
}

/// A world closing drops every state: entity ids start again in the next.
pub fn clear() {
    if let Ok(mut map) = states().lock() {
        map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projectiles_only_spare_the_nightmare() {
        assert!(is_projectile("minecraft:arrow"));
        assert!(!is_projectile("minecraft:player_attack"));
    }

    #[test]
    fn state_is_per_mob() {
        with_state(7, |s| s.attack_timer = 5);
        assert_eq!(with_state(7, |s| s.attack_timer), 5);
        assert_eq!(with_state(8, |s| s.attack_timer), 0);
        clear();
        assert_eq!(with_state(7, |s| s.attack_timer), 0);
    }

    #[test]
    fn leaving_the_level_forgets_only_our_mobs() {
        with_state(9001, |s| s.attack_timer = 3);
        entity_leave_level(9001, "minecraft:zombie");
        assert_eq!(with_state(9001, |s| s.attack_timer), 3);
        entity_leave_level(9001, "graveyard:ghoul");
        assert_eq!(with_state(9001, |s| s.attack_timer), 0);
    }
}
