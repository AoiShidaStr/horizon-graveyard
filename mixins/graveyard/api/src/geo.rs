//! The mobs' models and animations: upstream's GeckoLib files, translated into
//! the tables of `geo_data.rs` by `tools/geckolib2rs.py`, built here into a
//! vanilla `ModelPart` tree and posed from the keyframes.
//!
//! The frames are worked out in the tool's header: a part's place and turn are
//! the bone's, and an animated rotation adds to the bone's own in radians with
//! the file's signs; an animated position moves the part by (x, -y, z) texels.
//!
//! Part of the Horizon port of The Graveyard (GPL-3.0-only); original code.
use horizon_client::mod_api::{ModelPart, ModelPolygon, ModelVertex, PartPose};

/// One face of a cube, baked: its corners in its bone's frame (texels) and
/// where they fall on the texture (0 to 1).
pub struct Quad {
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
}

/// A bone: a part of the tree.
pub struct Bone {
    pub name: &'static str,
    /// Index of its parent in the model's bones, -1 for a root.
    pub parent: i32,
    /// Its pivot in its parent's frame (texels).
    pub offset: [f32; 3],
    /// Its rest rotation, radians, applied z, then y, then x.
    pub rot: [f32; 3],
    pub quads: &'static [Quad],
}

/// A keyframe of one channel, in seconds; `pre` is the value arriving at it,
/// `post` the one leaving it (Bedrock's pre/post keyframes).
pub struct Key {
    pub t: f32,
    pub pre: [f32; 3],
    pub post: [f32; 3],
    pub catmull: bool,
}

/// One bone's rotation (kind 0, degrees), position (1, texels) or scale (2).
pub struct Channel {
    pub bone: u16,
    pub kind: u8,
    pub keys: &'static [Key],
}

pub struct Clip {
    /// Seconds.
    pub length: f32,
    pub looping: bool,
    pub channels: &'static [Channel],
}

/// The clips a mob may have, in `Geo::clips` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipId {
    Idle = 0,
    Walk = 1,
    Run = 2,
    Crawl = 3,
    Attack = 4,
    Death = 5,
    Spawn = 6,
    Rage = 7,
}

pub struct Geo {
    /// The texture's size in texels.
    pub texture: [u32; 2],
    pub bones: &'static [Bone],
    pub clips: [Option<&'static Clip>; 8],
}

impl Geo {
    pub fn clip(&self, id: ClipId) -> Option<&'static Clip> {
        self.clips[id as usize]
    }

    /// The bone index of `name`.
    pub fn bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    /// The model as a `ModelPart` tree: an empty root holding the root bones,
    /// each part at its bone's pivot and rest rotation.
    pub fn build(&self) -> ModelPart {
        let roots = (0..self.bones.len())
            .filter(|&i| self.bones[i].parent < 0)
            .map(|i| (self.bones[i].name.to_owned(), self.part(i)))
            .collect();
        ModelPart::new(Vec::new(), roots, PartPose::ZERO)
    }

    fn part(&self, index: usize) -> ModelPart {
        let bone = &self.bones[index];
        let polygons = bone
            .quads
            .iter()
            .map(|q| ModelPolygon {
                vertices: std::array::from_fn(|k| ModelVertex {
                    pos: q.pos[k],
                    u: q.uv[k][0],
                    v: q.uv[k][1],
                }),
                normal: normal(&q.pos),
            })
            .collect();
        let children = (0..self.bones.len())
            .filter(|&i| self.bones[i].parent == index as i32)
            .map(|i| (self.bones[i].name.to_owned(), self.part(i)))
            .collect();
        let [x, y, z] = bone.offset;
        let [xr, yr, zr] = bone.rot;
        ModelPart::new(polygons, children, PartPose::offset_and_rotation(x, y, z, xr, yr, zr))
    }
}

fn normal(p: &[[f32; 3]; 4]) -> [f32; 3] {
    let a = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
    let b = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1.0e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [n[0] / len, n[1] / len, n[2] / len]
    }
}

/// Each bone's animated offsets for one frame: rotation (degrees), position
/// (texels) and scale.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub rot: Vec<[f32; 3]>,
    pub pos: Vec<[f32; 3]>,
    pub scale: Vec<[f32; 3]>,
}

impl Pose {
    pub fn rest(bones: usize) -> Self {
        Self {
            rot: vec![[0.0; 3]; bones],
            pos: vec![[0.0; 3]; bones],
            scale: vec![[1.0; 3]; bones],
        }
    }

    /// `self` moved towards `other` by `t` (0 keeps `self`).
    pub fn blend(&mut self, other: &Pose, t: f32) {
        let mix = |a: &mut [[f32; 3]], b: &[[f32; 3]]| {
            for (x, y) in a.iter_mut().zip(b) {
                for k in 0..3 {
                    x[k] += (y[k] - x[k]) * t;
                }
            }
        };
        mix(&mut self.rot, &other.rot);
        mix(&mut self.pos, &other.pos);
        mix(&mut self.scale, &other.scale);
    }
}

/// The clip's local time for `seconds` since it started: wrapped for a
/// looping clip, held at the end otherwise.
pub fn clip_time(clip: &Clip, seconds: f32) -> f32 {
    if clip.length <= 0.0 {
        return 0.0;
    }
    if clip.looping {
        seconds.rem_euclid(clip.length)
    } else {
        seconds.clamp(0.0, clip.length)
    }
}

