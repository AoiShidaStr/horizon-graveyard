//! The three mobs of this slice and the numbers both halves share, from
//! upstream's entity classes (GhoulEntity, RevenantEntity, NightmareEntity,
//! their melee goals and TGEntities).
//!
//! Part of the Horizon port of The Graveyard (GPL-3.0-only).
use crate::geo::Geo;

/// `AnimationTable.TRANSITION_TICKS`: the blend between two clips, and the
/// slack every attack timer adds to its clip.
pub const TRANSITION_TICKS: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Ghoul,
    Revenant,
    Nightmare,
}

impl Kind {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "graveyard:ghoul" => Some(Kind::Ghoul),
            "graveyard:revenant" => Some(Kind::Revenant),
            "graveyard:nightmare" => Some(Kind::Nightmare),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Ghoul => "ghoul",
            Kind::Revenant => "revenant",
            Kind::Nightmare => "nightmare",
        }
    }

    pub fn geo(self) -> &'static Geo {
        match self {
            Kind::Ghoul => &crate::geo_data::GHOUL,
            Kind::Revenant => &crate::geo_data::REVENANT,
            Kind::Nightmare => &crate::geo_data::NIGHTMARE,
        }
    }

    /// Upstream's box (`EntityType.Builder.sized`, `eyeHeight`): width,
    /// height, eye height. The zombie template's default eye height is 85% of
    /// the height, as vanilla's.
    pub fn size(self) -> (f64, f64, f64) {
        match self {
            Kind::Ghoul => (0.8, 2.1, 2.1 * 0.85),
            Kind::Revenant => (0.6, 1.9, 1.9 * 0.85),
            Kind::Nightmare => (0.6, 2.6, 2.55),
        }
    }

    /// `AbstractGraveyardMeleeGoal.getSquaredMaxAttackDistance`:
    /// `(width * 2)^2 + target width`, for a player-wide (0.6) target.
    pub fn reach_sqr(self) -> f64 {
        let w = self.size().0;
        (w * 2.0) * (w * 2.0) + 0.6
    }

    /// The attack clip's length in ticks (`ATTACK_ANIMATION_DURATION`).
    pub fn attack_duration(self) -> i32 {
        match self {
            Kind::Ghoul => 22,
            Kind::Revenant | Kind::Nightmare => 10,
        }
    }

    /// `ATTACK_ANIM_SYNC_TICKS`: the attack timer a swing sets.
    pub fn attack_sync(self) -> i32 {
        self.attack_duration() + TRANSITION_TICKS
    }

    /// `DAMAGE_START_IN_ANIM`: the blow lands when the goal's animation ticker
    /// (set to the clip's length when the swing starts) reaches this.
    pub fn damage_at(self) -> i32 {
        match self {
            Kind::Ghoul => 10,
            Kind::Revenant => 1,
            Kind::Nightmare => 0,
        }
    }

    /// Upstream's `WaterAvoidingRandomStrollGoal` speed.
    pub fn stroll_speed(self) -> f64 {
        match self {
            Kind::Ghoul => 0.8,
            Kind::Revenant | Kind::Nightmare => 1.0,
        }
    }

    /// The base movement speed (createXAttributes), restored after a rage.
    pub fn speed(self) -> f64 {
        match self {
            Kind::Ghoul => 0.175,
            Kind::Revenant => 0.155,
            Kind::Nightmare => 0.19,
        }
    }

    /// The textures of the model and of its glowing eyes, by `variant` for
    /// the ghoul's nine skins.
    pub fn textures(self, variant: u8) -> (String, Option<String>) {
        match self {
            Kind::Ghoul => (
                format!("graveyard:entity/ghoul_skin{variant}"),
                Some(format!("graveyard:entity/ghoul_eyes_skin{variant}")),
            ),
            Kind::Revenant => (
                "graveyard:entity/revenant".to_owned(),
                Some("graveyard:entity/revenant_eyes".to_owned()),
            ),
            Kind::Nightmare => ("graveyard:entity/nightmare".to_owned(), None),
        }
    }

    /// The sound events that stand in for the zombie template's, with the
    /// pitch upstream plays them at (a pitch under 0.5 is played at 0.5, as
    /// vanilla clamps it).
    pub fn sound(self, zombie_event: &str) -> Option<(&'static str, f32, f32)> {
        let event = zombie_event.strip_prefix("minecraft:entity.zombie.")?;
        match (self, event) {
            (Kind::Ghoul, "ambient") => Some(("graveyard:entity.ghoul.ambient", 1.0, 0.5)),
            (Kind::Ghoul, "hurt") => Some(("graveyard:entity.ghoul.hurt", 1.0, 0.5)),
            (Kind::Ghoul, "death") => Some(("graveyard:entity.ghoul.death", 1.0, 0.5)),
            (Kind::Ghoul, "step") => Some(("graveyard:entity.ghoul.step", 0.5, 0.5)),
            (Kind::Revenant, "ambient") => Some(("graveyard:entity.revenant.ambient", 1.0, 1.0)),
            (Kind::Revenant, "hurt") => Some(("graveyard:entity.revenant.hurt", 1.0, 1.0)),
            (Kind::Revenant, "death") => Some(("graveyard:entity.revenant.death", 1.0, 1.0)),
            (Kind::Revenant, "step") => Some(("graveyard:entity.revenant.step", 0.15, 1.0)),
            (Kind::Nightmare, "ambient") => Some(("graveyard:entity.nightmare.ambient", 1.0, 0.5)),
            (Kind::Nightmare, "hurt") => Some(("graveyard:entity.nightmare.hurt", 1.0, 0.5)),
            (Kind::Nightmare, "death") => Some(("graveyard:entity.nightmare.death", 1.0, 0.5)),
            // The nightmare walks silently upstream (no step sound of its own).
            (Kind::Nightmare, "step") => Some(("", 0.0, 0.0)),
            _ => None,
        }
    }
}

/// Ghoul: `GhoulEntity.SPAWN_ANIMATION_DURATION`, the climb out of the ground.
pub const GHOUL_EMERGE_TICKS: i32 = 92;
/// Ghoul: `RAGE_ANIMATION_COOLDOWN`, the countdown a first swing starts.
pub const GHOUL_RAGE_COOLDOWN: i32 = 300;
/// Ghoul: `RAGE_ANIMATION_DURATION`, the roar at the end of that countdown.
pub const GHOUL_RAGE_TICKS: i32 = 64 + TRANSITION_TICKS;
/// Ghoul: the walk speed (`walkAnimation.speed`) over which it runs.
pub const GHOUL_RUN_SPEED: f32 = 0.55;
/// The scoreboard tag a ghoul gets once it has climbed out, so a ghoul read
/// back from a save does not climb out again.
pub const EMERGED_TAG: &str = "graveyard.emerged";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_timings() {
        assert_eq!(Kind::Ghoul.attack_sync(), 27);
        assert_eq!(Kind::Revenant.attack_sync(), 15);
        // Ghoul 0.8 wide: 1.6^2 + 0.6.
        assert!((Kind::Ghoul.reach_sqr() - 3.16).abs() < 1.0e-9);
        assert_eq!(Kind::from_id("graveyard:nightmare"), Some(Kind::Nightmare));
        assert_eq!(Kind::from_id("minecraft:zombie"), None);
    }

    #[test]
    fn sounds_replace_only_the_zombies() {
        assert_eq!(
            Kind::Ghoul.sound("minecraft:entity.zombie.hurt").map(|s| s.0),
            Some("graveyard:entity.ghoul.hurt")
        );
        assert_eq!(Kind::Ghoul.sound("minecraft:entity.player.hurt"), None);
    }
}
