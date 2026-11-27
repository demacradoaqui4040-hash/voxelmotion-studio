// VoxelMotion Studio - Procedural Starter Weapons & Runtime Asset Importer (.png / .jar / .zip)

use crate::model::{SpritePixels, WeaponCategory, WeaponRecord};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use zip::ZipArchive;

/// Generate a crisp 32x32 pixel-art Katana sprite procedurally.
fn make_classic_katana_sprite() -> (u32, u32, Vec<u8>) {
    let w = 32usize;
    let mut px = vec![0u8; w * w * 4];
    let mut set = |x: i32, y: i32, rgba: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < w {
            let i = ((y as usize) * w + (x as usize)) * 4;
            px[i..i + 4].copy_from_slice(&rgba);
        }
    };

    for k in 0..7 {
        let x = 4 + k;
        let y = 27 - k;
        let diamond = k % 2 == 1;
        set(x, y, if diamond { [62, 52, 84, 255] } else { [28, 26, 38, 255] });
        set(x + 1, y, [42, 38, 56, 255]);
        set(x, y - 1, [20, 18, 28, 255]);
    }
    set(3, 28, [224, 176, 62, 255]);
    set(4, 28, [186, 138, 42, 255]);
    set(3, 29, [228, 232, 240, 255]);
    set(3, 30, [138, 92, 246, 255]);
    set(3, 31, [109, 40, 217, 255]);

    for dx in -2i32..=2 {
        for dy in -2i32..=2 {
            if (dx + dy).abs() <= 2 && (dx.abs() + dy.abs() <= 2) {
                let c = if dx == 0 && dy == 0 {
                    [246, 206, 92, 255]
                } else {
                    [204, 154, 48, 255]
                };
                set(11 + dx, 20 + dy, c);
            }
        }
    }

    for s in 1..=18 {
        let curve = if s > 11 { -1 } else { 0 };
        let x = 11 + s;
        let y = 20 - s + curve;
        set(x - 1, y - 1, [38, 44, 58, 255]);
        set(x, y - 1, [74, 84, 102, 255]);
        set(x, y, [152, 164, 184, 255]);
        set(x + 1, y, [242, 246, 255, 255]);
        set(x + 1, y - 1, [224, 232, 246, 255]);
    }
    (w as u32, w as u32, px)
}

/// Generate clean 16x16 pixel-art fallback sprite for any weapon archetype.
pub fn make_archetype_sprite(archetype: &str, tint: [u8; 3]) -> (u32, u32, Vec<u8>) {
    let w = 16usize;
    let mut px = vec![0u8; w * w * 4];
    let mut set = |x: i32, y: i32, rgba: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < w {
            let i = ((y as usize) * w + (x as usize)) * 4;
            px[i..i + 4].copy_from_slice(&rgba);
        }
    };
    let dark = [(tint[0] as f32 * 0.55) as u8, (tint[1] as f32 * 0.55) as u8, (tint[2] as f32 * 0.55) as u8, 255];
    let bright = [
        (tint[0] as u16 + 48).min(255) as u8,
        (tint[1] as u16 + 48).min(255) as u8,
        (tint[2] as u16 + 48).min(255) as u8,
        255,
    ];
    let hilt = [76, 52, 40, 255];
    let gold = [228, 182, 68, 255];

    match archetype {
        "arc" | "bow" => {
            for i in 2..=13 {
                let x = match i {
                    2 | 13 => 5,
                    3 | 12 => 7,
                    4 | 11 => 9,
                    5..=10 => 10,
                    _ => 8,
                };
                set(x, i, hilt);
                set(5, i, [215, 222, 232, 255]);
            }
        }
        "ranged" | "musket" => {
            for x in 2..=6 {
                set(x, 11, hilt);
                set(x, 10, hilt);
            }
            for x in 5..=14 {
                set(x, 8, dark);
                set(x, 9, [196, 206, 218, 255]);
            }
            set(9, 9, gold);
            set(13, 9, gold);
        }
        "heavy" | "claymore" => {
            for k in 0..4 {
                set(2 + k, 13 - k, hilt);
            }
            for d in -2..=2 {
                set(5 + d, 10 + d, gold);
            }
            for k in 1..=9 {
                set(5 + k, 10 - k, [tint[0], tint[1], tint[2], 255]);
                set(6 + k, 10 - k, bright);
                set(5 + k, 9 - k, dark);
            }
        }
        "polearm" | "spear" => {
            for k in 0..10 {
                set(2 + k, 13 - k, hilt);
            }
            for k in 10..=13 {
                set(2 + k, 13 - k, bright);
                set(3 + k, 13 - k, dark);
            }
            set(12, 5, [tint[0], tint[1], tint[2], 255]);
        }
        "dagger" => {
            set(4, 11, hilt);
            set(5, 10, hilt);
            set(5, 9, gold);
            set(7, 11, gold);
            for k in 1..=5 {
                set(6 + k, 10 - k, bright);
                set(5 + k, 9 - k, dark);
            }
        }
        _ => {
            for k in 0..3 {
                set(2 + k, 13 - k, hilt);
            }
            set(4, 10, gold);
            set(6, 12, gold);
            set(5, 11, gold);
            for k in 1..=9 {
                set(5 + k, 11 - k, [tint[0], tint[1], tint[2], 255]);
                set(6 + k, 11 - k, bright);
            }
        }
    }
    (w as u32, w as u32, px)
}

