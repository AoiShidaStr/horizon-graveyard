//! The client half: the three mobs drawn with upstream's models and
//! animations (`geo.rs`) in place of the zombie template's, their glowing
//! eyes, and their own sounds in place of the zombie's.
//!
//! A translation of The Graveyard's GhoulRenderer, RevenantRenderer,
//! NightmareRenderer, GraveyardEyesLayer and the entities'
//! `registerControllers` animation tables (finallion, GPL-3.0-only), changed
//! for Horizon's mob drawing hooks; see NOTICE.md.
use crate::geo::{self, ClipId, Pose};
use crate::mobs::{GHOUL_RUN_SPEED, Kind, TRANSITION_TICKS};
use crate::net::{Message, Op};
use horizon_client::mod_api::{
    BlockEntityDraw, Flow, Host, MobSubmitEvent, ModelPart, NetFromServerEvent, SoundPlayEvent,
};
use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU32, Ordering},
    },
};

/// Client ticks since the game started, to time what the server announced.
static TICKS: AtomicU32 = AtomicU32::new(0);

/// Full brightness in the draw's light (16 x sky light in bits 0..8, 16 x
/// block light in bits 8..16): the eyes layer's `FULL_LIGHT`.
const FULL_BRIGHT: u32 = 0xF0F0;

/// Each kind's model, built once and posed in place each frame.
fn models() -> &'static Mutex<HashMap<Kind, ModelPart>> {
    static MODELS: OnceLock<Mutex<HashMap<Kind, ModelPart>>> = OnceLock::new();
    MODELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What the server said a mob started, by client tick.
#[derive(Clone, Copy, Debug, Default)]
struct Announced {
    attack: Option<(u32, u16)>,
    emerge: Option<(u32, u16)>,
    rage: Option<(u32, u16)>,
}

/// The clip a mob showed last, for the blend into the next.
#[derive(Clone, Debug)]
struct Shown {
    clip: ClipId,
    /// The frame's age when it started.
    since: f32,
    /// The pose it left, blended out over `TRANSITION_TICKS`.
    from: Option<Pose>,
    /// The pose drawn last frame.
    last: Pose,
    /// When it was last drawn, to forget mobs out of sight.
    seen: u32,
}

#[derive(Default)]
struct Anim {
    announced: HashMap<u32, Announced>,
    shown: HashMap<u32, Shown>,
}

fn anim() -> &'static Mutex<Anim> {
    static ANIM: OnceLock<Mutex<Anim>> = OnceLock::new();
    ANIM.get_or_init(|| Mutex::new(Anim::default()))
}

/// Where our mobs are, refreshed each client tick, for the sound hook (which
/// gets no host).
fn positions() -> &'static Mutex<Vec<(Kind, [f64; 3])>> {
    static POSITIONS: OnceLock<Mutex<Vec<(Kind, [f64; 3])>>> = OnceLock::new();
    POSITIONS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Sounds to play in place of a zombie's, on the next client tick.
