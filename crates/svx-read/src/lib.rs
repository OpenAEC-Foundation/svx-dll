//! Reader of Voyager scene documents (`.svx.json`, the JSON scene format of the Smithsonian's
//! Voyager viewer): a tree of nodes with translation, rotation and scale; the nodes with a model
//! place a glTF file chosen among the model's derivatives (qualities), with the model's units
//! and its pose; models carry annotations (a position and direction on the model, a title and a
//! lead text).
//!
//! The scene is Y up, in the units of the scene; models are in their own units and are scaled
//! to the scene's. The reader gives everything in metres, Z up.

use serde_json::Value;
use svx_model::{Annotation, Model, Scene};

type Mat4 = [f64; 16];
const IDENTITY: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
/// Y up to Z up: x, y, z to x, -z, y.
const Y_TO_Z: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];
const Z_TO_Y: Mat4 = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// The qualities of a model's derivatives, the preferred first.
pub const QUALITIES: [&str; 6] = ["High", "Medium", "Highest", "Low", "AR", "Thumb"];

/// The length of a unit of the document in metres.
pub fn unit(name: &str) -> f64 {
    match name {
        "mm" => 0.001,
        "cm" => 0.01,
        "m" => 1.0,
        "km" => 1000.0,
        "in" => 0.0254,
        "ft" => 0.3048,
        "yd" => 0.9144,
        "mi" => 1609.344,
        _ => 0.01,
    }
}

/// Reads a scene document; `read` reads a file the document names (relative URI).
pub fn read(json: &[u8], read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>) -> Result<Scene, String> {
    read_preferring(json, &QUALITIES, read)
}

