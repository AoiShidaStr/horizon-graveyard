//! The one channel from the server half to the client half: which animation
//! a mob starts. Upstream syncs these as entity data (attack and rage timers,
//! the ghoul's spawn timer); Horizon's mob render state has no such fields,
//! so the server tells the clients near the mob instead.
//!
//! A message is `[op][entity: u32 LE][ticks: u16 LE]`, 7 bytes.
//!
//! Part of the Horizon port of The Graveyard (GPL-3.0-only); original code.
pub const CHANNEL: &str = "graveyard:anim";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// A melee swing started.
    Attack,
    /// A ghoul started climbing out of the ground; `ticks` is what is left.
    Emerge,
    /// A ghoul started its rage roar.
    Rage,
}

impl Op {
    fn byte(self) -> u8 {
        match self {
            Op::Attack => 1,
            Op::Emerge => 2,
            Op::Rage => 3,
        }
    }

    fn from_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Op::Attack),
            2 => Some(Op::Emerge),
            3 => Some(Op::Rage),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub op: Op,
    pub entity: u32,
    pub ticks: u16,
}

impl Message {
    pub fn encode(&self) -> [u8; 7] {
        let mut out = [0u8; 7];
        out[0] = self.op.byte();
        out[1..5].copy_from_slice(&self.entity.to_le_bytes());
        out[5..7].copy_from_slice(&self.ticks.to_le_bytes());
        out
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != 7 {
            return None;
        }
        Some(Self {
            op: Op::from_byte(data[0])?,
            entity: u32::from_le_bytes(data[1..5].try_into().ok()?),
            ticks: u16::from_le_bytes(data[5..7].try_into().ok()?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for op in [Op::Attack, Op::Emerge, Op::Rage] {
            let m = Message { op, entity: 0xDEAD_BEEF, ticks: 92 };
            assert_eq!(Message::decode(&m.encode()), Some(m));
        }
    }

    #[test]
    fn junk_is_refused() {
        assert_eq!(Message::decode(&[]), None);
        assert_eq!(Message::decode(&[9, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(Message::decode(&[1, 0, 0, 0, 0, 0, 0, 0]), None);
    }
}