fn prettify_slug(slug: &str) -> String {
    slug.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut ch = w.chars();
            match ch.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn classify_weapon(slug: &str, parent: &str) -> (WeaponCategory, &'static str, &'static str) {
    let s = slug.to_lowercase();
    let p = parent.to_lowercase();
    if s.contains("musket")
        || s.contains("pistol")
        || s.contains("blunderbuss")
        || s.contains("flintlock")
        || s.contains("rifle")
        || s.contains("gatling")
    {
        (WeaponCategory::Fuego, "RANGED", "ranged")
    } else if s.contains("bow") || s.contains("crossbow") || s.contains("blowgun") {
        (WeaponCategory::Arcos, "RANGED", "arc")
    } else if s.contains("shield") || s.contains("targe") || s.contains("bulwark_of_the_flame") {
        (WeaponCategory::Escudos, "HEAVY", "heavy")
    } else if s.contains("gauntlet")
        || s.contains("fist")
        || s.contains("tidal_claws")
        || s.contains("haki_fists")
        || p.contains("fist")
    {
        (WeaponCategory::Punos, "UNARMED", "dagger")
    } else if s.contains("katana") || p.contains("katana") {
        (WeaponCategory::Katanas, "MEDIUM", "katana")
    } else if s.contains("greataxe")
        || s.contains("greathammer")
        || s.contains("hammer")
        || s.contains("mace")
        || s.contains("axe")
        || s.contains("anchor")
        || s.contains("forge")
        || s.contains("annihilator")
        || s.contains("ceraunus")
        || s.contains("mjolnir")
        || s.contains("frostfall")
        || p.contains("heavy_axe")
        || p.contains("double_axe")
        || p.contains("hammer")
        || p.contains("mace")
        || p.contains("anchor")
    {
        (WeaponCategory::Hachas, "HEAVY", "heavy")
    } else if s.contains("claymore")
        || s.contains("greatsword")
        || s.contains("cleaver")
        || s.contains("incinerator")
        || s.contains("shredder")
        || p.contains("claymore")
    {
        (WeaponCategory::Mandobles, "HEAVY", "heavy")
    } else if s.contains("spear")
        || s.contains("halberd")
        || s.contains("glaive")
        || s.contains("lance")
        || s.contains("trident")
        || s.contains("bardiche")
        || s.contains("scythe")
        || s.contains("battlestaff")
        || s.contains("warglaive")
        || s.contains("staff")
        || s.contains("twin_blade")
        || p.contains("spear")
        || p.contains("halberd")
        || p.contains("glaive")
        || p.contains("trident")
        || p.contains("scythe")
        || p.contains("battlestaff")
    {
        (WeaponCategory::Lanzas, "HEAVY", "polearm")
    } else if s.contains("dagger")
        || s.contains("knife")
        || s.contains("sai")
        || s.contains("rapier")
        || s.contains("athame")
        || s.contains("sickle")
        || s.contains("chakram")
        || s.contains("estoc")
        || s.contains("claw")
        || p.contains("dagger")
        || p.contains("rapier")
        || p.contains("soul_knife")
        || p.contains("sickle")
    {
        (WeaponCategory::Dagas, "LIGHT", "dagger")
    } else if s.contains("sword")
        || s.contains("cutlass")
        || s.contains("blade")
        || s.contains("saber")
        || p.contains("sword")
        || p.contains("cutlass")
        || p.contains("coral_blade")
    {
        (WeaponCategory::Espadas, "MEDIUM", "sword")
    } else {
        (WeaponCategory::Espadas, "MEDIUM", "sword")
    }
}

pub fn decode_png_top_frame(bytes: &[u8]) -> Option<Arc<SpritePixels>> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let frame_h = w.min(h).max(1);
    let mut raw = img.into_raw();
    raw.truncate((w * frame_h * 4) as usize);
    Some(SpritePixels::new(w, frame_h, raw))
}