/// [`read`], choosing each model's glTF by the qualities given (the first there is wins; a model
/// with none of them gets its best by [`QUALITIES`]).
pub fn read_preferring(json: &[u8], qualities: &[&str], read: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>) -> Result<Scene, String> {
    let doc: Value = serde_json::from_slice(json.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(json)).map_err(|e| format!("Voyager scene: {e}"))?;
    if !doc["asset"]["type"].as_str().is_some_and(|t| t.contains("si-dpo-3d")) && !doc["models"].is_array() {
        return Err("Voyager scene: not a Voyager document".into());
    }
    let scene_index = doc["scene"].as_u64().unwrap_or(0) as usize;
    let scene = &doc["scenes"][scene_index];
    let setup = &doc["setups"][scene["setup"].as_u64().unwrap_or(0) as usize];
    let scene_unit = unit(scene["units"].as_str().or(setup["units"].as_str()).unwrap_or("cm"));

    let mut out = Scene::default();
    for (key, label) in [("version", "Voyager document version"), ("generator", "Generator"), ("copyright", "Copyright")] {
        if let Some(v) = doc["asset"][key].as_str() {
            out.info.push((label.into(), v.into()));
        }
    }
    let meta = &doc["metas"][scene["meta"].as_u64().unwrap_or(0) as usize];
    if let Some(title) = text(&meta["collection"], "title") {
        out.info.push(("Title".into(), title));
    }
    if let Some(record) = meta["collection"]["edanRecordId"].as_str() {
        out.info.push(("Collection record".into(), record.into()));
    }

    // The node tree, from the scene's root nodes (all nodes without a parent when it has none).
    let roots: Vec<u64> = match scene["nodes"].as_array() {
        Some(r) => r.iter().filter_map(Value::as_u64).collect(),
        None => (0..doc["nodes"].as_array().map_or(0, Vec::len) as u64).collect(),
    };
    let mut stack: Vec<(u64, Mat4)> = roots.into_iter().map(|n| (n, IDENTITY)).collect();
    let mut seen = 0;
    while let Some((index, parent)) = stack.pop() {
        seen += 1;
        if seen > 100_000 {
            break;
        }
        let node = &doc["nodes"][index as usize];
        let world = mul(&parent, &trs(node));
        if let Some(m) = node["model"].as_u64() {
            let model = &doc["models"][m as usize];
            // the model's own units into the scene's; annotations are written in the model's
            // frame after its pose, the glTF before it
            let units = mul(&world, &scale(unit(model["units"].as_str().unwrap_or("cm")) / scene_unit));
            let to_scene = mul(&units, &trs(model));
            // in metres, Z up; the glTF's own frame comes in Z up, so turned back first
            let placement = mul(&mul(&mul(&Y_TO_Z, &scale(scene_unit)), &to_scene), &Z_TO_Y);
            let name = node["name"].as_str().map(str::to_owned).unwrap_or_else(|| format!("Model {m}"));
            let Some((uri, quality)) = choose(model, qualities).or_else(|| choose(model, &QUALITIES)) else {
                out.info.push((format!("{name}: not shown"), "no web glTF derivative".into()));
                continue;
            };
            let glb = read(&uri).map_err(|e| format!("Voyager scene: {uri}: {e}"))?;
            let model_index = out.models.len();
            for a in model["annotations"].as_array().into_iter().flatten() {
                let p = vec3(&a["position"]).unwrap_or([0.0; 3]);
                let d = vec3(&a["direction"]).unwrap_or([0.0, 1.0, 0.0]);
                let world_m = mul(&Y_TO_Z, &mul(&scale(scene_unit), &units));
                let direction = normalize(vector(&world_m, d));
                out.annotations.push(Annotation {
                    id: a["id"].as_str().unwrap_or("").to_owned(),
                    title: text(a, "title").unwrap_or_default(),
                    lead: text(a, "lead").unwrap_or_default(),
                    model: model_index,
                    position: point(&world_m, p),
                    direction,
                });
            }
            out.models.push(Model { name, uri, quality, glb, placement });
        }
        for child in node["children"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
            stack.push((child, world));
        }
    }
    if out.models.is_empty() {
        return Err("Voyager scene: no model to show".into());
    }
    out.models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The glTF the viewer shows of a model: a web 3D derivative of the most preferred quality.
fn choose(model: &Value, qualities: &[&str]) -> Option<(String, String)> {
    let derivatives = model["derivatives"].as_array()?;
    let gltf = |d: &&Value| {
        let usage = d["usage"].as_str().unwrap_or("Web3D");
        (usage == "Web3D" || usage == "App3D") && d["assets"].as_array().is_some_and(|a| a.iter().any(is_gltf))
    };
    let candidates: Vec<&Value> = derivatives.iter().filter(gltf).collect();
    let rank = |d: &&Value| qualities.iter().position(|q| d["quality"].as_str() == Some(q));
    let best = candidates.into_iter().filter(|d| rank(d).is_some()).min_by_key(|d| (rank(d), d["usage"].as_str() != Some("Web3D")))?;
    let asset = best["assets"].as_array()?.iter().find(|a| is_gltf(a))?;
    Some((asset["uri"].as_str()?.to_owned(), best["quality"].as_str().unwrap_or("").to_owned()))
}

fn is_gltf(asset: &Value) -> bool {
    let uri = asset["uri"].as_str().unwrap_or("").to_ascii_lowercase();
    asset["type"].as_str().is_none_or(|t| t == "Model") && (uri.ends_with(".glb") || uri.ends_with(".gltf"))
}

/// A text of the document: the 2.0 form by language (`titles: {"EN": ..}`, English first) or
/// the 1.0 form (`title`).
fn text(v: &Value, key: &str) -> Option<String> {
    let many = &v[format!("{key}s")];
    if let Some(map) = many.as_object() {
        if let Some(t) = map.get("EN").and_then(Value::as_str).or_else(|| map.values().find_map(Value::as_str)) {
            return Some(t.to_owned());
        }
    }
    v[key].as_str().map(str::to_owned)
}

fn vec3(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?])
}

