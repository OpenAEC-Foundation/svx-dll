//! A Voyager scene of the Smithsonian's open access 3D collection, read.
use std::path::Path;

#[test]
fn dress_with_annotations() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/dress");
    let json = std::fs::read(dir.join("scene.svx.json")).unwrap();
    let scene = svx_read::read_preferring(&json, &["Thumb"], &mut |uri| std::fs::read(dir.join(uri)).map_err(|e| e.to_string())).unwrap();
    assert_eq!(scene.models.len(), 1);
    let model = &scene.models[0];
    assert_eq!(model.uri, "cs_264155_001-20k-thumb.glb");
    assert!(model.glb.starts_with(b"glTF"));
    assert_eq!(scene.annotations.len(), 8);
    assert_eq!(scene.annotations[0].title, "Overall garment");
    assert!(scene.annotations[0].lead.starts_with("Mrs. Nathan B. Scott"));
    assert!(scene.info.iter().any(|(k, v)| k == "Title" && v == "Flapper Dress"));
    // the dress is about 1.4 m tall; its annotations lie within a metre of the scene's origin
    for a in &scene.annotations {
        let r = (a.position[0].powi(2) + a.position[1].powi(2) + a.position[2].powi(2)).sqrt();
        assert!(r < 1.0, "{a:?}");
    }
}
