// VoxelMotion Studio - Core Animation Data Model, Easing Engine & Keyframe Optimizer

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::{Index, IndexMut};
use std::sync::Arc;

pub const BONE_COUNT: usize = 10;

pub const BONE_NAMES: [&str; BONE_COUNT] = [
    "waist",
    "body",
    "head",
    "right_arm",
    "left_arm",
    "right_item",
    "right_leg",
    "left_leg",
    "root",
    "left_item",
];

pub const BONE_LABELS_ES: [&str; BONE_COUNT] = [
    "Cintura",
    "Cuerpo",
    "Cabeza",
    "BrazoDerecho",
    "BrazoIzquierdo",
    "Espada",
    "PiernaDerecha",
    "PiernaIzquierda",
    "Raíz",
    "ArmaIzq",
];

pub const DISPLAY_BONE_ORDER: [usize; BONE_COUNT] = [8, 0, 1, 2, 3, 4, 5, 9, 6, 7];

/// Map a canonical bone identifier to its fixed rig array index in O(1) without heap lookups.
#[inline]
pub fn bone_index_by_name(name: &str) -> Option<usize> {
    match name {
        "waist" => Some(0),
        "body" => Some(1),
        "head" => Some(2),
        "right_arm" => Some(3),
        "left_arm" => Some(4),
        "right_item" => Some(5),
        "right_leg" => Some(6),
        "left_leg" => Some(7),
        "root" => Some(8),
        "left_item" => Some(9),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WeaponCategory {
    Todas,
    Espadas,
    Katanas,
    Mandobles,
    Hachas,
    Lanzas,
    Dagas,
    Punos,
    Escudos,
    Arcos,
    Fuego,
    #[serde(other)]
    Otros,
}

impl WeaponCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Todas => "Todas",
            Self::Espadas => "Espadas",
            Self::Katanas => "Katanas",
            Self::Mandobles => "Mandobles",
            Self::Hachas => "Hachas",
            Self::Lanzas => "Lanzas",
            Self::Dagas => "Dagas",
            Self::Punos => "Puños",
            Self::Escudos => "Escudos",
            Self::Arcos => "Arcos",
            Self::Fuego => "Fuego",
            Self::Otros => "Otros",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Todas => "Todas",
            Self::Espadas => "Espadas",
            Self::Katanas => "Katanas",
            Self::Mandobles => "Mand.",
            Self::Hachas => "Hachas",
            Self::Lanzas => "Lanzas",
            Self::Dagas => "Dagas",
            Self::Punos => "Puños",
            Self::Escudos => "Escudos",
            Self::Arcos => "Arcos",
            Self::Fuego => "Fuego",
            Self::Otros => "Otros",
        }
    }

    pub fn export_key(self) -> &'static str {
        match self {
            Self::Todas => "todas",
            Self::Espadas => "espadas",
            Self::Katanas => "katanas",
            Self::Mandobles => "mandobles",
            Self::Hachas => "hachas",
            Self::Lanzas => "lanzas",
            Self::Dagas => "dagas",
            Self::Punos => "punos",
            Self::Escudos => "escudos",
            Self::Arcos => "arcos",
            Self::Fuego => "fuego",
            Self::Otros => "otros",
        }
    }

    pub const ALL_FAMILIES: [Self; 10] = [
        Self::Espadas,
        Self::Katanas,
        Self::Mandobles,
        Self::Hachas,
        Self::Lanzas,
        Self::Dagas,
        Self::Punos,
        Self::Escudos,
        Self::Arcos,
        Self::Fuego,
    ];

    pub const ALL_SIDEBAR: [Self; 11] = [
        Self::Todas,
        Self::Espadas,
        Self::Katanas,
        Self::Mandobles,
        Self::Hachas,
        Self::Lanzas,
        Self::Dagas,
        Self::Punos,
        Self::Escudos,
        Self::Arcos,
        Self::Fuego,
    ];

    pub const MELEE_AND_SHIELD_FAMILIES: [Self; 8] = [
        Self::Espadas,
        Self::Katanas,
        Self::Mandobles,
        Self::Hachas,
        Self::Lanzas,
        Self::Dagas,
        Self::Punos,
        Self::Escudos,
    ];

    pub fn from_family_index(i: i32) -> Option<Self> {
        if i < 0 {
            return None;
        }
        Self::ALL_FAMILIES.get(i as usize).copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EasingMode {
    Snap,
    EaseOut,
    EaseInOut,
    Linear,
    Step,
}

impl EasingMode {
    pub fn index(self) -> i32 {
        match self {
            Self::Snap => 0,
            Self::EaseOut => 1,
            Self::EaseInOut => 2,
            Self::Linear => 3,
            Self::Step => 4,
        }
    }
    pub fn from_index(i: i32) -> Self {
        match i {
            0 => Self::Snap,
            1 => Self::EaseOut,
            2 => Self::EaseInOut,
            4 => Self::Step,
            _ => Self::Linear,
        }
    }
    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Snap => {
                if t < 0.35 {
                    1.8 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
                }
            }
            Self::EaseOut => 1.0 - (1.0 - t).powi(3),
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
                }
            }
            Self::Linear => t,
            Self::Step => {
                if t >= 0.999 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct BoneTransform {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    #[serde(default = "default_scale")]
    pub scale: [f32; 3],
}

fn default_scale() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

impl BoneTransform {
    pub const IDENTITY: Self = Self {
        pos: [0.0, 0.0, 0.0],
        rot: [0.0, 0.0, 0.0],
        scale: [1.0, 1.0, 1.0],
    };

    #[inline]
    pub fn is_identity(&self) -> bool {
        self.pos == [0.0, 0.0, 0.0] && self.rot == [0.0, 0.0, 0.0] && self.scale == [1.0, 1.0, 1.0]
    }
}

impl Default for BoneTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Fixed-size, stack-allocated 8-bone pose matching [`BONE_NAMES`] order.
/// Eliminates per-frame `HashMap`/`String` heap allocations while preserving
/// backwards-compatible JSON map serialization for `project.json`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigPose(pub [BoneTransform; BONE_COUNT]);

impl Default for RigPose {
    fn default() -> Self {
        Self([BoneTransform::IDENTITY; BONE_COUNT])
    }
}

impl RigPose {
    pub const ZERO: Self = Self([BoneTransform::IDENTITY; BONE_COUNT]);

    #[inline]
    pub fn bone(&self, idx: usize) -> &BoneTransform {
        &self.0[idx.min(BONE_COUNT - 1)]
    }

    #[inline]
    pub fn bone_mut(&mut self, idx: usize) -> &mut BoneTransform {
        &mut self.0[idx.min(BONE_COUNT - 1)]
    }

    #[inline]
    pub fn get(&self, name: &str) -> Option<&BoneTransform> {
        bone_index_by_name(name).map(|i| &self.0[i])
    }

    #[inline]
    pub fn insert(&mut self, name: &str, tr: BoneTransform) {
        if let Some(i) = bone_index_by_name(name) {
            self.0[i] = tr;
        }
    }
}

impl Index<usize> for RigPose {
    type Output = BoneTransform;
    #[inline]
    fn index(&self, idx: usize) -> &Self::Output {
        &self.0[idx]
    }
}

impl IndexMut<usize> for RigPose {
    #[inline]
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        &mut self.0[idx]
    }
}

