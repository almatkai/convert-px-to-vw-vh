// Test-only OS boundary. Production uses the real Tauri AppHandle/Emitter.
#[derive(Clone)]
pub struct AppHandle;
pub trait Emitter {
    fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) -> Result<(), String>;
}
impl Emitter for AppHandle {
    fn emit<S: serde::Serialize + Clone>(&self, _: &str, _: S) -> Result<(), String> {
        Err("Tauri desktop events are not exercised by the fixture".into())
    }
}
