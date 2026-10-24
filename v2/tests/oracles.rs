use serde_json::Value;
use tezbar_convert_px_to_vw_vh::{model, numeric, presets};

#[test]
fn pinned_original_ts_commands_form_presets_and_history_oracle() {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/source-oracle.json")).unwrap();
    for row in oracle["rows"].as_array().unwrap() {
        let c = row["command"].as_str().unwrap();
        let result = model::convert(
            c,
            row["viewport"].as_str().unwrap(),
            row["pixels"].as_str().unwrap(),
        );
        if row["accepted"] == false {
            assert!(result.is_err(), "{row}");
            continue;
        }
        let mut result = result.unwrap_or_else(|e| panic!("{e}: {row}"));
        if c == "index" && row["property"] == "height" {
            result.value = result.value.strip_suffix("vw").unwrap().to_string() + "vh";
            result.label = result.label.strip_suffix("vw").unwrap().to_string() + "vh";
        }
        assert_eq!(result.value, row["value"].as_str().unwrap(), "{row}");
        assert_eq!(result.label, row["label"].as_str().unwrap(), "{row}");
        assert_eq!(result.copy, row["copy"].as_bool().unwrap(), "{row}");
    }
    for devices in oracle["devices"].as_array().unwrap() {
        let expected = devices["options"].as_array().unwrap();
        let actual = presets::for_property(devices["property"].as_str().unwrap()).unwrap();
        assert_eq!(actual.len(), 11);
        for (p, e) in actual.iter().zip(expected) {
            assert_eq!(p.value, e["value"]);
            assert_eq!(p.title, e["title"]);
            assert_eq!(p.group, e["group"]);
        }
    }
    assert_eq!(oracle["history"]["duplicateCount"], 10);
    assert_eq!(oracle["history"]["removeIsNoop"], true);
}

#[test]
fn binary64_matches_large_javascript_fixed_and_shortest_string_vector() {
    let rows: Value = serde_json::from_str(include_str!("fixtures/fixed-oracle.json")).unwrap();
    for r in rows.as_array().unwrap() {
        let bits = u64::from_str_radix(r["bits"].as_str().unwrap(), 16).unwrap();
        let x = f64::from_bits(bits);
        assert_eq!(
            numeric::to_fixed_3(x),
            r["fixed"].as_str().unwrap(),
            "bits={bits:016x}, x={x}"
        );
        assert_eq!(
            numeric::js_string(x),
            r["string"].as_str().unwrap(),
            "bits={bits:016x}, x={x}"
        );
    }
}