impl Index<&str> for RigPose {
    type Output = BoneTransform;
    #[inline]
    fn index(&self, name: &str) -> &Self::Output {
        let idx = bone_index_by_name(name).expect("unknown bone name");
        &self.0[idx]
    }
}

impl IndexMut<&str> for RigPose {
    #[inline]
    fn index_mut(&mut self, name: &str) -> &mut Self::Output {
        let idx = bone_index_by_name(name).expect("unknown bone name");
        &mut self.0[idx]
    }
}

impl Serialize for RigPose {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let count = self.0.iter().filter(|t| !t.is_identity()).count();
        let mut map = serializer.serialize_map(Some(count))?;
        for (i, tr) in self.0.iter().enumerate() {
            if !tr.is_identity() {
                map.serialize_entry(BONE_NAMES[i], tr)?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for RigPose {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PoseVisitor;

        impl<'de> Visitor<'de> for PoseVisitor {
            type Value = RigPose;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map of bone names or an array of 8 BoneTransforms")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut pose = RigPose::ZERO;
                while let Some((k, v)) = map.next_entry::<String, BoneTransform>()? {
                    if let Some(idx) = bone_index_by_name(&k) {
                        pose.0[idx] = v;
                    }
                }
                Ok(pose)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut pose = RigPose::ZERO;
                let mut i = 0usize;
                while let Some(v) = seq.next_element::<BoneTransform>()? {
                    if i < BONE_COUNT {
                        pose.0[i] = v;
                    }
                    i += 1;
                }
                Ok(pose)
            }
        }

        deserializer.deserialize_any(PoseVisitor)
    }
}

pub const ALL_BONES_MASK: u16 = (1u16 << BONE_COUNT) - 1;

fn default_bone_mask() -> u16 {
    ALL_BONES_MASK
}

fn deserialize_bone_mask<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    let raw = u16::deserialize(deserializer)?;
    // Upgrade legacy 8-bone full-body masks (0x00FF) seamlessly so imported keyframes cover root/left_item too.
    Ok(if raw == 0x00FF { ALL_BONES_MASK } else { raw & ALL_BONES_MASK })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: f32,
    pub easing: EasingMode,
    pub bones: RigPose,
    #[serde(default = "default_bone_mask", deserialize_with = "deserialize_bone_mask")]
    pub bone_mask: u16,
}

impl Keyframe {
    #[inline]
    pub fn has_bone(&self, bone_idx: usize) -> bool {
        (self.bone_mask & (1u16 << bone_idx.min(BONE_COUNT - 1))) != 0
    }