/// The matrix of `translation`, `rotation` (quaternion x, y, z, w) and `scale`.
fn trs(v: &Value) -> Mat4 {
    let t = vec3(&v["translation"]).unwrap_or([0.0; 3]);
    let s = vec3(&v["scale"]).unwrap_or([1.0; 3]);
    let q = v["rotation"].as_array().filter(|a| a.len() == 4).map(|a| [0, 1, 2, 3].map(|i| a[i].as_f64().unwrap_or(0.0))).unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let n = (q.iter().map(|v| v * v).sum::<f64>()).sqrt();
    let [x, y, z, w] = if n > 0.0 { q.map(|v| v / n) } else { [0.0, 0.0, 0.0, 1.0] };
    let r = [
        1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + z * w), 2.0 * (x * z - y * w),
        2.0 * (x * y - z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + x * w),
        2.0 * (x * z + y * w), 2.0 * (y * z - x * w), 1.0 - 2.0 * (x * x + y * y),
    ];
    [r[0] * s[0], r[1] * s[0], r[2] * s[0], 0.0, r[3] * s[1], r[4] * s[1], r[5] * s[1], 0.0, r[6] * s[2], r[7] * s[2], r[8] * s[2], 0.0, t[0], t[1], t[2], 1.0]
}

fn scale(s: f64) -> Mat4 {
    [s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

fn point(m: &Mat4, p: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r])
}

fn vector(m: &Mat4, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2])
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 { v.map(|c| c / l) } else { [0.0, 0.0, 1.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{
        "asset": {"type": "application/si-dpo-3d.document+json", "version": "1.0", "generator": "Voyager"},
        "scene": 0,
        "scenes": [{"units": "mm", "nodes": [0]}],
        "nodes": [{"name": "Group", "translation": [0, 100, 0], "children": [1]}, {"name": "Vase", "model": 0}],
        "models": [{
            "units": "m",
            "translation": [0, 0, 1],
            "derivatives": [
                {"usage": "Web3D", "quality": "Thumb", "assets": [{"uri": "thumb.glb", "type": "Model"}]},
                {"usage": "Web3D", "quality": "Medium", "assets": [{"uri": "medium.glb", "type": "Model"}]},
                {"usage": "Image2D", "quality": "High", "assets": [{"uri": "picture.jpg", "type": "Image"}]}
            ],
            "annotations": [{"id": "a1", "titles": {"EN": "Handle"}, "leads": {"EN": "Made by hand."}, "position": [0, 0.5, 0], "direction": [0, 1, 0]}]
        }]
    }"#;

    #[test]
    fn places_models_in_metres_z_up() {
        let mut asked = Vec::new();
        let scene = read(DOC.as_bytes(), &mut |uri| {
            asked.push(uri.to_owned());
            Ok(b"glTF".to_vec())
        })
        .unwrap();
        assert_eq!(asked, ["medium.glb"]);
        let m = &scene.models[0];
        assert_eq!((m.name.as_str(), m.quality.as_str()), ("Vase", "Medium"));
        // the glTF's origin: model pose (0, 0, 1 m) plus the group's 100 mm up (Y), in Z up:
        // x, y, z = 0, -1, 0.1
        let o = [m.placement[12], m.placement[13], m.placement[14]];
        assert!((o[0]).abs() < 1e-12 && (o[1] + 1.0).abs() < 1e-12 && (o[2] - 0.1).abs() < 1e-12, "{o:?}");
        // a glTF point 1 m up (Z up after the turn) stays 1 m up: models in metres, scene in mm
        assert!((m.placement[10] - 1.0).abs() < 1e-12);
        let a = &scene.annotations[0];
        assert_eq!((a.title.as_str(), a.lead.as_str()), ("Handle", "Made by hand."));
        // 0.5 m up on the model: z = 0.1 + 0.5
        assert!((a.position[2] - 0.6).abs() < 1e-12 && (a.direction[2] - 1.0).abs() < 1e-12, "{:?}", a.position);
    }

    #[test]
    fn units() {
        assert_eq!(unit("mm"), 0.001);
        assert_eq!(unit("in"), 0.0254);
    }
}
