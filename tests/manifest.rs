//! The schema-3 manifest must mirror the preserved Raycast package exactly,
//! except for the documented menu-bar List adaptation.
use serde_json::{json, Value};
use std::path::Path;

fn load(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn manifest_mirrors_original_package_commands_and_identity() {
    let manifest = load("extension.json");
    let original = load("package.json");
    assert_eq!(manifest["schemaVersion"], 3);
    assert_eq!(manifest["runtime"], "rust");
    assert_eq!(manifest["execution"], "process");
    assert_eq!(manifest["protocolVersion"], 1);
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
    let slug = original["name"].as_str().unwrap();
    assert_eq!(manifest["slug"], slug);
    assert_eq!(manifest["id"], format!("com.tezbar.native.{slug}"));
    assert_eq!(manifest["replaces"], json!([format!("raycast.{slug}")]));
    assert_eq!(manifest["name"], original["title"]);
    assert_eq!(
        manifest["capabilities"],
        json!(["clipboard.write", "window.visibility"])
    );
    assert_eq!(manifest["preferences"], json!([]));
    assert!(
        original.get("preferences").is_none(),
        "original declares no preferences"
    );
    let ported = manifest["commands"].as_array().unwrap();
    let source = original["commands"].as_array().unwrap();
    assert_eq!(ported.len(), source.len());
    for (p, s) in ported.iter().zip(source) {
        for key in ["name", "title", "arguments"] {
            assert_eq!(p.get(key), s.get(key), "{key}");
        }
        assert_eq!(p["platforms"], s["platforms"]);
        assert_eq!(p["runtime"], "rust");
        if s["mode"] == "menu-bar" {
            assert_eq!(p["mode"], "view");
            assert!(p["gap"].as_str().unwrap().contains("no OS menu-bar"));
        } else {
            assert_eq!(p["mode"], s["mode"]);
            assert_eq!(p["description"], s["description"]);
            assert!(p.get("gap").is_none());
        }
        if let Some(icon) = s["icon"].as_str() {
            let packaged = p["icon"].as_str().unwrap();
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            assert_eq!(
                std::fs::read(root.join(packaged)).unwrap(),
                std::fs::read(root.join(icon)).unwrap()
            );
        }
    }
    let names: Vec<_> = ported.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["index", "height", "width", "menu-bar"]);
}