fn sprite_arc((w, h, rgba): (u32, u32, Vec<u8>)) -> Arc<SpritePixels> {
    SpritePixels::new(w, h, rgba)
}

/// Pure-procedural starter voxel weapon library requiring zero external mod files.
pub fn build_starter_weapons() -> Vec<WeaponRecord> {
    vec![
        WeaponRecord {
            id: "starter:steel_sword".into(),
            name: "Espada de Acero".into(),
            mod_source: "VoxelMotion".into(),
            item_id: "minecraft:iron_sword".into(),
            category: WeaponCategory::Espadas,
            weapon_class: "MEDIUM".into(),
            mesh_archetype: "sword".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("sword", [175, 205, 232]))),
        },
        WeaponRecord {
            id: "starter:classic_katana".into(),
            name: "Katana Clásica".into(),
            mod_source: "VoxelMotion".into(),
            item_id: "voxelmotion:classic_katana".into(),
            category: WeaponCategory::Katanas,
            weapon_class: "MEDIUM".into(),
            mesh_archetype: "katana".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_classic_katana_sprite())),
        },
        WeaponRecord {
            id: "starter:heavy_claymore".into(),
            name: "Mandoble Pesado".into(),
            mod_source: "VoxelMotion".into(),
            item_id: "voxelmotion:heavy_claymore".into(),
            category: WeaponCategory::Mandobles,
            weapon_class: "HEAVY".into(),
            mesh_archetype: "heavy".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("heavy", [148, 128, 210]))),
        },
        WeaponRecord {
            id: "starter:battle_halberd".into(),
            name: "Lanza / Alabarda".into(),
            mod_source: "VoxelMotion".into(),
            item_id: "voxelmotion:battle_halberd".into(),
            category: WeaponCategory::Lanzas,
            weapon_class: "HEAVY".into(),
            mesh_archetype: "polearm".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("polearm", [198, 210, 224]))),
        },
        WeaponRecord {
            id: "starter:shadow_dagger".into(),
            name: "Daga Ligera".into(),
            mod_source: "VoxelMotion".into(),
            item_id: "voxelmotion:shadow_dagger".into(),
            category: WeaponCategory::Dagas,
            weapon_class: "LIGHT".into(),
            mesh_archetype: "dagger".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("dagger", [185, 225, 238]))),
        },
        WeaponRecord {
            id: "starter:flintlock".into(),
            name: "Mosquete Clásico".into(),
            mod_source: "VoxelMotion".into(),
            item_id: "voxelmotion:flintlock".into(),
            category: WeaponCategory::Fuego,
            weapon_class: "RANGED".into(),
            mesh_archetype: "ranged".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("ranged", [160, 175, 195]))),
        },
        WeaponRecord {
            id: "starter:hunter_bow".into(),
            name: "Arco de Caza".into(),
            mod_source: "Minecraft".into(),
            item_id: "minecraft:bow".into(),
            category: WeaponCategory::Arcos,
            weapon_class: "RANGED".into(),
            mesh_archetype: "arc".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("arc", [190, 145, 92]))),
        },
        WeaponRecord {
            id: "starter:unarmed".into(),
            name: "Sin Arma (Rig Base)".into(),
            mod_source: "Rig Base".into(),
            item_id: "minecraft:air".into(),
            category: WeaponCategory::Punos,
            weapon_class: "UNARMED".into(),
            mesh_archetype: "dagger".into(),
            status: crate::model::CurationStatus::Pendiente,
            bc_archetype: String::new(),
            icon_rgba: Some(sprite_arc(make_archetype_sprite("dagger", [210, 140, 180]))),
        },
    ]
}

