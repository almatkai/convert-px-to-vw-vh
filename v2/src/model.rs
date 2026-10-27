//! Validated native storage. Call mutations only inside the SDK transaction closure.
use crate::numeric::{js_string, number, parse_int, to_fixed_3};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversion {
    pub value: String,
    pub label: String,
    pub copy: bool,
}

pub fn convert(command: &str, viewport: &str, pixels: &str) -> Result<Conversion, String> {
    if !matches!(command, "index" | "width" | "height") {
        return Err("Unknown conversion command".into());
    }
    let form = command == "index";
    let p = parse_int(pixels);
    let v = parse_int(viewport);
    if p.is_nan() || v.is_nan() {
        return Err("Viewport and pixels must be integers".into());
    }
    if !(form || v > 0.0 && p > 0.0) {
        return Err("Viewport and pixels must be greater than 0".into());
    }
    if form && v == 0.0 {
        return Err("Select a valid device".into());
    }
    let (p, viewport_label, pixels_label, denominator) = if form {
        (parse_int(&js_string(p)), js_string(v), js_string(p), v)
    } else {
        (
            p,
            viewport.to_string(),
            pixels.to_string(),
            number(viewport),
        )
    };
    let result = (p / denominator) * 100.0;
    let fixed = to_fixed_3(result);
    let unit = if command == "height" { "vh" } else { "vw" };
    Ok(Conversion {
        value: format!("{fixed}{unit}"),
        label: format!("{viewport_label}px / {pixels_label}px → {fixed}{unit}"),
        copy: form || !result.is_nan(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub date: String,
    pub value: String,
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    pub revision: u64,
    pub entries: Vec<Entry>,
}
impl History {
    pub fn read(store: &Value) -> Result<Self, String> {
        if !store.is_object() {
            return Err("Native store must be an object".into());
        }
        if let Some(draft) = store.get("textfield") {
            if !draft.is_string() {
                return Err("Invalid saved pixel field".into());
            }
        }
        let revision = match store.get("historyRevision") {
            Some(v) => v.as_u64().ok_or("Invalid history revision")?,
            None if store.get("history").is_none() => 0,
            None => return Err("History has no revision; refusing to overwrite".into()),
        };
        let entries: Vec<Entry> = match store.get("history") {
            Some(v) => serde_json::from_value(v.clone()).map_err(|_| "Invalid history entries")?,
            None if revision == 0 => vec![],
            None => return Err("History missing for saved revision".into()),
        };
        if entries.len() > 10 {
            return Err("History exceeds ten entries".into());
        }
        let mut ids = HashSet::new();
        for e in &entries {
            let formatted = e
                .value
                .strip_suffix("vw")
                .or_else(|| e.value.strip_suffix("vh"))
                .ok_or("Invalid history unit")?;
            if formatted != "-0.000" && to_fixed_3(number(formatted)) != formatted {
                return Err("Invalid history conversion value".into());
            }
            if !e.label.ends_with(&format!(" → {}", e.value)) || !e.label.contains("px / ") {
                return Err("Invalid history conversion label".into());
            }
            if e.id == 0
                || e.id > revision
                || !ids.insert(e.id)
                || e.label.is_empty()
                || !(e.value.ends_with("vw") || e.value.ends_with("vh"))
                || time::OffsetDateTime::parse(
                    &e.date,
                    &time::format_description::well_known::Rfc3339,
                )
                .is_err()
            {
                return Err("Invalid history entry".into());
            }
        }
        if entries.windows(2).any(|w| w[0].id <= w[1].id) {
            return Err("History is not newest first".into());
        }
        Ok(Self { revision, entries })
    }
    fn advance(&mut self) -> Result<(), String> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or("History revision exhausted")?;
        Ok(())
    }
    fn save(&self, store: &mut Value) {
        store["historyRevision"] = json!(self.revision);
        store["history"] = json!(self.entries);
    }
    pub fn add(store: &mut Value, value: &Conversion, date: String) -> Result<(), String> {
        let mut history = Self::read(store)?;
        history.advance()?;
        history.entries.insert(
            0,
            Entry {
                id: history.revision,
                date,
                value: value.value.clone(),
                label: value.label.clone(),
            },
        );
        history.entries.truncate(10);
        history.save(store);
        Ok(())
    }
    pub fn change(store: &mut Value, expected: u64, remove: Option<u64>) -> Result<(), String> {
        let mut history = Self::read(store)?;
        history.ensure_revision(expected)?;
        if let Some(id) = remove {
            if !history.entries.iter().any(|e| e.id == id) {
                return Err("History entry no longer exists".into());
            }
            history.entries.retain(|e| e.id != id);
        } else {
            history.entries.clear();
        }
        history.advance()?;
        history.save(store);
        Ok(())
    }
    pub fn ensure_revision(&self, expected: u64) -> Result<(), String> {
        if self.revision != expected {
            Err("History changed; refresh before acting".into())
        } else {
            Ok(())
        }
    }
}

/// ISO 8601 UTC timestamp with the same millisecond precision as Date#toISOString.
pub fn timestamp() -> Result<String, String> {
    let format = time::format_description::parse(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z",
    )
    .map_err(|e| e.to_string())?;
    time::OffsetDateTime::now_utc()
        .format(&format)
        .map_err(|e| e.to_string())
}
