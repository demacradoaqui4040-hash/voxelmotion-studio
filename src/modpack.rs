// VoxelMotion Studio - Personal Local Modpack & RELL Seas Integration Profile
// Loads curated weapons, 7-JAR layered sprites, combat animations (5 sources), and personal curation state.
// Personal paths and curation decisions live in gitignored `profile.local.json`, never in tracked repo defaults.

use crate::catalog::{decode_png_top_frame, make_archetype_sprite};
use crate::model::{
    bone_index_by_name, ClipDef, ClipSource, CurationStatus, EasingMode, GalleryGroup, Keyframe,
    RigPose, SpritePixels, VfxEvent, VfxKind, WeaponCategory, WeaponRecord,
};
use crate::player_animator::parse_player_animator_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zip::ZipArchive;

fn default_true() -> bool {
    true
}

fn default_timeline_fps() -> i32 {
    20
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalProfile {
    #[serde(default)]
    pub servernuevo_root: Option<String>,
    #[serde(default)]
    pub weapon_status: HashMap<String, CurationStatus>,
    #[serde(default)]
    pub clip_status: HashMap<String, CurationStatus>,
    #[serde(default)]
    pub clip_vfx: HashMap<String, Vec<VfxEvent>>,
    #[serde(default)]
    pub clip_impact_overrides: HashMap<String, f32>,
    #[serde(default)]
    pub clip_modified_clips: HashMap<String, ClipDef>,
    #[serde(default)]
    pub clip_allowed_categories: HashMap<String, Vec<WeaponCategory>>,
    #[serde(default)]
    pub clip_allowed_weapons: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub theme_idx: i32,
    #[serde(default = "default_true")]
    pub ui_motion_enabled: bool,
    #[serde(default)]
    pub gpu_low_power: bool,
    #[serde(default = "default_timeline_fps")]
    pub timeline_fps: i32,
    #[serde(default)]
    pub skin_path: Option<String>,
    #[serde(default)]
    pub skin_slim: bool,
    #[serde(default = "default_true")]
    pub skin_outer_layer: bool,
}

impl Default for LocalProfile {
    fn default() -> Self {
        Self {
            servernuevo_root: None,
            weapon_status: HashMap::new(),
            clip_status: HashMap::new(),
            clip_vfx: HashMap::new(),
            clip_impact_overrides: HashMap::new(),
            clip_modified_clips: HashMap::new(),
            clip_allowed_categories: HashMap::new(),
            clip_allowed_weapons: HashMap::new(),
            theme_idx: 0,
            ui_motion_enabled: true,
            gpu_low_power: false,
            timeline_fps: 20,
            skin_path: None,
            skin_slim: false,
            skin_outer_layer: true,
        }
    }
}

pub fn discover_servernuevo_root(workspace_root: &Path, profile: &LocalProfile) -> Option<PathBuf> {
    if let Some(ref p) = profile.servernuevo_root {
        let pb = PathBuf::from(p);
        if pb.join("modpack").join("curated_weapons.json").is_file() {
            return Some(pb);
        }
    }
    if let Some(parent) = workspace_root.parent() {
        let sibling = parent.join("servernuevo");
        if sibling.join("modpack").join("curated_weapons.json").is_file() {
            return Some(sibling);
        }
    }
    None
}

pub fn load_local_profile(workspace_root: &Path) -> LocalProfile {
    let p = workspace_root.join("profile.local.json");
    let Ok(raw) = std::fs::read_to_string(p) else {
        return LocalProfile::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_local_profile(workspace_root: &Path, profile: &LocalProfile) -> Result<PathBuf, String> {
    let p = workspace_root.join("profile.local.json");
    let tmp = workspace_root.join("profile.local.json.tmp");
    let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, json).map_err(|e| format!("No se pudo escribir profile.local.json.tmp: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp, &p) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("No se pudo actualizar profile.local.json: {e}"));
    }
    Ok(p)
}

#[derive(Deserialize)]
struct CuratedManifest {
    #[serde(default)]
    weapons: Vec<CuratedWeaponEntry>,
}

#[derive(Deserialize)]
struct CuratedWeaponEntry {
    id: String,
    item_id: String,
    name: String,
    category: String,
    mod_source: String,
    weapon_class: String,
    mesh_archetype: String,
}

fn parse_weapon_category(raw: &str) -> WeaponCategory {
    match raw.to_ascii_lowercase().as_str() {
        "espadas" => WeaponCategory::Espadas,
        "katanas" => WeaponCategory::Katanas,
        "mandobles" => WeaponCategory::Mandobles,
        "hachas" => WeaponCategory::Hachas,
        "lanzas" => WeaponCategory::Lanzas,
        "dagas" => WeaponCategory::Dagas,
        "punos" | "puños" => WeaponCategory::Punos,
        "escudos" => WeaponCategory::Escudos,
        "arcos" => WeaponCategory::Arcos,
        "fuego" => WeaponCategory::Fuego,
        _ => WeaponCategory::Otros,
    }
}

#[derive(Clone, Debug, Default)]
struct ParsedWeaponAttr {
    parent: String,
    category: String,
    pose: String,
    attacks: Vec<String>,
}

fn parse_weapon_attribute_json(raw_json: &str) -> Option<ParsedWeaponAttr> {
    let val: Value = serde_json::from_str(raw_json).ok()?;
    let parent = val
        .get("parent")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let attrs = val.get("attributes");
    let category = attrs
        .and_then(|a| a.get("category"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pose = attrs
        .and_then(|a| a.get("pose"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let attacks = attrs
        .and_then(|a| a.get("attacks"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|atk| atk.get("animation").and_then(|s| s.as_str()))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    Some(ParsedWeaponAttr {
        parent,
        category,
        pose,
        attacks,
    })
}

fn derive_bc_archetype(attr: &ParsedWeaponAttr) -> String {
    if !attr.parent.is_empty() {
        attr.parent.clone()
    } else if !attr.category.is_empty() {
        format!("bettercombat:{}", attr.category)
    } else if let Some(first_anim) = attr.attacks.first() {
        first_anim.clone()
    } else {
        String::new()
    }
}

fn classify_bc_template_categories(stem: &str) -> Vec<WeaponCategory> {
    match stem.to_ascii_lowercase().as_str() {
        "sword" | "cutlass" | "coral_blade" => vec![WeaponCategory::Espadas],
        "katana" => vec![WeaponCategory::Katanas],
        "claymore" => vec![WeaponCategory::Mandobles],
        "axe" | "double_axe" | "heavy_axe" | "hammer" | "mace" | "anchor" | "pickaxe" => {
            vec![WeaponCategory::Hachas]
        }
        "spear" | "halberd" | "glaive" | "lance" | "trident" | "scythe" | "battlestaff"
        | "staff" | "twin_blade" => vec![WeaponCategory::Lanzas],
        "dagger" | "rapier" | "sickle" | "soul_knife" | "claw" | "wand" => {
            vec![WeaponCategory::Dagas]
        }
        "fist" => vec![WeaponCategory::Punos],
        "bow" => vec![WeaponCategory::Arcos],
        "crossbow" | "crossbow_two_handed_heavy" | "crossbow_two_handed_light" => {
            vec![WeaponCategory::Arcos, WeaponCategory::Fuego]
        }
        _ => vec![WeaponCategory::Espadas],
    }
}

fn family_from_attr_token(token: &str) -> Option<(WeaponCategory, &'static str, &'static str)> {
    let clean = token
        .trim()
        .to_ascii_lowercase()
        .rsplit(':')
        .next()
        .unwrap_or("")
        .to_string();
    match clean.as_str() {
        "katana" | "pose_two_handed_katana" => Some((WeaponCategory::Katanas, "MEDIUM", "katana")),
        "claymore" | "greatsword" | "pose_greatsword" | "pose_two_handed_dragable" => {
            Some((WeaponCategory::Mandobles, "HEAVY", "heavy"))
        }
        "greataxe"
        | "greathammer"
        | "heavy_axe"
        | "double_axe"
        | "axe"
        | "hammer"
        | "mace"
        | "anchor"
        | "pickaxe"
        | "pose_two_handed_greataxe"
        | "pose_two_handed_heavy" => Some((WeaponCategory::Hachas, "HEAVY", "heavy")),
        "halberd"
        | "glaive"
        | "spear"
        | "trident"
        | "lance"
        | "scythe"
        | "battlestaff"
        | "warglaive"
        | "staff"
        | "twin_blade"
        | "pose_halberd"
        | "pose_two_handed_polearm"
        | "pose_soulrender"
        | "pose_warglaive_dual_animated" => Some((WeaponCategory::Lanzas, "HEAVY", "polearm")),
        "dagger" | "rapier" | "sai" | "soul_knife" | "sickle" | "chakram" | "claw" | "wand" => {
            Some((WeaponCategory::Dagas, "LIGHT", "dagger"))
        }
        "fist" => Some((WeaponCategory::Punos, "UNARMED", "dagger")),
        "pose_two_handed_bow" | "bow" => Some((WeaponCategory::Arcos, "RANGED", "arc")),
        "pose_two_handed_crossbow" | "crossbow" => Some((WeaponCategory::Arcos, "RANGED", "arc")),
        "sword" | "cutlass" | "coral_blade" | "pose_two_handed_sword" => {
            Some((WeaponCategory::Espadas, "MEDIUM", "sword"))
        }
        _ => None,
    }
}

fn classify_modpack_weapon(
    ns: &str,
    slug: &str,
    attr: Option<&ParsedWeaponAttr>,
    manifest_cat: &str,
) -> (WeaponCategory, &'static str, &'static str) {
    let ns_low = ns.to_ascii_lowercase();
    let s_low = slug.to_ascii_lowercase();

    // Rule A — Explicit namespace & item overrides (highest precision).
    if ns_low == "musketmod"
        || s_low == "laser_gatling"
        || s_low.contains("musket")
        || s_low.contains("flintlock")
        || s_low.contains("pistol")
        || s_low.contains("blunderbuss")
    {
        return (WeaponCategory::Fuego, "RANGED", "ranged");
    }

    if ns_low == "cataclysm" {
        match s_low.as_str() {
            "black_steel_targe" | "bulwark_of_the_flame" => {
                return (WeaponCategory::Escudos, "HEAVY", "heavy");
            }
            "gauntlet_of_bulwark"
            | "gauntlet_of_guard"
            | "gauntlet_of_maelstrom"
            | "tidal_claws" => {
                return (WeaponCategory::Punos, "UNARMED", "dagger");
            }
            "laser_gatling" => {
                return (WeaponCategory::Fuego, "RANGED", "ranged");
            }
            "black_steel_axe" | "ceraunus" | "infernal_forge" | "void_forge"
            | "the_annihilator" => {
                return (WeaponCategory::Hachas, "HEAVY", "heavy");
            }
            "meat_shredder" | "the_incinerator" => {
                return (WeaponCategory::Mandobles, "HEAVY", "heavy");
            }
            "ancient_spear" | "astrape" | "coral_bardiche" | "coral_spear" | "soul_render" => {
                return (WeaponCategory::Lanzas, "HEAVY", "polearm");
            }
            "athame" => {
                return (WeaponCategory::Dagas, "LIGHT", "dagger");
            }
            "cursed_bow" => {
                return (WeaponCategory::Arcos, "RANGED", "arc");
            }
            "black_steel_sword" => {
                return (WeaponCategory::Espadas, "MEDIUM", "sword");
            }
            _ => {}
        }
    }

    if s_low == "air" || s_low == "haki_fists" || s_low.contains("gauntlet") {
        return (WeaponCategory::Punos, "UNARMED", "dagger");
    }
    if ns_low == "simplyswords" && s_low == "mjolnir" {
        return (WeaponCategory::Hachas, "HEAVY", "heavy");
    }
    if s_low.contains("cutlass") {
        return (WeaponCategory::Espadas, "MEDIUM", "sword");
    }
    if ns_low == "minecraft" && s_low.ends_with("_axe") {
        return (WeaponCategory::Hachas, "HEAVY", "heavy");
    }
    if ns_low == "mowziesmobs" {
        match s_low.as_str() {
            "wrought_axe" => return (WeaponCategory::Hachas, "HEAVY", "heavy"),
            "spear" | "sculptor_staff" => return (WeaponCategory::Lanzas, "HEAVY", "polearm"),
            "naga_fang_dagger" => return (WeaponCategory::Dagas, "LIGHT", "dagger"),
            "earthrend_gauntlet" => return (WeaponCategory::Punos, "UNARMED", "dagger"),
            "blowgun" => return (WeaponCategory::Arcos, "RANGED", "arc"),
            _ => {}
        }
    }
    if ns_low == "farmersdelight" || s_low.ends_with("_knife") {
        return (WeaponCategory::Dagas, "LIGHT", "dagger");
    }

    // Rule B — Official JAR weapon_attributes (`category` BEFORE `parent`!).
    if let Some(a) = attr {
        if let Some(res) = family_from_attr_token(&a.category) {
            return res;
        }
        if let Some(res) = family_from_attr_token(&a.pose) {
            return res;
        }
        if let Some(res) = family_from_attr_token(&a.parent) {
            return res;
        }
    }

    // Rule C — Manifest & slug fallback (never returns `Otros`).
    if s_low.contains("bow") || s_low.contains("crossbow") || s_low.contains("blowgun") {
        (WeaponCategory::Arcos, "RANGED", "arc")
    } else if s_low.contains("shield") || s_low.contains("targe") {
        (WeaponCategory::Escudos, "HEAVY", "heavy")
    } else if s_low.contains("axe") || s_low.contains("hammer") || s_low.contains("mace") {
        (WeaponCategory::Hachas, "HEAVY", "heavy")
    } else if s_low.contains("claymore") || s_low.contains("greatsword") || s_low.contains("cleaver") {
        (WeaponCategory::Mandobles, "HEAVY", "heavy")
    } else if s_low.contains("spear")
        || s_low.contains("halberd")
        || s_low.contains("glaive")
        || s_low.contains("trident")
        || s_low.contains("staff")
        || s_low.contains("scythe")
    {
        (WeaponCategory::Lanzas, "HEAVY", "polearm")
    } else if s_low.contains("katana") {
        (WeaponCategory::Katanas, "MEDIUM", "katana")
    } else if s_low.contains("dagger")
        || s_low.contains("knife")
        || s_low.contains("sai")
        || s_low.contains("rapier")
        || s_low.contains("chakram")
    {
        (WeaponCategory::Dagas, "LIGHT", "dagger")
    } else {
        match parse_weapon_category(manifest_cat) {
            WeaponCategory::Katanas => (WeaponCategory::Katanas, "MEDIUM", "katana"),
            WeaponCategory::Mandobles => (WeaponCategory::Mandobles, "HEAVY", "heavy"),
            WeaponCategory::Hachas => (WeaponCategory::Hachas, "HEAVY", "heavy"),
            WeaponCategory::Lanzas => (WeaponCategory::Lanzas, "HEAVY", "polearm"),
            WeaponCategory::Dagas => (WeaponCategory::Dagas, "LIGHT", "dagger"),
            WeaponCategory::Punos => (WeaponCategory::Punos, "UNARMED", "dagger"),
            WeaponCategory::Escudos => (WeaponCategory::Escudos, "HEAVY", "heavy"),
            WeaponCategory::Arcos => (WeaponCategory::Arcos, "RANGED", "arc"),
            WeaponCategory::Fuego => (WeaponCategory::Fuego, "RANGED", "ranged"),
            _ => (WeaponCategory::Espadas, "MEDIUM", "sword"),
        }
    }
}

fn sorted_zip_entries(zip: &mut ZipArchive<File>, pred: impl Fn(&str) -> bool) -> Vec<String> {
    let mut entries: Vec<String> = (0..zip.len())
        .filter_map(|i| {
            zip.by_index(i)
                .ok()
                .and_then(|f| f.name().ok().map(|n| n.into_owned()))
        })
        .filter(|n| pred(n))
        .collect();
    entries.sort();
    entries
}

fn collect_jar_weapon_attributes(
    zip: &mut ZipArchive<File>,
    ns: &str,
) -> HashMap<String, ParsedWeaponAttr> {
    let prefix = format!("data/{ns}/weapon_attributes/");
    let paths = sorted_zip_entries(zip, |n| {
        n.starts_with(&prefix) && !n.contains("/compat/") && n.ends_with(".json")
    });
    let mut map = HashMap::new();
    for path in paths {
        if let Some(raw_attr) = read_zip_string(zip, &path) {
            if let Some(parsed) = parse_weapon_attribute_json(&raw_attr) {
                let slug = path.rsplit('/').next().unwrap_or("").trim_end_matches(".json");
                map.insert(slug.to_string(), parsed);
            }
        }
    }
    map
}

fn vanilla_jar_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let p = PathBuf::from(appdata)
        .join(".sklauncher")
        .join("versions")
        .join("1.20.1")
        .join("1.20.1.jar");
    p.is_file().then_some(p)
}

fn open_zip(path: &Path) -> Option<ZipArchive<File>> {
    let f = File::open(path).ok()?;
    ZipArchive::new(f).ok()
}

fn read_zip_string(zip: &mut ZipArchive<File>, entry: &str) -> Option<String> {
    let mut f = zip.by_name(entry).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

fn read_zip_bytes(zip: &mut ZipArchive<File>, entry: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(entry).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn composite_sprite_layers(layers: &[Arc<SpritePixels>]) -> Option<Arc<SpritePixels>> {
    let first = layers.first()?;
    if layers.len() == 1 {
        return Some(first.clone());
    }
    let w = first.width.max(1);
    let h = first.height.max(1);
    let mut canvas = first.rgba.clone();
    for layer in &layers[1..] {
        if layer.width != w || layer.height != h || layer.rgba.len() != canvas.len() {
            continue;
        }
        for i in (0..canvas.len()).step_by(4) {
            let sa = layer.rgba[i + 3] as f32 / 255.0;
            if sa <= 0.001 {
                continue;
            }
            let inv = 1.0 - sa;
            for c in 0..3 {
                let dst = canvas[i + c] as f32;
                let src = layer.rgba[i + c] as f32;
                canvas[i + c] = (src * sa + dst * inv).clamp(0.0, 255.0).round() as u8;
            }
            let da = canvas[i + 3] as f32 / 255.0;
            canvas[i + 3] = ((sa + da * inv) * 255.0).clamp(0.0, 255.0).round() as u8;
        }
    }
    Some(SpritePixels::new(w, h, canvas))
}

fn texture_ref_to_zip_path(tex_ref: &str, default_ns: &str) -> String {
    let clean = tex_ref.trim();
    let (ns, rel) = if let Some((a, b)) = clean.split_once(':') {
        (a, b)
    } else {
        (default_ns, clean)
    };
    let rel_no_ext = rel.strip_suffix(".png").unwrap_or(rel);
    if rel_no_ext.starts_with("textures/") {
        format!("assets/{ns}/{rel_no_ext}.png")
    } else {
        format!("assets/{ns}/textures/{rel_no_ext}.png")
    }
}

fn tint_for_slug(slug: &str, arch: &str) -> [u8; 3] {
    let s = slug.to_ascii_lowercase();
    if s.contains("fire") || s.contains("brimstone") || s.contains("flame") {
        [242, 130, 72]
    } else if s.contains("frost") || s.contains("ice") || s.contains("aqua") || s.contains("diamond") {
        [116, 214, 244]
    } else if s.contains("runic") || s.contains("shadow") || s.contains("soul") || s.contains("magic") || s.contains("twilight") {
        [176, 128, 248]
    } else if s.contains("gold") || s.contains("sun") {
        [244, 206, 98]
    } else if s.contains("netherite") || s.contains("wither") {
        [124, 112, 128]
    } else {
        match arch {
            "katana" => [186, 202, 232],
            "heavy" => [164, 148, 210],
            "polearm" => [194, 212, 224],
            "dagger" => [162, 220, 230],
            "ranged" | "arc" => [188, 152, 108],
            _ => [180, 204, 228],
        }
    }
}

fn resolve_sprite_from_jar(
    zips: &mut HashMap<String, ZipArchive<File>>,
    ns: &str,
    slug: &str,
) -> Option<Arc<SpritePixels>> {
    let z = zips.get_mut(ns)?;

    // Mowzie's Mobs: strongly prefer flat *_icon.png over 3D UV atlases.
    if ns == "mowziesmobs" {
        let icon_p = format!("assets/mowziesmobs/textures/item/{slug}_icon.png");
        if let Some(bytes) = read_zip_bytes(z, &icon_p) {
            if let Some(sp) = decode_png_top_frame(&bytes) {
                return Some(sp);
            }
        }
    }

    // Vanilla special item models (`crossbow` standby, `trident` flat item sprite).
    if ns == "minecraft" && slug == "crossbow" {
        if let Some(bytes) = read_zip_bytes(z, "assets/minecraft/textures/item/crossbow_standby.png") {
            if let Some(sp) = decode_png_top_frame(&bytes) {
                return Some(sp);
            }
        }
    }

    let model_path = format!("assets/{ns}/models/item/{slug}.json");
    if let Some(model_raw) = read_zip_string(z, &model_path) {
        if let Ok(model_val) = serde_json::from_str::<Value>(&model_raw) {
            if let Some(tex_obj) = model_val.get("textures").and_then(|v| v.as_object()) {
                let mut layer_keys: Vec<(usize, String)> = tex_obj
                    .iter()
                    .filter_map(|(k, v)| {
                        let rest = k.strip_prefix("layer")?;
                        let idx = rest.parse::<usize>().ok()?;
                        Some((idx, v.as_str()?.to_string()))
                    })
                    .collect();
                layer_keys.sort_by_key(|(idx, _)| *idx);

                let mut decoded_layers = Vec::new();
                for (_, tex_ref) in layer_keys {
                    let zp = texture_ref_to_zip_path(&tex_ref, ns);
                    if let Some(bytes) = read_zip_bytes(z, &zp) {
                        if let Some(sp) = decode_png_top_frame(&bytes) {
                            decoded_layers.push(sp);
                        }
                    }
                }
                if !decoded_layers.is_empty() {
                    return composite_sprite_layers(&decoded_layers);
                }

                // Fallback for models using `textures.texture` (Simply Swords frostfall) or `textures["1"]` (Musket Mod).
                for key in ["texture", "1", "0"] {
                    if let Some(tex_ref) = tex_obj.get(key).and_then(|v| v.as_str()) {
                        let zp = texture_ref_to_zip_path(tex_ref, ns);
                        if let Some(bytes) = read_zip_bytes(z, &zp) {
                            if let Some(sp) = decode_png_top_frame(&bytes) {
                                return Some(sp);
                            }
                        }
                    }
                }
            }
        }
    }

    // Direct item texture fallback (handles Cataclysm `builtin/entity` weapons cleanly).
    let direct = format!("assets/{ns}/textures/item/{slug}.png");
    if let Some(bytes) = read_zip_bytes(z, &direct) {
        if let Some(sp) = decode_png_top_frame(&bytes) {
            return Some(sp);
        }
    }
    None
}

/// Load the curated RELL Seas modpack weapons + layered JAR sprites + local curation state.
pub fn load_curated_weapons(
    server_root: &Path,
    profile: &LocalProfile,
) -> Result<(Vec<WeaponRecord>, usize), String> {
    let manifest_path = server_root.join("modpack").join("curated_weapons.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("No se pudo leer curated_weapons.json: {e}"))?;
    let manifest: CuratedManifest =
        serde_json::from_str(&raw).map_err(|e| format!("curated_weapons.json inválido: {e}"))?;
    let raw_count = manifest.weapons.len();

    let mods_dir = server_root.join("sklauncher_modpack").join("mods");
    let mut jar_paths: HashMap<String, PathBuf> = HashMap::new();
    jar_paths.insert("simplyswords".into(), mods_dir.join("simplyswords-forge-neoforge-1.70.2-1.20.1.jar"));
    jar_paths.insert("cataclysm".into(), mods_dir.join("L_Enders_Cataclysm-3.31.jar"));
    jar_paths.insert("aquamirae".into(), mods_dir.join("aquamirae-forge-1.20.1-7.1.14.jar"));
    jar_paths.insert("musketmod".into(), mods_dir.join("musketmod-1.20.1-forge-1.5.4.jar"));
    jar_paths.insert("mowziesmobs".into(), mods_dir.join("mowziesmobs-1.8.2.jar"));
    jar_paths.insert("farmersdelight".into(), mods_dir.join("FarmersDelight-1.20.1-1.3.4.jar"));
    if let Some(vj) = vanilla_jar_path() {
        jar_paths.insert("minecraft".into(), vj);
    }

    let mut zips: HashMap<String, ZipArchive<File>> = HashMap::new();
    for (ns, p) in jar_paths {
        if let Some(z) = open_zip(&p) {
            zips.insert(ns, z);
        }
    }

    let mut jar_attrs: HashMap<(String, String), ParsedWeaponAttr> = HashMap::new();
    for ns_attr in ["simplyswords", "cataclysm", "aquamirae"] {
        if let Some(z) = zips.get_mut(ns_attr) {
            for (slug, attr) in collect_jar_weapon_attributes(z, ns_attr) {
                jar_attrs.insert((ns_attr.to_string(), slug), attr);
            }
        }
    }

    let mut seen_ids = HashSet::new();
    let mut records = Vec::with_capacity(manifest.weapons.len());
    for entry in manifest.weapons {
        if !seen_ids.insert(entry.item_id.clone()) {
            continue;
        }
        let (ns, slug) = entry
            .item_id
            .split_once(':')
            .unwrap_or(("minecraft", entry.item_id.as_str()));

        let attr_opt = jar_attrs.get(&(ns.to_string(), slug.to_string()));
        let (cat, inferred_class, inferred_arch) =
            classify_modpack_weapon(ns, slug, attr_opt, &entry.category);

        let use_inferred_shape = entry.category.eq_ignore_ascii_case("otros")
            || matches!(
                cat,
                WeaponCategory::Hachas
                    | WeaponCategory::Escudos
                    | WeaponCategory::Punos
                    | WeaponCategory::Fuego
            );
        let weapon_class = if use_inferred_shape {
            inferred_class.to_string()
        } else {
            entry.weapon_class
        };
        let mesh_archetype = if use_inferred_shape {
            inferred_arch.to_string()
        } else {
            entry.mesh_archetype
        };

        let resolved_sprite = if entry.item_id == "minecraft:air" {
            None
        } else {
            resolve_sprite_from_jar(&mut zips, ns, slug)
        };
        let sprite = resolved_sprite.unwrap_or_else(|| {
            let (w, h, rgba) =
                make_archetype_sprite(&mesh_archetype, tint_for_slug(slug, &mesh_archetype));
            SpritePixels::new(w, h, rgba)
        });

        let bc_archetype = attr_opt.map(derive_bc_archetype).unwrap_or_default();

        let status = profile
            .weapon_status
            .get(&entry.id)
            .copied()
            .unwrap_or(CurationStatus::Pendiente);

        records.push(WeaponRecord {
            id: entry.id,
            name: entry.name,
            mod_source: entry.mod_source,
            item_id: entry.item_id,
            category: cat,
            weapon_class,
            mesh_archetype,
            status,
            bc_archetype,
            icon_rgba: Some(sprite),
        });
    }

    Ok((records, raw_count))
}

fn parse_vec3_json(v: &Value) -> Option<[f32; 3]> {
    if let Some(arr) = v.as_array() {
        if arr.len() >= 3 {
            return Some([
                arr[0].as_f64()? as f32,
                arr[1].as_f64()? as f32,
                arr[2].as_f64()? as f32,
            ]);
        }
    }
    if let Some(obj) = v.as_object() {
        if let Some(vec_val) = obj.get("vector") {
            return parse_vec3_json(vec_val);
        }
    }
    None
}

fn parse_bedrock_channel_points(val: &Value) -> Vec<(f32, [f32; 3], EasingMode)> {
    let mut out = Vec::new();
    if let Some(arr) = val.as_array() {
        if let Some(v) = parse_vec3_json(&Value::Array(arr.clone())) {
            out.push((0.0, v, EasingMode::EaseInOut));
        }
        return out;
    }
    let Some(obj) = val.as_object() else { return out };
    for (k, v) in obj {
        let Ok(t) = k.trim().parse::<f32>() else { continue };
        let easing = v
            .get("easing")
            .and_then(|s| s.as_str())
            .map(|s| match s.to_ascii_lowercase().as_str() {
                "linear" => EasingMode::Linear,
                "step" => EasingMode::Snap,
                _ => EasingMode::EaseInOut,
            })
            .unwrap_or(EasingMode::EaseInOut);
        if let Some(vec3) = parse_vec3_json(v) {
            out.push((t.max(0.0), vec3, easing));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

fn sample_channel_at(pts: &[(f32, [f32; 3], EasingMode)], t: f32, default: [f32; 3]) -> [f32; 3] {
    if pts.is_empty() {
        return default;
    }
    if t <= pts[0].0 {
        return pts[0].1;
    }
    let last = &pts[pts.len() - 1];
    if t >= last.0 {
        return last.1;
    }
    for w in pts.windows(2) {
        if t >= w[0].0 && t <= w[1].0 {
            let span = (w[1].0 - w[0].0).max(1e-5);
            let u = w[0].2.eval((t - w[0].0) / span);
            return [
                w[0].1[0] + (w[1].1[0] - w[0].1[0]) * u,
                w[0].1[1] + (w[1].1[1] - w[0].1[1]) * u,
                w[0].1[2] + (w[1].1[2] - w[0].1[2]) * u,
            ];
        }
    }
    last.1
}

fn prettify_clip_name(id: &str) -> String {
    id.strip_prefix("rellseas_")
        .unwrap_or(id)
        .replace('_', " ")
        .split_whitespace()
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                Some(first) => first.to_uppercase().to_string() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn infer_gallery_group(id: &str, source: ClipSource) -> GalleryGroup {
    let low = id.to_ascii_lowercase();
    if source == ClipSource::CombatRoll
        || low.contains("idle")
        || low.contains("walk")
        || low.contains("run")
        || low.contains("roll")
        || low.contains("dodge")
    {
        GalleryGroup::Movimiento
    } else if low.contains("block") || low.contains("parry") || low.contains("shield") || low.contains("guard") && !low.contains("break") {
        GalleryGroup::Defensa
    } else if low.contains("equip") {
        GalleryGroup::Interaccion
    } else if low.contains("execution")
        || low.contains("geppo")
        || low.contains("guard_break")
        || low.contains("slam")
        || low.contains("spin")
    {
        GalleryGroup::Especiales
    } else {
        GalleryGroup::Combate
    }
}

/// Parse all animations inside a Bedrock 1.8.0 `.animation.json` document.
pub fn parse_bedrock_animation_json(
    raw_json: &str,
    source: ClipSource,
    default_status: CurationStatus,
) -> Result<Vec<ClipDef>, String> {
    let root: Value = serde_json::from_str(raw_json).map_err(|e| format!("Bedrock JSON inválido: {e}"))?;
    let anims = root
        .get("animations")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "Falta objeto `animations` en Bedrock JSON".to_string())?;

    let mut out = Vec::new();
    for (clip_id, aobj) in anims {
        let Some(map) = aobj.as_object() else { continue };
        let looped = map.get("loop").and_then(|v| v.as_bool()).unwrap_or(false);
        let bones_obj = map.get("bones").and_then(|v| v.as_object());

        let mut parsed_bones: HashMap<usize, (Vec<(f32, [f32; 3], EasingMode)>, Vec<(f32, [f32; 3], EasingMode)>, Vec<(f32, [f32; 3], EasingMode)>)> = HashMap::new();
        let mut timestamps_ms: BTreeSet<i32> = BTreeSet::new();
        timestamps_ms.insert(0);

        if let Some(bmap) = bones_obj {
            for (bname, bch) in bmap {
                let Some(bidx) = bone_index_by_name(bname) else { continue };
                let pos_pts = bch
                    .get("position")
                    .map(parse_bedrock_channel_points)
                    .unwrap_or_default();
                let rot_pts = bch
                    .get("rotation")
                    .map(parse_bedrock_channel_points)
                    .unwrap_or_default();
                let scl_pts = bch
                    .get("scale")
                    .map(parse_bedrock_channel_points)
                    .unwrap_or_default();
                for (t, _, _) in pos_pts.iter().chain(rot_pts.iter()).chain(scl_pts.iter()) {
                    timestamps_ms.insert((t * 1000.0).round() as i32);
                }
                parsed_bones.insert(bidx, (pos_pts, rot_pts, scl_pts));
            }
        }

        let max_kf_t = timestamps_ms.iter().next_back().copied().unwrap_or(0) as f32 / 1000.0;
        let duration = map
            .get("animation_length")
            .and_then(|v| v.as_f64())
            .map(|d| d as f32)
            .unwrap_or(max_kf_t)
            .max(0.15);
        timestamps_ms.insert((duration * 1000.0).round() as i32);

        let mut keyframes = Vec::with_capacity(timestamps_ms.len());
        for t_ms in timestamps_ms {
            let t = (t_ms as f32 / 1000.0).clamp(0.0, duration);
            let mut pose = RigPose::ZERO;
            let mut frame_ease = EasingMode::EaseInOut;
            for (bidx, (ppts, rpts, spts)) in &parsed_bones {
                let tr = pose.bone_mut(*bidx);
                tr.pos = sample_channel_at(ppts, t, [0.0, 0.0, 0.0]);
                tr.rot = sample_channel_at(rpts, t, [0.0, 0.0, 0.0]);
                tr.scale = sample_channel_at(spts, t, [1.0, 1.0, 1.0]);
                if let Some((_, _, e)) = rpts.iter().find(|(rt, _, _)| (rt - t).abs() < 0.008) {
                    frame_ease = *e;
                }
            }
            keyframes.push(Keyframe {
                time: t,
                easing: frame_ease,
                bones: pose,
                bone_mask: crate::model::ALL_BONES_MASK,
            });
        }

        let impact_time = if !looped && duration >= 0.20 {
            Some((duration * 0.42).clamp(0.05, duration - 0.04))
        } else {
            None
        };

        out.push(ClipDef {
            id: clip_id.clone(),
            name: prettify_clip_name(clip_id),
            gallery_group: infer_gallery_group(clip_id, source),
            duration,
            impact_time,
            looped,
            bc_targets: Vec::new(),
            event_tag: None,
            source,
            status: default_status,
            vfx_events: Vec::new(),
            allowed_categories: Vec::new(),
            allowed_weapons: Vec::new(),
            keyframes,
        });
    }
    Ok(out)
}

fn starter_vfx_for_rell_clip(id: &str, impact_time: Option<f32>) -> Vec<VfxEvent> {
    let t = impact_time.unwrap_or(0.18);
    match id {
        "rellseas_parry" => vec![
            VfxEvent {
                time: t,
                kind: VfxKind::ParrySpark,
                color: [1.0, 0.85, 0.32],
                scale: 1.25,
                duration: 0.22,
                intensity: 1.0,
                texture_hint: "vfx_parry_star.png".into(),
            },
            VfxEvent {
                time: t,
                kind: VfxKind::HitStop,
                color: [1.0, 0.90, 0.45],
                scale: 1.0,
                duration: 0.07,
                intensity: 1.0,
                texture_hint: String::new(),
            },
        ],
        "rellseas_geppo_kick" | "rellseas_guard_break" => vec![
            VfxEvent {
                time: t,
                kind: VfxKind::SonarRing,
                color: if id.contains("geppo") { [0.22, 0.88, 1.0] } else { [1.0, 0.36, 0.30] },
                scale: 1.35,
                duration: 0.28,
                intensity: 1.0,
                texture_hint: if id.contains("geppo") { "anime_geppo_cyan.png".into() } else { "vfx_pixel_ring.png".into() },
            },
            VfxEvent {
                time: t,
                kind: VfxKind::ScreenShake,
                color: [1.0, 0.5, 0.3],
                scale: 1.0,
                duration: 0.16,
                intensity: 0.8,
                texture_hint: String::new(),
            },
        ],
        "rellseas_heavy_attack" | "rellseas_execution" => vec![
            VfxEvent {
                time: t,
                kind: VfxKind::SlashTrail,
                color: [0.96, 0.42, 0.28],
                scale: 1.3,
                duration: 0.20,
                intensity: 1.0,
                texture_hint: String::new(),
            },
            VfxEvent {
                time: t,
                kind: VfxKind::ScreenShake,
                color: [1.0, 0.4, 0.3],
                scale: 1.0,
                duration: 0.18,
                intensity: 0.85,
                texture_hint: String::new(),
            },
            VfxEvent {
                time: t,
                kind: VfxKind::HitStop,
                color: [1.0, 0.78, 0.3],
                scale: 1.0,
                duration: 0.08,
                intensity: 1.0,
                texture_hint: String::new(),
            },
        ],
        id if id.starts_with("rellseas_attack_") => vec![VfxEvent {
            time: t,
            kind: VfxKind::SlashTrail,
            color: [0.58, 0.45, 1.0],
            scale: 1.0,
            duration: 0.16,
            intensity: 1.0,
            texture_hint: String::new(),
        }],
        _ => Vec::new(),
    }
}

fn rell_event_tag_for_clip(id: &str) -> Option<String> {
    match id {
        "rellseas_block" => Some("MEDIUM:GUARD".into()),
        "rellseas_parry" => Some("MEDIUM:PARRY_SUCCESS".into()),
        "rellseas_heavy_attack" => Some("ANY:STRONG_STRIKE".into()),
        "rellseas_execution" => Some("ANY:EXECUTION".into()),
        "rellseas_guard_break" => Some("ANY:GUARD_BREAK".into()),
        "rellseas_geppo_kick" => Some("ANY:GEPPO_KICK".into()),
        "rellseas_dodge" => Some("ANY:EVADE_DASH".into()),
        _ => None,
    }
}

fn is_punchy_combat_tool_file(path: &str) -> bool {
    const ALLOWED: [&str; 8] = [
        "actions-tool-sword.animation.json",
        "actions-tool-axe.animation.json",
        "actions-tool-mace.animation.json",
        "actions-tool-spear.animation.json",
        "actions-tool-trident.animation.json",
        "actions-tool-bow.animation.json",
        "actions-tool-crossbow.animation.json",
        "actions-tool-shield.animation.json",
    ];
    let fname = path.rsplit('/').next().unwrap_or("");
    ALLOWED.iter().any(|&a| a.eq_ignore_ascii_case(fname))
}

fn should_keep_punchy_clip(clip_id: &str) -> bool {
    let low = clip_id.to_ascii_lowercase();
    if low.contains("_add_") || low.contains("head_movement") || low.contains("mining") || low.contains("overtime") {
        return false;
    }
    low.contains("attack")
        || low.contains("slam")
        || low.contains("lunge")
        || low.contains("charge")
        || low.contains("shoot")
        || low.contains("throw")
        || low.contains("block")
        || low.contains("impact")
        || low.contains("break")
        || low == "spear"
        || low.contains("loop")
}

fn normalize_categories(cats: Vec<WeaponCategory>) -> Vec<WeaponCategory> {
    let mut set = BTreeSet::new();
    for c in cats {
        if !matches!(c, WeaponCategory::Todas | WeaponCategory::Otros) {
            set.insert(c);
        }
    }
    set.into_iter().collect()
}

fn infer_rell_clip_categories(clip_id: &str) -> Vec<WeaponCategory> {
    match clip_id {
        "rellseas_attack_1" | "rellseas_attack_2" | "rellseas_attack_3" => {
            vec![WeaponCategory::Espadas, WeaponCategory::Katanas, WeaponCategory::Dagas]
        }
        "rellseas_heavy_attack" => {
            vec![WeaponCategory::Mandobles, WeaponCategory::Hachas, WeaponCategory::Lanzas]
        }
        "rellseas_block" | "rellseas_parry" | "rellseas_guard_break" | "rellseas_execution" => {
            WeaponCategory::MELEE_AND_SHIELD_FAMILIES.to_vec()
        }
        _ => Vec::new(), // Universal (idle, walk, run, dodge, equip, unequip, geppo_kick)
    }
}

fn infer_punchy_tool_categories(path: &str) -> Vec<WeaponCategory> {
    let fname = path.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    if fname.contains("tool-sword") {
        vec![WeaponCategory::Espadas, WeaponCategory::Katanas]
    } else if fname.contains("tool-axe") {
        vec![WeaponCategory::Hachas, WeaponCategory::Mandobles]
    } else if fname.contains("tool-mace") {
        vec![WeaponCategory::Hachas]
    } else if fname.contains("tool-spear") || fname.contains("tool-trident") {
        vec![WeaponCategory::Lanzas]
    } else if fname.contains("tool-bow") {
        vec![WeaponCategory::Arcos]
    } else if fname.contains("tool-crossbow") {
        vec![WeaponCategory::Arcos, WeaponCategory::Fuego]
    } else if fname.contains("tool-shield") {
        vec![WeaponCategory::Escudos]
    } else {
        Vec::new()
    }
}

fn build_animation_category_index(mods_dir: &Path) -> HashMap<String, BTreeSet<WeaponCategory>> {
    let mut anim_to_cats: HashMap<String, BTreeSet<WeaponCategory>> = HashMap::new();
    let mut record = |anim_id: &str, cats: &[WeaponCategory]| {
        if anim_id.is_empty() {
            return;
        }
        let e = anim_to_cats.entry(anim_id.to_string()).or_default();
        for &c in cats {
            if !matches!(c, WeaponCategory::Todas | WeaponCategory::Otros) {
                e.insert(c);
            }
        }
    };

    let mut bc_templates: HashMap<String, ParsedWeaponAttr> = HashMap::new();
    if let Some(mut bc_zip) = open_zip(&mods_dir.join("bettercombat-forge-1.9.0+1.20.1.jar")) {
        bc_templates = collect_jar_weapon_attributes(&mut bc_zip, "bettercombat");
    }

    // 1. Index Better Combat built-in archetypes (including 1-hop `parent` inheritance).
    for (stem, attr) in &bc_templates {
        let cats = classify_bc_template_categories(stem);
        let mut add_attr = |a: &ParsedWeaponAttr| {
            record(&a.pose, &cats);
            for atk in &a.attacks {
                record(atk, &cats);
            }
        };
        add_attr(attr);
        if let Some(pstem) = attr.parent.strip_prefix("bettercombat:") {
            if let Some(pattr) = bc_templates.get(pstem) {
                add_attr(pattr);
            }
        }
    }

    // 2. Index Simply Swords, Cataclysm, and Aquamirae weapon_attributes into their target animations.
    for (jar_name, ns) in [
        ("simplyswords-forge-neoforge-1.70.2-1.20.1.jar", "simplyswords"),
        ("L_Enders_Cataclysm-3.31.jar", "cataclysm"),
        ("aquamirae-forge-1.20.1-7.1.14.jar", "aquamirae"),
    ] {
        if let Some(mut zip) = open_zip(&mods_dir.join(jar_name)) {
            let attrs = collect_jar_weapon_attributes(&mut zip, ns);
            for (slug, attr) in attrs {
                let (cat, _, _) = classify_modpack_weapon(ns, &slug, Some(&attr), "");
                let cats = [cat];
                record(&attr.pose, &cats);
                for atk in &attr.attacks {
                    record(atk, &cats);
                }
                if let Some(pstem) = attr.parent.strip_prefix("bettercombat:") {
                    if let Some(pattr) = bc_templates.get(pstem) {
                        record(&pattr.pose, &cats);
                        for atk in &pattr.attacks {
                            record(atk, &cats);
                        }
                    }
                }
            }
        }
    }

    anim_to_cats
}

fn infer_bc_clip_categories(
    stem: &str,
    anim_to_cats: &HashMap<String, BTreeSet<WeaponCategory>>,
) -> Vec<WeaponCategory> {
    let key = format!("bettercombat:{stem}");
    let mut set: BTreeSet<WeaponCategory> = anim_to_cats.get(&key).cloned().unwrap_or_default();

    // Explicit pose / firearm / shield / fist guarantees.
    match stem {
        "pose_two_handed_katana" => {
            set.insert(WeaponCategory::Katanas);
        }
        "pose_two_handed_bow" => {
            set.insert(WeaponCategory::Arcos);
        }
        "pose_two_handed_crossbow" => {
            set.insert(WeaponCategory::Arcos);
            set.insert(WeaponCategory::Fuego);
        }
        "pose_two_handed_heavy" => {
            set.insert(WeaponCategory::Mandobles);
            set.insert(WeaponCategory::Hachas);
        }
        "pose_two_handed_polearm" | "pose_two_handed_scythe" => {
            set.insert(WeaponCategory::Lanzas);
        }
        "pose_two_handed_sword" => {
            set.insert(WeaponCategory::Espadas);
            set.insert(WeaponCategory::Mandobles);
        }
        "pose_one_handed_backwards" => {
            set.insert(WeaponCategory::Dagas);
            set.insert(WeaponCategory::Espadas);
        }
        "one_handed_punch" => {
            set.insert(WeaponCategory::Punos);
            set.insert(WeaponCategory::Escudos);
        }
        "one_handed_swipe_horizontal_right" => {
            set.insert(WeaponCategory::Punos);
            set.insert(WeaponCategory::Dagas);
        }
        _ => {}
    }

    if set.is_empty() {
        if stem.starts_with("two_handed_") {
            set.extend([
                WeaponCategory::Mandobles,
                WeaponCategory::Hachas,
                WeaponCategory::Lanzas,
            ]);
        } else if stem.starts_with("dual_handed_") {
            set.extend([WeaponCategory::Dagas, WeaponCategory::Espadas]);
        } else {
            set.extend([WeaponCategory::Espadas, WeaponCategory::Dagas]);
        }
    }
    set.into_iter().collect()
}

fn infer_ss_clip_categories(
    stem: &str,
    anim_to_cats: &HashMap<String, BTreeSet<WeaponCategory>>,
) -> Vec<WeaponCategory> {
    let key = format!("simplyswords:{stem}");
    if let Some(set) = anim_to_cats.get(&key) {
        if !set.is_empty() {
            return set.iter().copied().collect();
        }
    }
    let low = stem.to_ascii_lowercase();
    if low.contains("chakram") {
        vec![WeaponCategory::Dagas, WeaponCategory::Hachas]
    } else if low.contains("halberd")
        || low.contains("polearm")
        || low.contains("warglaive")
        || low.contains("soulrender")
    {
        vec![WeaponCategory::Lanzas]
    } else if low.contains("greatsword")
        || low.contains("dragable")
        || low.contains("whirlwind")
        || low.contains("twilight")
    {
        vec![WeaponCategory::Mandobles]
    } else if low.contains("greataxe") || low.contains("slam_reversed") {
        vec![WeaponCategory::Mandobles, WeaponCategory::Hachas]
    } else if low.contains("estoc") {
        vec![WeaponCategory::Espadas, WeaponCategory::Dagas]
    } else {
        vec![WeaponCategory::Espadas]
    }
}

/// Import all 5 combat animation families from `servernuevo`, strictly excluding ParCool and Punchy Moves locomotion.
pub fn load_all_combat_animations(server_root: &Path, profile: &LocalProfile) -> Vec<ClipDef> {
    let mut clips: Vec<ClipDef> = Vec::new();
    let mut seen_ids = HashSet::new();

    let mods_dir = server_root.join("sklauncher_modpack").join("mods");
    let rp_dir = server_root.join("sklauncher_modpack").join("resourcepacks");
    let combat_dir = server_root
        .join("src")
        .join("main")
        .join("resources")
        .join("assets")
        .join("rellseas")
        .join("animations")
        .join("combat");

    let anim_to_cats = build_animation_category_index(&mods_dir);

    // Preload Better Combat mapping from active RELL_Seas_Combat_Animations compat pack.
    let mut bc_by_rell_clip: HashMap<String, Vec<String>> = HashMap::new();
    let compat_path = rp_dir
        .join("RELL_Seas_Combat_Animations")
        .join("assets")
        .join("punchy_moves")
        .join("compat")
        .join("rellseas_combat.json");
    if let Ok(compat_raw) = std::fs::read_to_string(&compat_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&compat_raw) {
            if let Some(anims) = val
                .get("defaults")
                .and_then(|d| d.get("better_combat"))
                .and_then(|b| b.get("animations"))
                .and_then(|a| a.as_object())
            {
                for (bc_id, cfg) in anims {
                    if let Some(target_clip) = cfg.get("clip").and_then(|v| v.as_str()) {
                        bc_by_rell_clip
                            .entry(target_clip.to_string())
                            .or_default()
                            .push(bc_id.clone());
                    }
                }
            }
        }
    }

    // Load timing overrides from rellseas_combat_timings.json
    let mut rell_timings: HashMap<String, (f32, Option<f32>, bool)> = HashMap::new();
    let timings_path = combat_dir.join("rellseas_combat_timings.json");
    if let Ok(traw) = std::fs::read_to_string(&timings_path) {
        if let Ok(tval) = serde_json::from_str::<Value>(&traw) {
            if let Some(events) = tval.get("events").and_then(|v| v.as_object()) {
                for (cid, cfg) in events {
                    let dur = cfg.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                    let imp = cfg.get("impact_time").and_then(|v| v.as_f64()).map(|x| x as f32);
                    let lp = cfg.get("loop").and_then(|v| v.as_bool()).unwrap_or(false);
                    rell_timings.insert(cid.clone(), (dur, imp, lp));
                }
            }
        }
    }

    // 1. RELL Seas native clips (`15` clips) — Approved baseline unless user curated differently.
    let rs_anim_path = combat_dir.join("rellseas_combat.animation.json");
    if let Ok(raw) = std::fs::read_to_string(&rs_anim_path) {
        if let Ok(parsed) = parse_bedrock_animation_json(&raw, ClipSource::RellSeas, CurationStatus::Aprobada) {
            for mut c in parsed {
                if let Some((dur, imp, lp)) = rell_timings.get(&c.id) {
                    c.duration = (*dur).max(0.15);
                    c.impact_time = *imp;
                    c.looped = *lp;
                }
                c.event_tag = rell_event_tag_for_clip(&c.id);
                c.allowed_categories = normalize_categories(infer_rell_clip_categories(&c.id));
                if let Some(targets) = bc_by_rell_clip.get(&c.id) {
                    c.bc_targets = targets.clone();
                }
                c.vfx_events = starter_vfx_for_rell_clip(&c.id, c.impact_time);
                if seen_ids.insert(c.id.clone()) {
                    clips.push(c);
                }
            }
        }
    }

    // 2. Better Combat (`30` PlayerAnimator attack animations).
    if let Some(mut bc_zip) = open_zip(&mods_dir.join("bettercombat-forge-1.9.0+1.20.1.jar")) {
        let entries = sorted_zip_entries(&mut bc_zip, |n| {
            n.starts_with("assets/bettercombat/attack_animations/") && n.ends_with(".json")
        });
        for entry in entries {
            let Some(raw) = read_zip_string(&mut bc_zip, &entry) else { continue };
            let stem = entry.rsplit('/').next().unwrap_or("").trim_end_matches(".json");
            let cid = format!("bc_{stem}");
            let bc_target = format!("bettercombat:{stem}");
            if let Ok(mut c) = parse_player_animator_json(&raw, &cid, "Better Combat", vec![bc_target]) {
                c.source = ClipSource::BetterCombat;
                c.status = CurationStatus::Pendiente;
                c.allowed_categories = normalize_categories(infer_bc_clip_categories(stem, &anim_to_cats));
                if let Some(imp) = c.impact_time {
                    c.vfx_events.push(VfxEvent {
                        time: imp,
                        kind: VfxKind::SlashTrail,
                        color: [0.58, 0.45, 1.0],
                        scale: 1.0,
                        duration: 0.16,
                        intensity: 1.0,
                        texture_hint: String::new(),
                    });
                }
                if seen_ids.insert(c.id.clone()) {
                    clips.push(c);
                }
            }
        }
    }

    // 3. Simply Swords (`34` PlayerAnimator attack animations).
    if let Some(mut ss_zip) = open_zip(&mods_dir.join("simplyswords-forge-neoforge-1.70.2-1.20.1.jar")) {
        let entries = sorted_zip_entries(&mut ss_zip, |n| {
            n.starts_with("assets/simplyswords/attack_animations/") && n.ends_with(".json")
        });
        for entry in entries {
            let Some(raw) = read_zip_string(&mut ss_zip, &entry) else { continue };
            let stem = entry.rsplit('/').next().unwrap_or("").trim_end_matches(".json");
            let cid = format!("ss_{stem}");
            let bc_target = format!("simplyswords:{stem}");
            if let Ok(mut c) = parse_player_animator_json(&raw, &cid, "Simply Swords", vec![bc_target]) {
                c.source = ClipSource::SimplySwords;
                c.status = CurationStatus::Pendiente;
                c.allowed_categories = normalize_categories(infer_ss_clip_categories(stem, &anim_to_cats));
                if let Some(imp) = c.impact_time {
                    c.vfx_events.push(VfxEvent {
                        time: imp,
                        kind: VfxKind::SlashTrail,
                        color: [0.95, 0.62, 0.25],
                        scale: 1.05,
                        duration: 0.16,
                        intensity: 1.0,
                        texture_hint: String::new(),
                    });
                }
                if seen_ids.insert(c.id.clone()) {
                    clips.push(c);
                }
            }
        }
    }

    // 4. Combat Roll (`1` animation).
    if let Some(mut cr_zip) = open_zip(&mods_dir.join("combatroll-forge-1.3.3+1.20.1.jar")) {
        if let Some(raw) = read_zip_string(&mut cr_zip, "assets/combatroll/rolling_animations/roll.json") {
            if let Ok(mut c) = parse_player_animator_json(&raw, "cr_roll", "Combat Roll", vec!["combatroll:roll".into()]) {
                c.source = ClipSource::CombatRoll;
                c.status = CurationStatus::Pendiente;
                c.gallery_group = GalleryGroup::Movimiento;
                c.event_tag = Some("ANY:EVADE_DASH".into());
                if seen_ids.insert(c.id.clone()) {
                    clips.push(c);
                }
            }
        }
    }

    // 5. Punchy Moves / Fluid & Foolish (STRICTLY combat tool animations only — never locomotion or ParCool).
    if let Some(mut ff_zip) = open_zip(&rp_dir.join("Fluid & Foolish 1.0.1.zip")) {
        let entries = sorted_zip_entries(&mut ff_zip, is_punchy_combat_tool_file);
        for entry in entries {
            let Some(raw) = read_zip_string(&mut ff_zip, &entry) else { continue };
            let tool_cats = normalize_categories(infer_punchy_tool_categories(&entry));
            if let Ok(parsed) = parse_bedrock_animation_json(&raw, ClipSource::FluidFoolish, CurationStatus::Pendiente) {
                for mut c in parsed {
                    if !should_keep_punchy_clip(&c.id) {
                        continue;
                    }
                    let orig_id = c.id.clone();
                    c.id = format!("pm_{orig_id}");
                    c.name = prettify_clip_name(&orig_id);
                    c.allowed_categories = tool_cats.clone();
                    if seen_ids.insert(c.id.clone()) {
                        clips.push(c);
                    }
                }
            }
        }
    }

    // Apply persisted user modifications, curation statuses, impact times, timeline VFX, and allowed objects.
    for clip in &mut clips {
        let inferred_cats = clip.allowed_categories.clone();
        if let Some(edited) = profile.clip_modified_clips.get(&clip.id) {
            *clip = edited.clone();
        } else {
            if let Some(st) = profile.clip_status.get(&clip.id) {
                clip.status = *st;
            }
            if let Some(imp) = profile.clip_impact_overrides.get(&clip.id) {
                clip.impact_time = Some((*imp).clamp(0.02, (clip.duration - 0.02).max(0.03)));
            }
            if let Some(vfx) = profile.clip_vfx.get(&clip.id) {
                clip.vfx_events = vfx.clone();
            }
        }

        if let Some(saved_cats) = profile.clip_allowed_categories.get(&clip.id) {
            clip.allowed_categories = normalize_categories(saved_cats.clone());
        } else if clip.allowed_categories.is_empty()
            && clip.allowed_weapons.is_empty()
            && !inferred_cats.is_empty()
        {
            // Backwards-compat for legacy profile.local.json edits saved before allowed_categories existed.
            clip.allowed_categories = inferred_cats;
        }
        if let Some(saved_weapons) = profile.clip_allowed_weapons.get(&clip.id) {
            clip.allowed_weapons = saved_weapons.clone();
        }
        clip.allowed_categories = normalize_categories(clip.allowed_categories.clone());
    }

    // Include any purely custom clips saved in `profile.clip_modified_clips` that aren't in the 5 mod sources.
    let mut extra_custom: Vec<_> = profile
        .clip_modified_clips
        .values()
        .filter(|c| !seen_ids.contains(&c.id))
        .cloned()
        .collect();
    extra_custom.sort_by(|a, b| a.id.cmp(&b.id));
    clips.extend(extra_custom);

    clips
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bedrock_animation_vector_and_array_keyframes() {
        let raw = r#"{
            "format_version": "1.8.0",
            "animations": {
                "rell_test_slash": {
                    "animation_length": 0.5,
                    "loop": false,
                    "bones": {
                        "waist": {
                            "rotation": {
                                "0.0": [0.0, 0.0, 0.0],
                                "0.25": { "vector": [12.0, -40.0, 0.0], "easing": "linear" }
                            }
                        },
                        "right_item": {
                            "scale": {
                                "0.25": [1.0, 1.0, 1.4]
                            }
                        }
                    }
                }
            }
        }"#;
        let clips = parse_bedrock_animation_json(raw, ClipSource::RellSeas, CurationStatus::Aprobada).unwrap();
        assert_eq!(clips.len(), 1);
        let c = &clips[0];
        assert_eq!(c.id, "rell_test_slash");
        assert!((c.duration - 0.5).abs() < 1e-4);
        let k25 = c.keyframes.iter().find(|k| (k.time - 0.25).abs() < 0.01).unwrap();
        assert!((k25.bones.bone(0).rot[1] - (-40.0)).abs() < 0.1);
        assert!((k25.bones.bone(5).scale[2] - 1.4).abs() < 0.05);
    }

    #[test]
    fn enforces_strict_punchy_moves_combat_only_allowlist() {
        assert!(is_punchy_combat_tool_file("assets/punchy_moves/animations/creator/actions-tool-sword.animation.json"));
        assert!(!is_punchy_combat_tool_file("assets/punchy_moves/animations/creator/actions-tool-hoe.animation.json"));
        assert!(!is_punchy_combat_tool_file("assets/punchy_moves/animations/creator/player-walk.animation.json"));
        assert!(should_keep_punchy_clip("sword_attack_1"));
        assert!(!should_keep_punchy_clip("axe_mining_right"));
        assert!(!should_keep_punchy_clip("attack_1_add_right"));
    }

    #[test]
    fn loads_local_servernuevo_profile_counts_when_present_readonly() {
        let ws = crate::app_state::detect_workspace_root();
        let profile = LocalProfile::default();
        let Some(sr) = discover_servernuevo_root(&ws, &profile) else {
            return;
        };
        let (weapons, raw_count) = load_curated_weapons(&sr, &profile).unwrap();
        assert_eq!(raw_count, 146);
        assert_eq!(weapons.len(), 139);
        assert!(weapons.iter().all(|w| w.icon_rgba.is_some()));

        let clips = load_all_combat_animations(&sr, &profile);
        let c_rell = clips.iter().filter(|c| c.source == ClipSource::RellSeas).count();
        let c_bc = clips.iter().filter(|c| c.source == ClipSource::BetterCombat).count();
        let c_ss = clips.iter().filter(|c| c.source == ClipSource::SimplySwords).count();
        let c_cr = clips.iter().filter(|c| c.source == ClipSource::CombatRoll).count();
        let c_ff = clips.iter().filter(|c| c.source == ClipSource::FluidFoolish).count();
        eprintln!(
            "MODPACK_SUMMARY: manifest={raw_count} unique_weapons={} clips_total={} rell={c_rell} bc={c_bc} ss={c_ss} roll={c_cr} ff={c_ff}",
            weapons.len(),
            clips.len()
        );
        assert_eq!(c_rell, 15);
        assert_eq!(c_bc, 30);
        assert!(c_ss >= 33);
        assert_eq!(c_cr, 1);
        assert!(c_ff >= 20);
    }

    #[test]
    fn classifies_all_139_curated_weapons_into_10_families_with_zero_otros_and_valid_compat_mapping() {
        let ws = crate::app_state::detect_workspace_root();
        let profile = LocalProfile::default();
        let Some(sr) = discover_servernuevo_root(&ws, &profile) else {
            return;
        };
        let (weapons, _) = load_curated_weapons(&sr, &profile).unwrap();
        let clips = load_all_combat_animations(&sr, &profile);

        assert!(
            weapons.iter().all(|w| w.category != WeaponCategory::Otros),
            "Zero curated weapons should remain in Otros"
        );

        let by_id = |id: &str| {
            weapons
                .iter()
                .find(|w| w.id == id || w.item_id == id)
                .unwrap_or_else(|| panic!("Missing weapon {id}"))
        };

        for id in [
            "musketmod:pistol",
            "musketmod:musket",
            "musketmod:blunderbuss",
            "cataclysm:laser_gatling",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Fuego, "{id} should be Fuego");
        }
        for id in ["cataclysm:black_steel_targe", "cataclysm:bulwark_of_the_flame"] {
            assert_eq!(by_id(id).category, WeaponCategory::Escudos, "{id} should be Escudos");
        }
        for id in [
            "cataclysm:gauntlet_of_bulwark",
            "cataclysm:gauntlet_of_guard",
            "cataclysm:gauntlet_of_maelstrom",
            "cataclysm:tidal_claws",
            "mowziesmobs:earthrend_gauntlet",
            "rellseas:haki_fists",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Punos, "{id} should be Punos");
        }
        for id in [
            "cataclysm:black_steel_axe",
            "cataclysm:infernal_forge",
            "cataclysm:void_forge",
            "cataclysm:the_annihilator",
            "cataclysm:ceraunus",
            "simplyswords:mjolnir",
            "simplyswords:frostfall",
            "minecraft:netherite_axe",
            "aquamirae:shatterblade",
            "mowziesmobs:wrought_axe",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Hachas, "{id} should be Hachas");
        }
        for id in [
            "cataclysm:the_incinerator",
            "cataclysm:meat_shredder",
            "simplyswords:twilight",
            "simplyswords:awakened_lichblade",
            "aquamirae:terrible_cleaver",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Mandobles, "{id} should be Mandobles");
        }
        for id in [
            "cataclysm:ancient_spear",
            "cataclysm:astrape",
            "cataclysm:coral_bardiche",
            "cataclysm:coral_spear",
            "cataclysm:soul_render",
            "simplyswords:arcanethyst",
            "simplyswords:soulrender",
            "aquamirae:shatterspike",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Lanzas, "{id} should be Lanzas");
        }
        for id in [
            "simplyswords:whisperwind",
            "simplyswords:dreadwhisper",
            "aquamirae:terrible_blade",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Katanas, "{id} should be Katanas");
        }
        for id in [
            "cataclysm:athame",
            "simplyswords:tempest",
            "aquamirae:dagger_of_greed",
            "farmersdelight:netherite_knife",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Dagas, "{id} should be Dagas");
        }
        for id in [
            "cataclysm:black_steel_sword",
            "simplyswords:diamond_cutlass",
            "aquamirae:remnant_saber",
            "minecraft:netherite_sword",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Espadas, "{id} should be Espadas");
        }
        for id in [
            "cataclysm:cursed_bow",
            "minecraft:bow",
            "minecraft:crossbow",
            "mowziesmobs:blowgun",
        ] {
            assert_eq!(by_id(id).category, WeaponCategory::Arcos, "{id} should be Arcos");
        }

        // Every weapon must have >= 1 specific (non-universal) compatible combat clip.
        for w in &weapons {
            let specific_cnt = clips
                .iter()
                .filter(|c| !c.is_universal() && c.allows_weapon(w))
                .count();
            assert!(
                specific_cnt >= 1,
                "Weapon {} ({:?}) has 0 specific compatible clips!",
                w.id,
                w.category
            );
        }

        // Verify strict incompatibility guard across key cross-family combinations.
        let clip_by_id = |cid: &str| {
            clips
                .iter()
                .find(|c| c.id == cid)
                .unwrap_or_else(|| panic!("Missing clip {cid}"))
        };

        let pistol = by_id("musketmod:pistol");
        assert!(!clip_by_id("bc_pose_two_handed_katana").allows_weapon(pistol));
        assert!(!clip_by_id("rellseas_attack_1").allows_weapon(pistol));
        assert!(clip_by_id("bc_pose_two_handed_crossbow").allows_weapon(pistol));
        assert!(clip_by_id("pm_crossbow_shoot_right").allows_weapon(pistol));

        let targe = by_id("cataclysm:black_steel_targe");
        assert!(!clip_by_id("bc_pose_two_handed_bow").allows_weapon(targe));
        assert!(clip_by_id("bc_one_handed_punch").allows_weapon(targe));
        assert!(clips.iter().any(|c| c.id.starts_with("pm_shield") && c.allows_weapon(targe)));

        let gauntlet = by_id("cataclysm:gauntlet_of_guard");
        assert!(!clip_by_id("pm_bow_shoot_right").allows_weapon(gauntlet));
        assert!(clip_by_id("bc_one_handed_punch").allows_weapon(gauntlet));

        let katana = by_id("rellseas:kozuki_katana");
        assert!(!clip_by_id("pm_crossbow_shoot_right").allows_weapon(katana));
        assert!(clip_by_id("rellseas_attack_1").allows_weapon(katana));
        assert!(clip_by_id("bc_pose_two_handed_katana").allows_weapon(katana));
    }
}