fn queued() -> &'static Mutex<Vec<(&'static str, [f64; 3], f32, f32)>> {
    static QUEUED: OnceLock<Mutex<Vec<(&'static str, [f64; 3], f32, f32)>>> = OnceLock::new();
    QUEUED.get_or_init(|| Mutex::new(Vec::new()))
}

/// One client tick: count it, play the queued sounds, note where our mobs are.
pub fn tick(host: &mut dyn Host) {
    let now = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    let sounds: Vec<_> = queued().lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default();
    for (sound, at, volume, pitch) in sounds {
        let _ = host.play_local_sound(sound, at, volume, pitch, 0);
    }
    let ours: Vec<(Kind, [f64; 3])> = host
        .client_entities()
        .iter()
        .filter_map(|e| Kind::from_id(&e.kind).map(|k| (k, e.position)))
        .collect();
    if let Ok(mut p) = positions().lock() {
        *p = ours;
    }
    // Forget the mobs not drawn for ten seconds.
    if now % 200 == 0
        && let Ok(mut a) = anim().lock()
    {
        a.shown.retain(|_, s| now.wrapping_sub(s.seen) < 200);
        let shown: Vec<u32> = a.shown.keys().copied().collect();
        a.announced.retain(|id, _| shown.contains(id));
    }
}

/// The server announced a swing, a climb or a rage.
pub fn net_from_server(event: &mut NetFromServerEvent<'_>) {
    if event.channel != crate::net::CHANNEL {
        return;
    }
    let Some(m) = Message::decode(event.data) else {
        return;
    };
    let now = TICKS.load(Ordering::Relaxed);
    if let Ok(mut a) = anim().lock() {
        let entry = a.announced.entry(m.entity).or_default();
        let at = Some((now, m.ticks));
        match m.op {
            Op::Attack => entry.attack = at,
            Op::Emerge => entry.emerge = at,
            Op::Rage => entry.rage = at,
        }
    }
}

/// A sound the zombie template plays at one of our mobs is swapped for the
/// mob's own (upstream's `playAmbientSound`, `playHurtSound`, `die` and
/// `playStepSound`).
pub fn sound_play(event: &mut SoundPlayEvent<'_>) -> Flow {
    let sound = event.sound;
    if !sound.starting || !sound.identifier.starts_with("minecraft:entity.zombie.") {
        return Flow::Continue;
    }
    let Ok(ours) = positions().lock() else {
        return Flow::Continue;
    };
    let at = sound.position;
    let Some(kind) = ours
        .iter()
        .filter(|(_, p)| {
            (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2) + (p[2] - at[2]).powi(2) < 1.0
        })
        .map(|(k, _)| *k)
        .next()
    else {
        return Flow::Continue;
    };
    let Some((replacement, volume, pitch)) = kind.sound(sound.identifier) else {
        return Flow::Continue;
    };
    if !replacement.is_empty()
        && let Ok(mut q) = queued().lock()
    {
        q.push((replacement, at, volume, pitch));
    }
    Flow::Cancel
}

/// The clip a mob shows now and how far into it (seconds), as upstream's
/// animation tables choose: dying, climbing, raging, swinging, running,
/// walking, standing.
fn choose(kind: Kind, id: u32, mob: &horizon_client::mod_api::NativeMobRender<'_>, a: &Announced) -> (ClipId, f32) {
    let geo = kind.geo();
    let now = TICKS.load(Ordering::Relaxed) as f32 + mob.partial;
    let running = |at: Option<(u32, u16)>| {
        at.and_then(|(start, ticks)| {
            let elapsed = now - start as f32;
            (elapsed >= 0.0 && elapsed < f32::from(ticks)).then_some(elapsed / 20.0)
        })
    };
    if mob.death_time > 0 {
        // Horizon removes a dead mob after 20 ticks, as vanilla does; upstream
        // keeps the body up to 49. The death clip is played fast enough to
        // finish lying down by tick 18.
        let clip = geo.clip(ClipId::Death).map(|c| c.length).unwrap_or(1.0);
        let t = ((mob.death_time as f32 + mob.partial) / 18.0).min(1.0) * clip;
        return (ClipId::Death, t);
    }
    if let Some(t) = running(a.emerge) {
        // The message carries the ticks left; the clip is 92 ticks long.
        let skipped = crate::mobs::GHOUL_EMERGE_TICKS as f32
            - f32::from(a.emerge.map(|e| e.1).unwrap_or(0));
        return (ClipId::Spawn, t + skipped / 20.0);
    }
    if let Some(t) = running(a.rage) {
        return (ClipId::Rage, t);
    }
    if let Some(t) = running(a.attack) {
        return (ClipId::Attack, t);
    }
    let age = mob.age / 20.0;
    // AnimationTable.isMoving: a limb swing beyond 0.15.
    let moving = mob.walk_speed > 0.15;
    if moving && mob.walk_speed > GHOUL_RUN_SPEED && geo.clip(ClipId::Run).is_some() {
        // The ghoul runs on two legs or on all fours, a coin upstream tosses
        // per chase; here a coin per ghoul.
        if geo.clip(ClipId::Crawl).is_some() && id % 2 == 1 {
            return (ClipId::Crawl, age);
        }
        return (ClipId::Run, age);
    }
    if moving {
        return (ClipId::Walk, age);
    }
    (ClipId::Idle, age)
}

/// The frame's pose for mob `id`: its clip, blended from the last one over
/// `TRANSITION_TICKS` when it changed.
fn pose_for(kind: Kind, id: u32, mob: &horizon_client::mod_api::NativeMobRender<'_>) -> Pose {
    let geo = kind.geo();
    let bones = geo.bones.len();
    let Ok(mut a) = anim().lock() else {
        return Pose::rest(bones);
    };
    let announced = a.announced.get(&id).copied().unwrap_or_default();
    let (clip, t) = choose(kind, id, mob, &announced);
    let mut pose = Pose::rest(bones);
    if let Some(c) = geo.clip(clip) {
        geo::sample(c, t, &mut pose);
    }
    let now = TICKS.load(Ordering::Relaxed);
    let shown = a.shown.entry(id).or_insert_with(|| Shown {
        clip,
        since: mob.age,
        from: None,
        last: pose.clone(),
        seen: now,
    });
    if shown.clip != clip {
        shown.from = Some(shown.last.clone());
        shown.clip = clip;
        shown.since = mob.age;
    }
    // The ghoul's climb starts at once (upstream's zero transition into it).
    let blend_ticks = if clip == ClipId::Spawn { 0.0 } else { TRANSITION_TICKS as f32 };
    let progress = if blend_ticks > 0.0 { ((mob.age - shown.since) / blend_ticks).clamp(0.0, 1.0) } else { 1.0 };
    let mut out = match (&shown.from, progress < 1.0) {
        (Some(from), true) => {
            let mut p = from.clone();
            p.blend(&pose, progress);
            p
        }
        _ => {
            shown.from = None;
            pose
        }
    };
    if out.rot.len() != bones {
        out = Pose::rest(bones);
    }
    shown.last = out.clone();
    shown.seen = now;
    out
}

/// Draws a ghoul, revenant or nightmare with its own model; `Flow::Cancel`
/// leaves the zombie template's model out. Any other mob is left alone, and
/// so is ours if its texture is missing (the zombie is drawn instead of an
/// invisible mob).
pub fn mob_submit(host: &dyn Host, event: &mut MobSubmitEvent<'_>) -> Flow {
    let mob = event.mob;
    let Some(kind) = Kind::from_id(mob.kind) else {
        return Flow::Continue;
    };
    let geo = kind.geo();
    let pose = pose_for(kind, mob.id, mob);
    // GhoulEntity.getVariant: one of eight skins from its UUID; the client
    // has no UUID here, so from its network id.
    let variant = (mob.id % 8) as u8 + 1;
    let (texture, eyes) = kind.textures(variant);

    let Ok(mut models) = models().lock() else {
        return Flow::Continue;
    };
    let model = models.entry(kind).or_insert_with(|| geo.build());
    model.reset_all_poses();
    geo::apply(geo, model, &pose);

    let draw: &mut BlockEntityDraw = event.draw;
    draw.push();
    // LivingEntityRenderer's frame: from the feet to the model's own (+y
    // down, front towards -z).
    draw.scale([1.0, -1.0, -1.0]);
    draw.translate([0.0, -1.501, 0.0]);
    // The hurt flash, as vanilla's red overlay.
    let tint = if mob.hurt_time > 0 { 0xFF9090 } else { 0xFFFFFF };
    let drawn = host.draw_model_part(draw, model, &texture, tint);
    if drawn && let Some(eyes) = eyes {
        // GraveyardEyesLayer: the eyes texture again, full bright, untinted,
        // a hair larger so it is not hidden by the faces under it.
        let light = draw.light;
        draw.light = FULL_BRIGHT;
        draw.push();
        draw.translate([0.0, 1.501, 0.0]);
        draw.scale([1.002, 1.002, 1.002]);
        draw.translate([0.0, -1.501, 0.0]);
        let _ = host.draw_model_part(draw, model, &eyes, 0xFFFFFF);
        draw.pop();
        draw.light = light;
    }
    draw.pop();
    if drawn { Flow::Cancel } else { Flow::Continue }
}

/// A world closing forgets every mob's animation state.
pub fn clear() {
    if let Ok(mut a) = anim().lock() {
        *a = Anim::default();
    }
    if let Ok(mut p) = positions().lock() {
        p.clear();
    }
}
