//! The Graveyard for Horizon: the native (trait mod) half.
//!
//! A Horizon port of The Graveyard by finallion (GPL-3.0-only, see LICENSE
//! and NOTICE.md in the mod folder): the Ghoul, the Revenant and the
//! Nightmare with upstream's models, animations, goals and timings. The
//! content half (`content/`) declares their entity types, the blocks and the
//! structures.
//!
//! The launcher links this crate into the game and calls `hooks()`
//! (docs/native-mods.md, "Trait mods"). The same `Hooks` serve a dedicated
//! server (only the server hooks fire there) and a client (the drawing and
//! sound hooks).
pub mod geo;
#[rustfmt::skip]
pub mod geo_data;
pub mod mobs;
pub mod net;

mod client;
mod server;

use horizon_client::mod_api::{
    ClientTickPostEvent, EntityLeaveLevelEvent, EntityTickListenedEvent, EntityTickPostEvent, Flow, Hooks, Host,
    LivingConversionPreEvent, LivingDeathEvent, LivingIncomingDamageEvent, MobSubmitEvent,
    NetFromServerEvent, RegisterGoalsEvent, SoundPlayEvent, WorldLifecycleEvent, WorldPhase,
};

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Mod)
}

struct Mod;

impl Hooks for Mod {
    // Both sides.
    fn register_goals(&self, event: &mut RegisterGoalsEvent<'_>) {
        server::register_goals(event);
    }

    fn world_lifecycle(&self, _host: &mut dyn Host, event: &mut WorldLifecycleEvent<'_>) {
        if matches!(event.phase, WorldPhase::WorldOpenReady | WorldPhase::WorldCloseFinal) {
            server::clear();
            client::clear();
        }
    }

    // Server side.
    fn entity_tick_listened(&self, host: &mut dyn Host, event: &mut EntityTickListenedEvent) {
        server::entity_tick_listened(host, event);
    }

    fn entity_tick_post(&self, host: &mut dyn Host, event: &mut EntityTickPostEvent<'_>) {
        server::entity_tick_post(host, event);
    }

    fn living_conversion_pre(&self, _host: &mut dyn Host, event: &mut LivingConversionPreEvent<'_>) -> Flow {
        server::living_conversion_pre(event)
    }

    fn living_incoming_damage(&self, event: &mut LivingIncomingDamageEvent<'_>) -> Flow {
        server::living_incoming_damage(event)
    }

    fn living_death(&self, _host: &mut dyn Host, event: &mut LivingDeathEvent<'_>) -> Flow {
        server::living_death(event)
    }

    fn entity_leave_level(&self, _host: &mut dyn Host, event: &mut EntityLeaveLevelEvent<'_>) {
        server::entity_leave_level(event.entity, event.kind);
    }

    // Client side.
    fn mob_submit(&self, host: &dyn Host, event: &mut MobSubmitEvent<'_>) -> Flow {
        client::mob_submit(host, event)
    }

    fn net_from_server(&self, _host: &mut dyn Host, event: &mut NetFromServerEvent<'_>) {
        client::net_from_server(event);
    }

    fn client_tick_post(&self, host: &mut dyn Host, _event: &mut ClientTickPostEvent) {
        client::tick(host);
    }

    fn sound_play(&self, event: &mut SoundPlayEvent<'_>) -> Flow {
        client::sound_play(event)
    }
}