/// `clip` at `seconds` since it started, into `pose` (bones the clip leaves
/// alone keep what `pose` had).
pub fn sample(clip: &Clip, seconds: f32, pose: &mut Pose) {
    let t = clip_time(clip, seconds);
    for channel in clip.channels {
        let bone = channel.bone as usize;
        if bone >= pose.rot.len() {
            continue;
        }
        let value = eval(channel.keys, t);
        match channel.kind {
            0 => pose.rot[bone] = value,
            1 => pose.pos[bone] = value,
            _ => pose.scale[bone] = value,
        }
    }
}

/// A channel's value at `t`: held before the first and after the last key,
/// linear between keys, or Catmull-Rom when either end asks for it.
pub fn eval(keys: &[Key], t: f32) -> [f32; 3] {
    let Some(first) = keys.first() else {
        return [0.0; 3];
    };
    if keys.len() == 1 || t <= first.t {
        return first.pre;
    }
    let last = &keys[keys.len() - 1];
    if t >= last.t {
        return last.post;
    }
    let i = keys.windows(2).position(|w| t >= w[0].t && t < w[1].t).unwrap_or(0);
    let (a, b) = (&keys[i], &keys[i + 1]);
    let span = (b.t - a.t).max(1.0e-6);
    let s = (t - a.t) / span;
    if a.catmull || b.catmull {
        let p0 = if i > 0 { keys[i - 1].post } else { a.post };
        let p3 = keys.get(i + 2).map(|k| k.pre).unwrap_or(b.pre);
        std::array::from_fn(|k| catmull(p0[k], a.post[k], b.pre[k], p3[k], s))
    } else {
        std::array::from_fn(|k| a.post[k] + (b.pre[k] - a.post[k]) * s)
    }
}

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * p1
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

/// Sets the tree's parts from `pose` over their rest poses (call
/// `reset_all_poses` first).
pub fn apply(geo: &Geo, model: &mut ModelPart, pose: &Pose) {
    for (name, part) in model.children_mut() {
        apply_part(geo, name, part, pose);
    }
}

fn apply_part(geo: &Geo, name: &str, part: &mut ModelPart, pose: &Pose) {
    if let Some(i) = geo.bone(name) {
        let r = pose.rot[i];
        let p = pose.pos[i];
        let s = pose.scale[i];
        let rad = std::f32::consts::PI / 180.0;
        part.x_rot += r[0] * rad;
        part.y_rot += r[1] * rad;
        part.z_rot += r[2] * rad;
        part.x += p[0];
        part.y -= p[1];
        part.z += p[2];
        part.x_scale *= s[0];
        part.y_scale *= s[1];
        part.z_scale *= s[2];
    }
    for (child_name, child) in part.children_mut() {
        apply_part(geo, child_name, child, pose);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &[Key] = &[
        Key { t: 0.0, pre: [0.0; 3], post: [0.0; 3], catmull: false },
        Key { t: 1.0, pre: [10.0, 0.0, -10.0], post: [20.0, 0.0, 0.0], catmull: false },
        Key { t: 2.0, pre: [0.0; 3], post: [0.0; 3], catmull: false },
    ];

    #[test]
    fn keys_are_held_outside_and_lerped_between() {
        assert_eq!(eval(K, -1.0), [0.0; 3]);
        assert_eq!(eval(K, 0.5), [5.0, 0.0, -5.0]);
        // Leaving a key uses its `post` value.
        assert_eq!(eval(K, 1.5), [10.0, 0.0, 0.0]);
        assert_eq!(eval(K, 3.0), [0.0; 3]);
    }

    #[test]
    fn catmull_passes_through_its_keys() {
        const C: &[Key] = &[
            Key { t: 0.0, pre: [0.0; 3], post: [0.0; 3], catmull: true },
            Key { t: 1.0, pre: [4.0; 3], post: [4.0; 3], catmull: true },
            Key { t: 2.0, pre: [0.0; 3], post: [0.0; 3], catmull: true },
        ];
        assert!((eval(C, 0.999)[0] - 4.0).abs() < 0.05);
        assert!(eval(C, 0.5)[0] > 1.5 && eval(C, 0.5)[0] < 3.5);
    }

    #[test]
    fn clips_loop_or_hold() {
        let looping = Clip { length: 2.0, looping: true, channels: &[] };
        let once = Clip { length: 2.0, looping: false, channels: &[] };
        assert_eq!(clip_time(&looping, 5.0), 1.0);
        assert_eq!(clip_time(&once, 5.0), 2.0);
    }

    #[test]
    fn every_model_builds_with_its_bones() {
        for (geo, root_bones) in [
            (&crate::geo_data::GHOUL, 1),
            (&crate::geo_data::REVENANT, 1),
            (&crate::geo_data::NIGHTMARE, 1),
        ] {
            let model = geo.build();
            assert_eq!(model.children().count(), root_bones);
            let mut pose = Pose::rest(geo.bones.len());
            for id in [ClipId::Idle, ClipId::Walk, ClipId::Attack, ClipId::Death] {
                let clip = geo.clip(id).expect("clip");
                sample(clip, 0.3, &mut pose);
                assert!(pose.rot.iter().flatten().all(|v| v.is_finite()));
            }
            let mut posed = model.clone();
            apply(geo, &mut posed, &pose);
            assert_ne!(posed, model);
        }
    }
}