    #[inline]
    pub fn set_bone(&mut self, bone_idx: usize, present: bool) {
        let bit = 1u16 << bone_idx.min(BONE_COUNT - 1);
        if present {
            self.bone_mask |= bit;
        } else {
            self.bone_mask &= !bit;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GalleryGroup {
    Combate,
    Movimiento,
    Defensa,
    Especiales,
    Interaccion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CurationStatus {
    #[default]
    Pendiente,
    Aprobada,
    Mejorar,
    Descartada,
}

impl CurationStatus {
    pub fn index(self) -> i32 {
        match self {
            Self::Pendiente => 0,
            Self::Aprobada => 1,
            Self::Mejorar => 2,
            Self::Descartada => 3,
        }
    }

    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::Aprobada,
            2 => Self::Mejorar,
            3 => Self::Descartada,
            _ => Self::Pendiente,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pendiente => "Pendiente",
            Self::Aprobada => "Aprobada",
            Self::Mejorar => "Mejorar",
            Self::Descartada => "Descartada",
        }
    }

    pub fn short_tag(self) -> &'static str {
        match self {
            Self::Pendiente => "",
            Self::Aprobada => "OK",
            Self::Mejorar => "FIX",
            Self::Descartada => "NO",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClipSource {
    #[default]
    Personalizado,
    RellSeas,
    BetterCombat,
    SimplySwords,
    CombatRoll,
    FluidFoolish,
}

impl ClipSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Personalizado => "Custom",
            Self::RellSeas => "RELL Seas",
            Self::BetterCombat => "Better Combat",
            Self::SimplySwords => "Simply Swords",
            Self::CombatRoll => "Combat Roll",
            Self::FluidFoolish => "Fluid & Foolish",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Personalizado => "Custom",
            Self::RellSeas => "RELL",
            Self::BetterCombat => "BC",
            Self::SimplySwords => "SS",
            Self::CombatRoll => "Roll",
            Self::FluidFoolish => "F&F",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VfxKind {
    #[default]
    SlashTrail,
    ParrySpark,
    SonarRing,
    HitStop,
    ScreenShake,
}

impl VfxKind {
    pub fn from_index(i: i32) -> Self {
        match i {
            1 => Self::ParrySpark,
            2 => Self::SonarRing,
            3 => Self::HitStop,
            4 => Self::ScreenShake,
            _ => Self::SlashTrail,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SlashTrail => "Estela de Corte",
            Self::ParrySpark => "Chispas Parry",
            Self::SonarRing => "Onda Sonar",
            Self::HitStop => "Hit-Stop",
            Self::ScreenShake => "Screen Shake",
        }
    }

    pub fn export_key(self) -> &'static str {
        match self {
            Self::SlashTrail => "slash_trail",
            Self::ParrySpark => "parry_spark",
            Self::SonarRing => "sonar_ring",
            Self::HitStop => "hit_stop",
            Self::ScreenShake => "screen_shake",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VfxEvent {
    pub time: f32,
    pub kind: VfxKind,
    #[serde(default = "default_vfx_color")]
    pub color: [f32; 3],
    #[serde(default = "default_vfx_scale")]
    pub scale: f32,
    #[serde(default = "default_vfx_duration")]
    pub duration: f32,
    #[serde(default = "default_vfx_scale")]
    pub intensity: f32,
    #[serde(default)]
    pub texture_hint: String,
}

fn default_vfx_color() -> [f32; 3] {
    [0.58, 0.45, 1.0]
}
fn default_vfx_scale() -> f32 {
    1.0
}
fn default_vfx_duration() -> f32 {
    0.18
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClipDef {
    pub id: String,
    pub name: String,
    pub gallery_group: GalleryGroup,
    pub duration: f32,
    pub impact_time: Option<f32>,
    pub looped: bool,
    pub bc_targets: Vec<String>,
    #[serde(default, alias = "rell_event")]
    pub event_tag: Option<String>,
    #[serde(default)]
    pub source: ClipSource,
    #[serde(default)]
    pub status: CurationStatus,
    #[serde(default)]
    pub vfx_events: Vec<VfxEvent>,
    #[serde(default)]
    pub allowed_categories: Vec<WeaponCategory>,
    #[serde(default)]
    pub allowed_weapons: Vec<String>,
    pub keyframes: Vec<Keyframe>,
}

impl ClipDef {
    #[inline]
    pub fn is_universal(&self) -> bool {
        self.allowed_categories.is_empty() && self.allowed_weapons.is_empty()
    }

    #[inline]
    pub fn allows_weapon(&self, weapon: &WeaponRecord) -> bool {
        if self.is_universal() {
            return true;
        }
        if self
            .allowed_weapons
            .iter()
            .any(|id| id == &weapon.id || id == &weapon.item_id)
        {
            return true;
        }
        self.allowed_categories.contains(&weapon.category)
    }

    pub fn target_badge(&self) -> String {
        if self.is_universal() {
            return "Universal".into();
        }
        if self.allowed_categories.is_empty() && !self.allowed_weapons.is_empty() {
            if self.allowed_weapons.len() == 1 {
                let raw = self.allowed_weapons[0].as_str();
                let slug = raw.rsplit(':').next().unwrap_or(raw).replace('_', " ");
                let short: String = slug.chars().take(11).collect();
                return if slug.chars().count() > 11 {
                    format!("{short}…")
                } else {
                    short
                };
            }
            return format!("{} armas", self.allowed_weapons.len());
        }
        if self.allowed_categories.len() == WeaponCategory::MELEE_AND_SHIELD_FAMILIES.len()
            && WeaponCategory::MELEE_AND_SHIELD_FAMILIES
                .iter()
                .all(|c| self.allowed_categories.contains(c))
        {
            return "Melee/Esc.".into();
        }
        if self.allowed_categories.len() == 1 {
            let base = self.allowed_categories[0].short_label();
            return if self.allowed_weapons.is_empty() {
                base.into()
            } else {
                format!("{base}+")
            };
        }
        if self.allowed_categories.len() == 2 && self.allowed_weapons.is_empty() {
            return format!(
                "{}/{}",
                self.allowed_categories[0].short_label(),
                self.allowed_categories[1].short_label()
            );
        }
        let extra = (self.allowed_categories.len() - 1) + usize::from(!self.allowed_weapons.is_empty());
        format!("{}+{}", self.allowed_categories[0].short_label(), extra)
    }

    pub fn target_summary(&self, weapons: &[WeaponRecord]) -> String {
        if self.is_universal() {
            return "Universal (todos)".into();
        }
        let mut parts: Vec<String> = Vec::new();
        if self.allowed_categories.len() == WeaponCategory::MELEE_AND_SHIELD_FAMILIES.len()
            && WeaponCategory::MELEE_AND_SHIELD_FAMILIES
                .iter()
                .all(|c| self.allowed_categories.contains(c))
        {
            parts.push("Melee y Escudos".into());
        } else {
            for cat in &self.allowed_categories {
                parts.push(cat.label().into());
            }
        }
        if !self.allowed_weapons.is_empty() {
            if self.allowed_weapons.len() == 1 {
                let wid = &self.allowed_weapons[0];
                let wname = weapons
                    .iter()
                    .find(|w| &w.id == wid || &w.item_id == wid)
                    .map(|w| w.name.clone())
                    .unwrap_or_else(|| wid.rsplit(':').next().unwrap_or(wid).to_string());
                parts.push(wname);
            } else {
                parts.push(format!("{} armas específicas", self.allowed_weapons.len()));
            }
        }
        parts.join(", ")
    }
}

/// Immutable RGBA sprite pixel buffer shared via `Arc` between asset catalog and GPU voxelizer.
#[derive(Clone, Debug)]
pub struct SpritePixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl SpritePixels {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Arc<Self> {
        Arc::new(Self { width, height, rgba })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeaponRecord {
    pub id: String,
    pub name: String,
    pub mod_source: String,
    pub item_id: String,
    pub category: WeaponCategory,
    pub weapon_class: String, // MEDIUM | HEAVY | LIGHT | RANGED | UNARMED
    pub mesh_archetype: String,
    #[serde(default)]
    pub status: CurationStatus,
    #[serde(default)]
    pub bc_archetype: String,
    #[serde(skip)]
    pub icon_rgba: Option<Arc<SpritePixels>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimatorProject {
    pub version: u32,
    pub clips: Vec<ClipDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMode {
    Select,
    Rotate,
    Scale,
    Camera,
    Move,
}

/// Create a Blender-clean neutral resting clip ready for frame-by-frame keyframe authoring.
pub fn create_blank_clip(seq: usize) -> ClipDef {
    ClipDef {
        id: format!("action_{seq}"),
        name: if seq == 1 { "Nueva Acción".into() } else { format!("Nueva Acción {seq}") },
        gallery_group: GalleryGroup::Combate,
        duration: 0.60,
        impact_time: None,
        looped: false,
        bc_targets: Vec::new(),
        event_tag: None,
        source: ClipSource::Personalizado,
        status: CurationStatus::Pendiente,
        vfx_events: Vec::new(),
        allowed_categories: Vec::new(),
        allowed_weapons: Vec::new(),
        keyframes: vec![
            Keyframe { time: 0.0, easing: EasingMode::EaseInOut, bones: RigPose::ZERO, bone_mask: ALL_BONES_MASK },
            Keyframe { time: 0.60, easing: EasingMode::EaseInOut, bones: RigPose::ZERO, bone_mask: ALL_BONES_MASK },
        ],
    }
}

pub fn snap_time_to_fps(time: f32, duration: f32, fps: f32) -> f32 {
    let dur = duration.max(0.05);
    let frames = (dur * fps.max(1.0)).round().max(1.0);
    let f_idx = ((time / dur).clamp(0.0, 1.0) * frames).round().clamp(0.0, frames);
    (f_idx / frames) * dur
}

pub fn snap_time_to_frame(time: f32, duration: f32) -> f32 {
    snap_time_to_fps(time, duration, 60.0)
}

/// Align every keyframe and impact marker strictly onto exact `fps` integer frame lines,
/// resolving dense keyframe collisions cleanly without stacking or losing bone data.
pub fn snap_clip_to_fps(clip: &mut ClipDef, fps: f32) {
    let dur = clip.duration.max(0.05);
    clip.duration = dur;
    let frames = (dur * fps.max(1.0)).round().max(1.0) as i32;
    clip.keyframes.sort_by(|a, b| a.time.total_cmp(&b.time));
    let raw_frames: Vec<i32> = clip
        .keyframes
        .iter()
        .map(|k| ((k.time / dur).clamp(0.0, 1.0) * frames as f32).round() as i32)
        .collect();

    let mut snapped: Vec<(i32, Keyframe)> = Vec::with_capacity(clip.keyframes.len());
    for (idx, mut k) in clip.keyframes.drain(..).enumerate() {
        let f_raw = raw_frames[idx].clamp(0, frames);
        if let Some((prev_f, prev_k)) = snapped.last_mut() {
            if f_raw <= *prev_f {
                let next_f = raw_frames.get(idx + 1).copied();
                if *prev_f + 1 <= frames && next_f.map_or(true, |nf| *prev_f + 1 < nf) {
                    let nf = *prev_f + 1;
                    k.time = (nf as f32 / frames as f32) * dur;
                    snapped.push((nf, k));
                } else {
                    for bi in 0..BONE_COUNT {
                        if k.has_bone(bi) {
                            prev_k.bones.0[bi] = k.bones.0[bi];
                            prev_k.set_bone(bi, true);
                        }
                    }
                    prev_k.easing = k.easing;
                }
                continue;
            }
        }
        k.time = (f_raw as f32 / frames as f32) * dur;
        snapped.push((f_raw, k));
    }
    clip.keyframes = snapped.into_iter().map(|(_, k)| k).collect();
    if let Some(imp) = clip.impact_time {
        clip.impact_time = Some(snap_time_to_fps(imp, dur, fps));
    }
}

pub fn snap_clip_to_frame_grid(clip: &mut ClipDef) {
    snap_clip_to_fps(clip, 60.0);
}

pub fn prev_keyframe_time(clip: &ClipDef, t: f32) -> Option<f32> {
    clip.keyframes
        .iter()
        .filter(|k| k.time < t - 0.012)
        .max_by(|a, b| a.time.total_cmp(&b.time))
        .map(|k| k.time)
}

pub fn next_keyframe_time(clip: &ClipDef, t: f32) -> Option<f32> {
    clip.keyframes
        .iter()
        .filter(|k| k.time > t + 0.012)
        .min_by(|a, b| a.time.total_cmp(&b.time))
        .map(|k| k.time)
}

/// Zero-allocation clip pose sampler returning a stack-allocated [`RigPose`].
pub fn sample_clip_pose(clip: &ClipDef, time: f32) -> RigPose {
    let kfs = &clip.keyframes;
    if kfs.is_empty() {
        return RigPose::ZERO;
    }
    let dur = clip.duration.max(0.05);
    let t = if clip.looped {
        ((time % dur) + dur) % dur
    } else {
        time.clamp(0.0, dur)
    };

    if kfs.iter().all(|k| k.bone_mask == ALL_BONES_MASK) {
        if kfs.len() == 1 {
            return kfs[0].bones;
        }

        let is_sorted = kfs.windows(2).all(|w| w[0].time <= w[1].time);
        let (a, b) = if is_sorted {
            if t <= kfs[0].time {
                return kfs[0].bones;
            }
            let last = &kfs[kfs.len() - 1];
            if t >= last.time {
                return last.bones;
            }
            let mut pair = (&kfs[0], last);
            for w in kfs.windows(2) {
                if t >= w[0].time && t <= w[1].time {
                    pair = (&w[0], &w[1]);
                    break;
                }
            }
            pair
        } else {
            let mut min_kf = &kfs[0];
            let mut max_kf = &kfs[0];
            let mut prev_kf: Option<&Keyframe> = None;
            let mut next_kf: Option<&Keyframe> = None;
            for k in kfs {
                if k.time < min_kf.time {
                    min_kf = k;
                }
                if k.time > max_kf.time {
                    max_kf = k;
                }
                if k.time <= t && prev_kf.map_or(true, |p| k.time > p.time) {
                    prev_kf = Some(k);
                }
                if k.time >= t && next_kf.map_or(true, |n| k.time < n.time) {
                    next_kf = Some(k);
                }
            }
            if t <= min_kf.time {
                return min_kf.bones;
            }
            if t >= max_kf.time {
                return max_kf.bones;
            }
            (prev_kf.unwrap_or(min_kf), next_kf.unwrap_or(max_kf))
        };

        let span = (b.time - a.time).max(1e-5);
        let u = a.easing.eval((t - a.time) / span);

        let mut out = RigPose::ZERO;
        for i in 0..BONE_COUNT {
            let ba = &a.bones.0[i];
            let bb = &b.bones.0[i];
            out.0[i] = BoneTransform {
                pos: [
                    ba.pos[0] + (bb.pos[0] - ba.pos[0]) * u,
                    ba.pos[1] + (bb.pos[1] - ba.pos[1]) * u,
                    ba.pos[2] + (bb.pos[2] - ba.pos[2]) * u,
                ],
                rot: [
                    ba.rot[0] + (bb.rot[0] - ba.rot[0]) * u,
                    ba.rot[1] + (bb.rot[1] - ba.rot[1]) * u,
                    ba.rot[2] + (bb.rot[2] - ba.rot[2]) * u,
                ],
                scale: [
                    ba.scale[0] + (bb.scale[0] - ba.scale[0]) * u,
                    ba.scale[1] + (bb.scale[1] - ba.scale[1]) * u,
                    ba.scale[2] + (bb.scale[2] - ba.scale[2]) * u,
                ],
            };
        }
        return out;
    }

    // Per-bone independent channel sampling when one or more keyframes target specific bones (`bone_mask != 0xFF`).
    let mut out = RigPose::ZERO;
    for i in 0..BONE_COUNT {
        let mut first: Option<&Keyframe> = None;
        let mut last: Option<&Keyframe> = None;
        let mut prev_k: Option<&Keyframe> = None;
        let mut next_k: Option<&Keyframe> = None;
        for k in kfs {
            if !k.has_bone(i) {
                continue;
            }
            if first.map_or(true, |f| k.time < f.time) {
                first = Some(k);
            }
            if last.map_or(true, |l| k.time > l.time) {
                last = Some(k);
            }
            if k.time <= t && prev_k.map_or(true, |p| k.time > p.time) {
                prev_k = Some(k);
            }
            if k.time >= t && next_k.map_or(true, |n| k.time < n.time) {
                next_k = Some(k);
            }
        }
        let Some(fk) = first else {
            continue;
        };
        let tr = match (prev_k, next_k) {
            (None, _) => fk.bones.0[i],
            (_, None) => last.unwrap_or(fk).bones.0[i],
            (Some(pk), Some(nk)) if (nk.time - pk.time).abs() < 1e-5 => pk.bones.0[i],
            (Some(pk), Some(nk)) => {
                let u = pk.easing.eval((t - pk.time) / (nk.time - pk.time).max(1e-5));
                let ba = &pk.bones.0[i];
                let bb = &nk.bones.0[i];
                BoneTransform {
                    pos: [
                        ba.pos[0] + (bb.pos[0] - ba.pos[0]) * u,
                        ba.pos[1] + (bb.pos[1] - ba.pos[1]) * u,
                        ba.pos[2] + (bb.pos[2] - ba.pos[2]) * u,
                    ],
                    rot: [
                        ba.rot[0] + (bb.rot[0] - ba.rot[0]) * u,
                        ba.rot[1] + (bb.rot[1] - ba.rot[1]) * u,
                        ba.rot[2] + (bb.rot[2] - ba.rot[2]) * u,
                    ],
                    scale: [
                        ba.scale[0] + (bb.scale[0] - ba.scale[0]) * u,
                        ba.scale[1] + (bb.scale[1] - ba.scale[1]) * u,
                        ba.scale[2] + (bb.scale[2] - ba.scale[2]) * u,
                    ],
                }
            }
        };
        out.0[i] = tr;
    }
    out
}

pub fn mirror_pose(src: &RigPose) -> RigPose {
    let flip = |b: &BoneTransform| BoneTransform {
        pos: [-b.pos[0], b.pos[1], b.pos[2]],
        rot: [b.rot[0], -b.rot[1], -b.rot[2]],
        scale: b.scale,
    };
    let mut out = *src;
    for idx in [0usize, 1, 2, 8] {
        out.0[idx] = flip(&src.0[idx]);
    }
    // Swap & mirror right/left limbs (3<->4 arms, 6<->7 legs)
    out.0[3] = flip(&src.0[4]);
    out.0[4] = flip(&src.0[3]);
    out.0[6] = flip(&src.0[7]);
    out.0[7] = flip(&src.0[6]);
    if src.0[9].is_identity() {
        out.0[5] = flip(&src.0[5]);
    } else {
        out.0[5] = flip(&src.0[9]);
        out.0[9] = flip(&src.0[5]);
    }
    out
}

/// Remove collinear intermediate points on baked tracks when "Optimizar keyframes" is enabled.
pub fn optimize_track_points(samples: &[(f32, [f32; 3])], eps: f32) -> Vec<(f32, [f32; 3])> {
    if samples.len() <= 2 {
        return samples.to_vec();
    }
    let mut keep = vec![false; samples.len()];
    keep[0] = true;
    keep[samples.len() - 1] = true;

    let mut stack = vec![(0usize, samples.len() - 1)];
    while let Some((s, e)) = stack.pop() {
        if e <= s + 1 {
            continue;
        }
        let (t0, v0) = samples[s];
        let (t1, v1) = samples[e];
        let dt = (t1 - t0).max(1e-5);
        let mut max_err = 0.0f32;
        let mut split = s;
        for i in (s + 1)..e {
            let u = (samples[i].0 - t0) / dt;
            let mut err = 0.0f32;
            for c in 0..3 {
                let interp = v0[c] + (v1[c] - v0[c]) * u;
                err = err.max((samples[i].1[c] - interp).abs());
            }
            if err > max_err {
                max_err = err;
                split = i;
            }
        }
        if max_err > eps && split > s && split < e {
            keep[split] = true;
            stack.push((s, split));
            stack.push((split, e));
        }
    }
    samples
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, v)| *v)
        .collect()
}

fn bt_rot(rx: f32, ry: f32, rz: f32) -> BoneTransform {
    BoneTransform {
        rot: [rx, ry, rz],
        ..Default::default()
    }
}

fn bt_pr(px: f32, py: f32, pz: f32, rx: f32, ry: f32, rz: f32) -> BoneTransform {
    BoneTransform {
        pos: [px, py, pz],
        rot: [rx, ry, rz],
        scale: [1.0, 1.0, 1.0],
    }
}

fn kf(time: f32, easing: EasingMode, pairs: Vec<(&str, BoneTransform)>) -> Keyframe {
    let mut bones = RigPose::ZERO;
    for (name, tr) in pairs {
        bones.insert(name, tr);
    }
    Keyframe { time, easing, bones, bone_mask: ALL_BONES_MASK }
}

pub fn build_demo_presets() -> Vec<ClipDef> {
    vec![
        ClipDef {
            id: "attack_1".into(),
            name: "Attack 1".into(),
            gallery_group: GalleryGroup::Combate,
            duration: 0.48,
            impact_time: Some(0.19),
            looped: false,
            bc_targets: vec![
                "bettercombat:one_handed_slash_horizontal_right".into(),
                "bettercombat:two_handed_slash_horizontal_right".into(),
            ],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: vec![
                WeaponCategory::Espadas,
                WeaponCategory::Katanas,
                WeaponCategory::Dagas,
            ],
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 8.0, -32.0, 0.0)),
                    ("right_arm", bt_rot(-48.0, -42.0, 34.0)),
                    ("left_arm", bt_rot(-44.0, 38.0, -14.0)),
                    ("right_item", bt_rot(-20.0, -14.0, 44.0)),
                ]),
                kf(0.09, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.7, 0.8, 12.0, -52.0, -5.0)),
                    ("head", bt_rot(-6.0, 36.0, 0.0)),
                    ("right_arm", bt_rot(-88.0, -56.0, 60.0)),
                    ("left_arm", bt_rot(-68.0, 46.0, -22.0)),
                    ("right_item", bt_rot(-38.0, -16.0, 60.0)),
                    ("right_leg", bt_rot(22.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-22.0, 0.0, -5.0)),
                ]),
                kf(0.19, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.6, -2.0, 16.0, 44.0, 6.0)),
                    ("head", bt_rot(-10.0, -30.0, 0.0)),
                    ("right_arm", bt_rot(-56.0, 56.0, -52.0)),
                    ("left_arm", bt_rot(-42.0, 18.0, -32.0)),
                    ("right_item", bt_rot(52.0, 18.0, -48.0)),
                    ("right_leg", bt_rot(-24.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(26.0, 0.0, -6.0)),
                ]),
                kf(0.35, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -0.7, -0.8, 8.0, 46.0, 3.0)),
                    ("right_arm", bt_rot(-34.0, 52.0, -44.0)),
                    ("left_arm", bt_rot(-30.0, 14.0, -24.0)),
                    ("right_item", bt_rot(42.0, 12.0, -40.0)),
                ]),
                kf(0.48, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "attack_2".into(),
            name: "Attack 2".into(),
            gallery_group: GalleryGroup::Combate,
            duration: 0.48,
            impact_time: Some(0.19),
            looped: false,
            bc_targets: vec![
                "bettercombat:one_handed_slash_horizontal_left".into(),
                "bettercombat:two_handed_slash_horizontal_left".into(),
            ],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: vec![
                WeaponCategory::Espadas,
                WeaponCategory::Katanas,
                WeaponCategory::Dagas,
            ],
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 10.0, 40.0, 4.0)),
                    ("right_arm", bt_rot(-50.0, 52.0, -48.0)),
                    ("left_arm", bt_rot(-44.0, 18.0, -28.0)),
                    ("right_item", bt_rot(40.0, 10.0, -44.0)),
                ]),
                kf(0.09, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.5, 0.6, 12.0, 54.0, 6.0)),
                    ("head", bt_rot(-6.0, -36.0, 0.0)),
                    ("right_arm", bt_rot(-68.0, 68.0, -62.0)),
                    ("left_arm", bt_rot(-56.0, 12.0, -34.0)),
                    ("right_item", bt_rot(56.0, 16.0, -54.0)),
                ]),
                kf(0.19, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.6, -1.8, 14.0, -42.0, -6.0)),
                    ("head", bt_rot(-8.0, 32.0, 0.0)),
                    ("right_arm", bt_rot(-90.0, -48.0, 54.0)),
                    ("left_arm", bt_rot(-72.0, 44.0, -16.0)),
                    ("right_item", bt_rot(-46.0, -18.0, 52.0)),
                    ("right_leg", bt_rot(24.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-24.0, 0.0, -5.0)),
                ]),
                kf(0.35, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(6.0, -44.0, -4.0)),
                    ("right_arm", bt_rot(-58.0, -52.0, 56.0)),
                    ("right_item", bt_rot(-40.0, -12.0, 44.0)),
                ]),
                kf(0.48, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "attack_3".into(),
            name: "Attack 3".into(),
            gallery_group: GalleryGroup::Combate,
            duration: 0.54,
            impact_time: Some(0.22),
            looped: false,
            bc_targets: vec![
                "bettercombat:one_handed_stab".into(),
                "bettercombat:two_handed_slash_vertical_right".into(),
                "simplyswords:halberd_stab_right".into(),
            ],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: vec![
                WeaponCategory::Espadas,
                WeaponCategory::Katanas,
                WeaponCategory::Dagas,
            ],
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_rot(-6.0, -16.0, 0.0)),
                    ("right_arm", bt_rot(-136.0, -16.0, 12.0)),
                    ("left_arm", bt_rot(-128.0, 24.0, -10.0)),
                    ("right_item", bt_rot(-30.0, 0.0, 0.0)),
                ]),
                kf(0.11, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -0.6, 1.2, -14.0, -22.0, 0.0)),
                    ("head", bt_rot(8.0, 16.0, 0.0)),
                    ("right_arm", bt_rot(-166.0, -18.0, 14.0)),
                    ("left_arm", bt_rot(-160.0, 26.0, -12.0)),
                    ("right_item", bt_rot(-52.0, 0.0, 0.0)),
                ]),
                kf(0.22, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.2, -2.4, 24.0, 18.0, 0.0)),
                    ("head", bt_rot(-14.0, -12.0, 0.0)),
                    ("right_arm", bt_rot(-20.0, 18.0, -12.0)),
                    ("left_arm", bt_rot(-26.0, 26.0, 10.0)),
                    ("right_item", bt_rot(68.0, 0.0, 0.0)),
                    ("right_leg", bt_rot(-26.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(28.0, 0.0, -4.0)),
                ]),
                kf(0.38, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.0, -1.0, 14.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-12.0, 12.0, -8.0)),
                    ("left_arm", bt_rot(-16.0, 20.0, 8.0)),
                    ("right_item", bt_rot(56.0, 0.0, 0.0)),
                ]),
                kf(0.54, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "heavy_attack".into(),
            name: "Heavy Attack".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.72,
            impact_time: Some(0.34),
            looped: false,
            bc_targets: vec![
                "bettercombat:two_handed_slam".into(),
                "bettercombat:two_handed_slam_heavy".into(),
                "bettercombat:two_handed_spin".into(),
            ],
            event_tag: Some("MEDIUM:STRONG_STRIKE".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: vec![
                WeaponCategory::Mandobles,
                WeaponCategory::Hachas,
                WeaponCategory::Lanzas,
            ],
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 6.0, -28.0, 0.0)),
                    ("right_arm", bt_rot(-105.0, -26.0, 24.0)),
                    ("left_arm", bt_rot(-96.0, 38.0, -12.0)),
                    ("right_item", bt_rot(-34.0, 0.0, 16.0)),
                ]),
                kf(0.19, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, 0.4, 1.8, -18.0, -42.0, -6.0)),
                    ("head", bt_rot(12.0, 32.0, 0.0)),
                    ("right_arm", bt_rot(-172.0, -34.0, 36.0)),
                    ("left_arm", bt_rot(-164.0, 40.0, -18.0)),
                    ("right_item", bt_rot(-58.0, 0.0, 20.0)),
                    ("right_leg", bt_rot(24.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(-26.0, 0.0, -6.0)),
                ]),
                kf(0.34, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.8, -3.0, 30.0, 38.0, 8.0)),
                    ("head", bt_rot(-18.0, -26.0, 0.0)),
                    ("right_arm", bt_rot(-18.0, 44.0, -36.0)),
                    ("left_arm", bt_rot(-24.0, 22.0, 12.0)),
                    ("right_item", bt_rot(74.0, 14.0, -20.0)),
                    ("right_leg", bt_rot(-30.0, 0.0, 8.0)),
                    ("left_leg", bt_rot(34.0, 0.0, -8.0)),
                ]),
                kf(0.56, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -1.4, -1.2, 16.0, 28.0, 4.0)),
                    ("right_arm", bt_rot(-12.0, 34.0, -24.0)),
                    ("right_item", bt_rot(60.0, 8.0, -14.0)),
                ]),
                kf(0.72, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "idle".into(),
            name: "Idle".into(),
            gallery_group: GalleryGroup::Movimiento,
            duration: 1.00,
            impact_time: None,
            looped: true,
            bc_targets: vec![],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.5, 0.0, 4.0, -14.0, 0.0)),
                    ("head", bt_rot(-2.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-32.0, -22.0, 14.0)),
                    ("left_arm", bt_rot(-18.0, 18.0, -10.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                    ("right_leg", bt_rot(8.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(-10.0, 0.0, -4.0)),
                ]),
                kf(0.50, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.9, 0.0, 5.0, -14.0, 0.0)),
                    ("head", bt_rot(-2.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-36.0, -24.0, 16.0)),
                    ("left_arm", bt_rot(-21.0, 20.0, -11.0)),
                    ("right_item", bt_rot(13.0, -8.0, 19.0)),
                    ("right_leg", bt_rot(9.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(-11.0, 0.0, -4.0)),
                ]),
                kf(1.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.5, 0.0, 4.0, -14.0, 0.0)),
                    ("head", bt_rot(-2.0, 12.0, 0.0)),
                    ("right_arm", bt_rot(-32.0, -22.0, 14.0)),
                    ("left_arm", bt_rot(-18.0, 18.0, -10.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                    ("right_leg", bt_rot(8.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(-10.0, 0.0, -4.0)),
                ]),
            ],
        },
        ClipDef {
            id: "block".into(),
            name: "Block".into(),
            gallery_group: GalleryGroup::Defensa,
            duration: 0.44,
            impact_time: Some(0.14),
            looped: true,
            bc_targets: vec![],
            event_tag: Some("MEDIUM:GUARD".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: WeaponCategory::MELEE_AND_SHIELD_FAMILIES.to_vec(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 8.0, -18.0, 0.0)),
                    ("head", bt_rot(-4.0, 14.0, 0.0)),
                    ("right_arm", bt_rot(-66.0, -40.0, 20.0)),
                    ("left_arm", bt_rot(-48.0, 30.0, -12.0)),
                    ("right_item", bt_rot(-22.0, -26.0, 56.0)),
                    ("right_leg", bt_rot(15.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-17.0, 0.0, -5.0)),
                ]),
                kf(0.22, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.3, 0.0, 9.0, -18.0, 0.0)),
                    ("head", bt_rot(-4.0, 14.0, 0.0)),
                    ("right_arm", bt_rot(-69.0, -42.0, 22.0)),
                    ("left_arm", bt_rot(-50.0, 32.0, -13.0)),
                    ("right_item", bt_rot(-24.0, -26.0, 58.0)),
                    ("right_leg", bt_rot(16.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-18.0, 0.0, -5.0)),
                ]),
                kf(0.44, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 8.0, -18.0, 0.0)),
                    ("head", bt_rot(-4.0, 14.0, 0.0)),
                    ("right_arm", bt_rot(-66.0, -40.0, 20.0)),
                    ("left_arm", bt_rot(-48.0, 30.0, -12.0)),
                    ("right_item", bt_rot(-22.0, -26.0, 56.0)),
                    ("right_leg", bt_rot(15.0, 0.0, 5.0)),
                    ("left_leg", bt_rot(-17.0, 0.0, -5.0)),
                ]),
            ],
        },
        ClipDef {
            id: "parry".into(),
            name: "Parry".into(),
            gallery_group: GalleryGroup::Defensa,
            duration: 0.42,
            impact_time: Some(0.11),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("MEDIUM:PARRY_SUCCESS".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: WeaponCategory::MELEE_AND_SHIELD_FAMILIES.to_vec(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 6.0, -20.0, 0.0)),
                    ("right_arm", bt_rot(-70.0, -42.0, 22.0)),
                    ("left_arm", bt_rot(-50.0, 30.0, -12.0)),
                    ("right_item", bt_rot(-22.0, -24.0, 54.0)),
                ]),
                kf(0.11, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -0.4, -0.8, -6.0, 28.0, -6.0)),
                    ("head", bt_rot(2.0, -20.0, 0.0)),
                    ("right_arm", bt_rot(-94.0, 36.0, 44.0)),
                    ("left_arm", bt_rot(-24.0, -22.0, -32.0)),
                    ("right_item", bt_rot(-38.0, 28.0, -44.0)),
                    ("right_leg", bt_rot(-16.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(18.0, 0.0, -6.0)),
                ]),
                kf(0.28, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(-2.0, 18.0, -3.0)),
                    ("right_arm", bt_rot(-78.0, 24.0, 34.0)),
                    ("right_item", bt_rot(-26.0, 18.0, -30.0)),
                ]),
                kf(0.42, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "dodge".into(),
            name: "Dodge".into(),
            gallery_group: GalleryGroup::Defensa,
            duration: 0.40,
            impact_time: Some(0.12),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:EVADE_DASH".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.8, 0.0, 18.0, 0.0, 0.0)),
                    ("right_arm", bt_rot(38.0, -12.0, 18.0)),
                    ("left_arm", bt_rot(38.0, 12.0, -18.0)),
                ]),
                kf(0.14, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.6, -2.5, 28.0, -18.0, -10.0)),
                    ("head", bt_rot(-20.0, 14.0, 6.0)),
                    ("right_arm", bt_rot(56.0, -16.0, 26.0)),
                    ("left_arm", bt_rot(56.0, 16.0, -26.0)),
                    ("right_leg", bt_rot(-32.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(38.0, 0.0, -4.0)),
                ]),
                kf(0.40, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "equip".into(),
            name: "Equip".into(),
            gallery_group: GalleryGroup::Interaccion,
            duration: 0.44,
            impact_time: Some(0.22),
            looped: false,
            bc_targets: vec![],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(6.0, 18.0, 0.0)),
                    ("right_arm", bt_rot(-12.0, 32.0, 22.0)),
                    ("left_arm", bt_rot(-28.0, 24.0, 0.0)),
                    ("right_item", bt_rot(48.0, 0.0, 28.0)),
                ]),
                kf(0.18, EasingMode::Snap, vec![
                    ("waist", bt_rot(4.0, -14.0, 0.0)),
                    ("right_arm", bt_rot(-74.0, -32.0, 24.0)),
                    ("right_item", bt_rot(-16.0, -10.0, 32.0)),
                ]),
                kf(0.44, EasingMode::EaseInOut, vec![
                    ("waist", bt_rot(4.0, -14.0, 0.0)),
                    ("right_arm", bt_rot(-34.0, -20.0, 14.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                ]),
            ],
        },
        ClipDef {
            id: "unequip".into(),
            name: "Unequip".into(),
            gallery_group: GalleryGroup::Interaccion,
            duration: 0.40,
            impact_time: None,
            looped: false,
            bc_targets: vec![],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("right_arm", bt_rot(-34.0, -20.0, 14.0)),
                    ("right_item", bt_rot(10.0, -8.0, 18.0)),
                ]),
                kf(0.20, EasingMode::EaseOut, vec![
                    ("waist", bt_rot(5.0, 16.0, 0.0)),
                    ("right_arm", bt_rot(-4.0, 28.0, 18.0)),
                    ("right_item", bt_rot(44.0, 0.0, 24.0)),
                ]),
                kf(0.40, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "run".into(),
            name: "Run".into(),
            gallery_group: GalleryGroup::Movimiento,
            duration: 0.56,
            impact_time: None,
            looped: true,
            bc_targets: vec![],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.8, 0.0, 16.0, -10.0, 0.0)),
                    ("head", bt_rot(-12.0, 8.0, 0.0)),
                    ("right_arm", bt_rot(34.0, -8.0, 12.0)),
                    ("left_arm", bt_rot(-46.0, 10.0, -10.0)),
                    ("right_leg", bt_rot(-44.0, 0.0, 3.0)),
                    ("left_leg", bt_rot(44.0, 0.0, -3.0)),
                    ("right_item", bt_rot(28.0, 0.0, 12.0)),
                ]),
                kf(0.28, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.8, 0.0, 16.0, 10.0, 0.0)),
                    ("head", bt_rot(-12.0, -8.0, 0.0)),
                    ("right_arm", bt_rot(44.0, 10.0, 14.0)),
                    ("left_arm", bt_rot(38.0, -10.0, -10.0)),
                    ("right_leg", bt_rot(44.0, 0.0, 3.0)),
                    ("left_leg", bt_rot(-44.0, 0.0, -3.0)),
                    ("right_item", bt_rot(34.0, 0.0, 14.0)),
                ]),
                kf(0.56, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.8, 0.0, 16.0, -10.0, 0.0)),
                    ("head", bt_rot(-12.0, 8.0, 0.0)),
                    ("right_arm", bt_rot(34.0, -8.0, 12.0)),
                    ("left_arm", bt_rot(-46.0, 10.0, -10.0)),
                    ("right_leg", bt_rot(-44.0, 0.0, 3.0)),
                    ("left_leg", bt_rot(44.0, 0.0, -3.0)),
                    ("right_item", bt_rot(28.0, 0.0, 12.0)),
                ]),
            ],
        },
        ClipDef {
            id: "walk".into(),
            name: "Walk".into(),
            gallery_group: GalleryGroup::Movimiento,
            duration: 0.76,
            impact_time: None,
            looped: true,
            bc_targets: vec![],
            event_tag: None,
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.3, 0.0, 4.0, -6.0, 0.0)),
                    ("right_arm", bt_rot(18.0, -8.0, 8.0)),
                    ("left_arm", bt_rot(-22.0, 8.0, -6.0)),
                    ("right_leg", bt_rot(-24.0, 0.0, 2.0)),
                    ("left_leg", bt_rot(24.0, 0.0, -2.0)),
                    ("right_item", bt_rot(14.0, 0.0, 10.0)),
                ]),
                kf(0.38, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.3, 0.0, 4.0, 6.0, 0.0)),
                    ("right_arm", bt_rot(-18.0, 8.0, 8.0)),
                    ("left_arm", bt_rot(22.0, -8.0, -6.0)),
                    ("right_leg", bt_rot(24.0, 0.0, 2.0)),
                    ("left_leg", bt_rot(-24.0, 0.0, -2.0)),
                    ("right_item", bt_rot(18.0, 0.0, 10.0)),
                ]),
                kf(0.76, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -0.3, 0.0, 4.0, -6.0, 0.0)),
                    ("right_arm", bt_rot(18.0, -8.0, 8.0)),
                    ("left_arm", bt_rot(-22.0, 8.0, -6.0)),
                    ("right_leg", bt_rot(-24.0, 0.0, 2.0)),
                    ("left_leg", bt_rot(24.0, 0.0, -2.0)),
                    ("right_item", bt_rot(14.0, 0.0, 10.0)),
                ]),
            ],
        },
        ClipDef {
            id: "guard_break".into(),
            name: "Guard Break".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.72,
            impact_time: Some(0.18),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:GUARD_BREAK".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: WeaponCategory::MELEE_AND_SHIELD_FAMILIES.to_vec(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_rot(6.0, 0.0, 0.0)),
                    ("right_arm", bt_rot(-60.0, -28.0, 14.0)),
                    ("left_arm", bt_rot(-60.0, 28.0, -14.0)),
                ]),
                kf(0.18, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -0.6, 2.4, -26.0, 0.0, 0.0)),
                    ("head", bt_rot(-24.0, 14.0, 10.0)),
                    ("right_arm", bt_rot(-42.0, -24.0, 68.0)),
                    ("left_arm", bt_rot(-42.0, 24.0, -68.0)),
                    ("right_leg", bt_rot(18.0, 0.0, 10.0)),
                    ("left_leg", bt_rot(-16.0, 0.0, -10.0)),
                ]),
                kf(0.46, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.2, 1.4, -14.0, 0.0, 0.0)),
                    ("head", bt_rot(-10.0, -10.0, -6.0)),
                    ("right_arm", bt_rot(-24.0, -14.0, 48.0)),
                    ("left_arm", bt_rot(-24.0, 14.0, -48.0)),
                ]),
                kf(0.72, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "air_kick".into(),
            name: "Air Kick".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.42,
            impact_time: Some(0.14),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:AIR_KICK".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, -1.0, 0.0, 12.0, 0.0, 0.0)),
                    ("right_leg", bt_rot(-56.0, 0.0, 4.0)),
                    ("left_leg", bt_rot(22.0, 0.0, -4.0)),
                    ("right_arm", bt_rot(-28.0, 0.0, 22.0)),
                    ("left_arm", bt_rot(-28.0, 0.0, -22.0)),
                ]),
                kf(0.14, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, 2.8, -0.6, -14.0, 10.0, 0.0)),
                    ("head", bt_rot(12.0, 0.0, 0.0)),
                    ("right_leg", bt_rot(62.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(-64.0, 0.0, -6.0)),
                    ("right_arm", bt_rot(24.0, 0.0, 46.0)),
                    ("left_arm", bt_rot(24.0, 0.0, -46.0)),
                ]),
                kf(0.42, EasingMode::Linear, vec![]),
            ],
        },
        ClipDef {
            id: "execution".into(),
            name: "Execution".into(),
            gallery_group: GalleryGroup::Especiales,
            duration: 0.62,
            impact_time: Some(0.28),
            looped: false,
            bc_targets: vec![],
            event_tag: Some("ANY:EXECUTION".into()),
            source: ClipSource::RellSeas,
            status: CurationStatus::Pendiente,
            vfx_events: Vec::new(),
            allowed_categories: WeaponCategory::MELEE_AND_SHIELD_FAMILIES.to_vec(),
            allowed_weapons: Vec::new(),
            keyframes: vec![
                kf(0.00, EasingMode::EaseInOut, vec![
                    ("waist", bt_pr(0.0, -1.8, 0.8, 14.0, -36.0, 0.0)),
                    ("right_arm", bt_rot(-126.0, -30.0, 32.0)),
                    ("left_arm", bt_rot(-74.0, 38.0, -20.0)),
                    ("right_item", bt_rot(-36.0, 0.0, 22.0)),
                ]),
                kf(0.16, EasingMode::Snap, vec![
                    ("waist", bt_pr(0.0, 1.6, 1.6, -12.0, -52.0, -6.0)),
                    ("head", bt_rot(8.0, 36.0, 0.0)),
                    ("right_arm", bt_rot(-168.0, -32.0, 34.0)),
                    ("left_arm", bt_rot(-110.0, 42.0, -24.0)),
                    ("right_item", bt_rot(-54.0, 0.0, 24.0)),
                    ("right_leg", bt_rot(32.0, 0.0, 6.0)),
                    ("left_leg", bt_rot(-34.0, 0.0, -6.0)),
                ]),
                kf(0.28, EasingMode::EaseOut, vec![
                    ("waist", bt_pr(0.0, -2.6, -3.2, 28.0, 48.0, 8.0)),
                    ("head", bt_rot(-18.0, -36.0, 0.0)),
                    ("right_arm", bt_rot(-18.0, 56.0, -48.0)),
                    ("left_arm", bt_rot(42.0, -26.0, -38.0)),
                    ("right_item", bt_rot(72.0, 18.0, -38.0)),
                    ("right_leg", bt_rot(-32.0, 0.0, 8.0)),
                    ("left_leg", bt_rot(36.0, 0.0, -8.0)),
                ]),
                kf(0.62, EasingMode::Linear, vec![]),
            ],
        },
    ]
}