/// Import custom sprites (.png) or Minecraft mod/resourcepack archives (.jar / .zip) chosen by the user.
pub fn import_weapons_from_paths(paths: &[PathBuf]) -> Vec<WeaponRecord> {
    let mut out = Vec::new();
    let mut seen_ids = HashSet::new();

    for path in paths {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if ext == "png" {
            let Ok(bytes) = std::fs::read(path) else { continue };
            let Some(sprite) = decode_png_top_frame(&bytes) else { continue };
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("custom_item");
            let slug = stem.to_ascii_lowercase().replace(' ', "_");
            let id = format!("custom:{slug}");
            if seen_ids.contains(&id) {
                continue;
            }
            seen_ids.insert(id.clone());
            let (cat, wclass, arch) = classify_weapon(&slug, "");
            out.push(WeaponRecord {
                id: id.clone(),
                name: prettify_slug(stem),
                mod_source: "PNG Local".into(),
                item_id: id,
                category: cat,
                weapon_class: wclass.into(),
                mesh_archetype: arch.into(),
                status: crate::model::CurationStatus::Pendiente,
                bc_archetype: String::new(),
                icon_rgba: Some(sprite),
            });
        } else if ext == "jar" || ext == "zip" {
            let Ok(file) = File::open(path) else { continue };
            let Ok(mut zip) = ZipArchive::new(file) else { continue };
            let source_label = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(prettify_slug)
                .unwrap_or_else(|| "Archivo JAR".into());

            let mut texture_paths = Vec::new();
            let mut parent_map = HashMap::<String, String>::new();

            for i in 0..zip.len() {
                if let Ok(mut f) = zip.by_index(i) {
                    let name = f.name().to_string();
                    if (name.contains("/textures/item/") || name.contains("/textures/items/"))
                        && name.ends_with(".png")
                        && !name.contains("_layer")
                        && !name.contains("empty_slot")
                        && !name.contains("upgrade")
                    {
                        texture_paths.push(name);
                    } else if name.contains("weapon_attributes/") && name.ends_with(".json") {
                        let mut buf = String::new();
                        if f.read_to_string(&mut buf).is_ok() {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&buf) {
                                let slug = name.rsplit('/').next().unwrap_or("").trim_end_matches(".json");
                                let parent = val.get("parent").and_then(|v| v.as_str()).unwrap_or("");
                                parent_map.insert(slug.to_string(), parent.to_string());
                            }
                        }
                    }
                }
            }

            for tex_path in texture_paths {
                let parts: Vec<&str> = tex_path.split('/').collect();
                if parts.len() < 4 {
                    continue;
                }
                let ns = if parts[0] == "assets" && parts.len() >= 5 {
                    parts[1]
                } else {
                    "imported"
                };
                let filename = parts[parts.len() - 1];
                let slug = filename.trim_end_matches(".png");
                if slug.contains("cartridge") {
                    continue;
                }
                let full_id = format!("{ns}:{slug}");
                if seen_ids.contains(&full_id) {
                    continue;
                }
                seen_ids.insert(full_id.clone());

                let parent = parent_map.get(slug).map(String::as_str).unwrap_or("");
                let (cat, wclass, arch) = classify_weapon(slug, parent);
                let icon_rgba = zip.by_name(&tex_path).ok().and_then(|mut entry| {
                    let mut bytes = Vec::new();
                    entry.read_to_end(&mut bytes).ok()?;
                    decode_png_top_frame(&bytes)
                });
                out.push(WeaponRecord {
                    id: full_id.clone(),
                    name: prettify_slug(slug),
                    mod_source: source_label.clone(),
                    item_id: full_id,
                    category: cat,
                    weapon_class: wclass.into(),
                    mesh_archetype: arch.into(),
                    status: crate::model::CurationStatus::Pendiente,
                    bc_archetype: parent.to_string(),
                    icon_rgba,
                });
            }
        }
    }

    out
}
