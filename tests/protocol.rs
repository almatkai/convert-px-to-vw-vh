//! Real-process JSONL protocol tests for the built executable. The test plays
//! the host: it validates hello, initializes private temporary paths, answers
//! only typed `host.*` reverse requests with recorded stub results and never
//! touches the desktop clipboard or windows.
use serde_json::{json, Value};
use std::{collections::VecDeque, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

const BIN: &str = env!("CARGO_BIN_EXE_tezbar-convert-px-to-vw-vh");
const ID: &str = "com.tezbar.native.convert-px-to-vw-vh";
const GRANTS: &[&str] = &[
    "clipboard.write",
    "window.visibility",
    "tray.set",
    "tray.remove",
    "background.retain",
    "background.release",
    "settings.open",
];

struct Peer {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    /// Every reverse request received, in order.
    calls: Vec<Value>,
    /// Scripted robustness/failure probes. Default replies match each current host method.
    replies: VecDeque<Result<Value, String>>,
    sequence: u64,
}

impl Peer {
    async fn start(binary: &Path, cwd: &Path) -> Self {
        let mut command = Command::new(binary);
        command.current_dir(cwd).env_clear();
        for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let system_path = if cfg!(windows) {
            Path::new(&std::env::var_os("SystemRoot").unwrap_or_else(|| "C:/Windows".into()))
                .join("System32")
                .into_os_string()
        } else {
            "/usr/bin:/bin".into()
        };
        let mut child = command
            .env("PATH", system_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            calls: vec![],
            replies: VecDeque::new(),
            sequence: 1,
        }
    }
    async fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.input
            .as_mut()
            .unwrap()
            .write_all(&bytes)
            .await
            .unwrap();
        self.input.as_mut().unwrap().flush().await.unwrap();
    }
    async fn frame(&mut self) -> Value {
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(10), self.output.read_line(&mut line))
            .await
            .expect("protocol timeout")
            .unwrap();
        assert!(n > 0, "unexpected EOF");
        assert!(line.ends_with('\n'), "LF framing");
        serde_json::from_str(&line).unwrap()
    }
    /// Sends a host request and returns `Ok(result)` or `Err(error)`.
    async fn try_request(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = format!("h:00000000-0000-4000-8000-{:012x}", self.sequence);
        self.sequence += 1;
        self.send(json!({"type":"request","id":id,"method":method,"params":params}))
            .await;
        loop {
            let frame = self.frame().await;
            match frame["type"].as_str().unwrap() {
                "response" if frame["id"] == id.as_str() => {
                    return match frame.get("error") {
                        Some(error) => {
                            if method == "background.event" {
                                // Current Process::background_event calls fail on every Err.
                                self.child.kill().await.unwrap();
                                panic!("fatal background callback terminated process: {error}");
                            }
                            assert!(error["code"].is_string() && error["message"].is_string());
                            Err(error.clone())
                        }
                        None => Ok(frame["result"].clone()),
                    };
                }
                "request" => {
                    let method = frame["method"].as_str().unwrap();
                    assert!(frame["id"].as_str().unwrap().starts_with("e:"));
                    assert!(
                        [
                            "host.clipboard.write",
                            "host.window.visibility",
                            "host.tray.set",
                            "host.tray.remove",
                            "host.background.retain",
                            "host.background.release",
                            "host.settings.open"
                        ]
                        .contains(&method),
                        "unexpected host method {method}"
                    );
                    self.calls.push(frame.clone());
                    let reply = match self.replies.pop_front().unwrap_or_else(|| {
                        Ok(match method {
                            "host.clipboard.write" | "host.settings.open" => json!({"ok":true}),
                            "host.window.visibility" => Value::Null,
                            "host.tray.set" | "host.tray.remove" => json!(true),
                            "host.background.retain" => {
                                json!({"retained":true,"key":frame["params"]["key"]})
                            }
                            "host.background.release" => {
                                json!({"retained":false,"key":frame["params"]["key"]})
                            }
                            _ => panic!("unrecognized host method {method}"),
                        })
                    }) {
                        Ok(result) => json!({"type":"response","id":frame["id"],"result":result}),
                        Err(message) => {
                            json!({"type":"response","id":frame["id"],"error":{"code":"host-error","message":message}})
                        }
                    };
                    self.send(reply).await;
                }
                "event" | "cancel" => {}
                other => panic!("unexpected frame {other}: {frame}"),
            }
        }
    }
    async fn request(&mut self, method: &str, params: Value) -> Value {
        self.try_request(method, params)
            .await
            .unwrap_or_else(|e| panic!("{method}: {e}"))
    }
    async fn initialize(&mut self, dir: &Path, grants: &[&str]) {
        self.initialize_paths(dir, grants, json!({})).await.unwrap();
    }
    async fn initialize_paths(
        &mut self,
        dir: &Path,
        grants: &[&str],
        mut paths: Value,
    ) -> Result<Value, Value> {
        for kind in ["Flat", "Managed", "Registry"] {
            if paths.get(format!("jsCache{kind}")).is_some()
                && paths.get(format!("jsPackage{kind}")).is_none()
            {
                let manifest = dir.join(format!("package-{kind}.json"));
                std::fs::write(&manifest, b"{}").unwrap();
                paths[format!("jsPackage{kind}")] = json!(manifest);
            }
        }
        let hello = self.frame().await;
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["id"], ID);
        assert_eq!(hello["protocol"], 1);
        assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
        let ready = self
            .try_request(
                "initialize",
                json!({"protocol":1,"extensionId":ID,"platform":if cfg!(windows) {"x86_64-pc-windows-msvc"} else {"aarch64-apple-darwin"},
                    "packageDir":env!("CARGO_MANIFEST_DIR"),"dataDir":dir.join("data"),
                    "tempDir":dir.join("temp"),"grantedCapabilities":grants,
                    "reason":"command","migrationPaths":paths}),
            )
            .await?;
        assert_eq!(ready["ready"], true);
        Ok(ready)
    }
    async fn open(&mut self, session: &str, command: &str, args: Value) -> Result<Value, Value> {
        self.try_request(
            "session.open",
            json!({"sessionId":session,"commandName":command,"preferences":{},"argumentValues":args}),
        )
        .await
    }
    async fn act(&mut self, session: &str, payload: Value) -> Result<Value, Value> {
        self.try_request(
            "session.call",
            json!({"sessionId":session,"operation":"extension:invoke-action","payload":payload}),
        )
        .await
    }
    async fn dispose(&mut self, session: &str) {
        self.request("session.dispose", json!({"sessionId":session}))
            .await;
    }
    fn methods(&self) -> Vec<(String, Value)> {
        self.calls
            .iter()
            .filter(|c| {
                ["host.clipboard.write", "host.window.visibility"]
                    .contains(&c["method"].as_str().unwrap())
            })
            .map(|c| {
                (
                    c["method"].as_str().unwrap().to_owned(),
                    c["params"].clone(),
                )
            })
            .collect()
    }
    async fn shutdown(mut self) {
        self.request("shutdown", json!({})).await;
        let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());
    }
}

fn binary() -> std::path::PathBuf {
    std::env::var_os("TEZBAR_EXTENSION_BINARY")
        .map(Into::into)
        .unwrap_or_else(|| BIN.into())
}

fn find<'a>(node: &'a Value, kind: &str, id: Option<&str>) -> Option<&'a Value> {
    if node["type"] == kind && id.is_none_or(|id| node["props"]["id"] == id) {
        return Some(node);
    }
    node["children"]
        .as_array()?
        .iter()
        .find_map(|c| find(c, kind, id))
}

fn history_rows(view: &Value) -> Vec<Value> {
    view["root"]["children"][0]["children"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn submit(view: &Value) -> String {
    let actions = view["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0]["kind"], "submit-form");
    assert_eq!(actions[0]["title"], "Convert and Copy");
    actions[0]["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn protocol_info_lists_all_original_commands() {
    let out = Command::new(binary())
        .arg("--protocol-info")
        .output()
        .await
        .unwrap();
    assert!(out.status.success());
    let info: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(info["protocol"], 1);
    assert_eq!(info["id"], ID);
    assert_eq!(info["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        info["commands"],
        json!(["index", "width", "height", "menu-bar"])
    );
}

#[tokio::test]
async fn form_conversion_history_copy_remove_and_clear_over_real_pipes() {
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(dir.path(), GRANTS).await;

    let view = peer.open("form", "index", json!({})).await.unwrap();
    assert_eq!(view["mode"], "view");
    assert_eq!(view["title"], "Convert PX to VW or VH");
    let property = find(&view["root"], "Form.Dropdown", Some("sizedropdown")).unwrap();
    assert_eq!(property["props"]["value"], "width");
    let device = find(&view["root"], "Form.Dropdown", Some("dropdown")).unwrap();
    assert_eq!(device["props"]["value"], "1440");
    let sections = device["children"].as_array().unwrap();
    assert_eq!(
        sections
            .iter()
            .map(|s| s["props"]["title"].clone())
            .collect::<Vec<_>>(),
        ["Desktop", "Tablet", "Mobile"]
    );
    assert_eq!(
        sections
            .iter()
            .map(|s| s["children"].as_array().unwrap().len())
            .sum::<usize>(),
        11
    );
    let field = find(&view["root"], "Form.TextField", Some("textfield"))
        .unwrap()
        .clone();
    assert_eq!(field["props"]["value"], "");
    let text_action = field["props"]["actionId"].as_str().unwrap().to_owned();
    let property_action = property["props"]["actionId"].as_str().unwrap().to_owned();
    let submit_id = submit(&view);

    // Typing persists the draft and leaves the view unchanged.
    let typed = peer
        .act(
            "form",
            json!({"actionId":text_action,"formValues":{"value":"16"}}),
        )
        .await
        .unwrap();
    renderer_consumes(&view, &typed, &json!(text_action), None);
    assert_eq!(
        typed["mode"], "view",
        "current renderer needs a complete view"
    );
    assert_eq!(typed["root"]["type"], "Form");
    assert!(
        find(&typed["root"], "Form.TextField", None).unwrap()["props"]
            .get("value")
            .is_none(),
        "preserve queued local typing"
    );
    // Invalid input: visible field error, no host effect.
    let invalid = peer
        .act("form", json!({"actionId":submit_id,"formValues":{"sizedropdown":"width","dropdown":"1440","textfield":"abc"}}))
        .await
        .unwrap();
    assert_eq!(
        find(&invalid["root"], "Form.TextField", None).unwrap()["props"]["error"],
        "Viewport and pixels must be integers"
    );
    assert!(peer.calls.is_empty());
    // Switching to height resets the device and expires the old submit ID.
    let height = peer
        .act(
            "form",
            json!({"actionId":property_action,"formValues":{"value":"height"}}),
        )
        .await
        .unwrap();
    assert_eq!(
        find(&height["root"], "Form.Dropdown", Some("dropdown")).unwrap()["props"]["value"],
        "1024"
    );
    let stale = peer
        .act("form", json!({"actionId":submit_id,"formValues":{"sizedropdown":"height","dropdown":"1024","textfield":"16"}}))
        .await
        .unwrap_err();
    assert!(
        stale["message"].as_str().unwrap().contains("stale"),
        "{stale}"
    );
    let height_submit = submit(&height);
    let done = peer
        .act("form", json!({"actionId":height_submit,"formValues":{"sizedropdown":"height","dropdown":"1024","textfield":"16"}}))
        .await
        .unwrap();
    assert_eq!(
        done,
        json!({"ok":true,"mode":"no-view","message":"1.563vh copied to clipboard"})
    );
    assert_eq!(
        peer.methods(),
        [
            ("host.clipboard.write".to_owned(), json!({"text":"1.563vh"})),
            (
                "host.window.visibility".to_owned(),
                json!({"visible":false})
            ),
        ]
    );
    // Completed sessions reject further actions.
    assert!(peer
        .act("form", json!({"actionId":height_submit,"formValues":{}}))
        .await
        .is_err());
    peer.dispose("form").await;

    // The draft is restored on the next launch, as the original storeValue.
    let reopened = peer.open("form2", "index", json!({})).await.unwrap();
    assert_eq!(
        find(&reopened["root"], "Form.TextField", None).unwrap()["props"]["value"],
        "16"
    );
    peer.dispose("form2").await;

    // No-view width conversion with the original JavaScript semantics.
    peer.calls.clear();
    let width = peer
        .open("w", "width", json!({"width":"1e2","pixels":"16"}))
        .await
        .unwrap();
    assert_eq!(width["message"], "16.000vw copied to clipboard");
    assert_eq!(
        peer.methods()[0],
        (
            "host.clipboard.write".to_owned(),
            json!({"text":"16.000vw"})
        )
    );
    peer.dispose("w").await;
    // NaN conversion is recorded and nothing is copied.
    peer.calls.clear();
    let nan = peer
        .open("nan", "width", json!({"width":"20px","pixels":"16"}))
        .await
        .unwrap();
    assert_eq!(
        nan["message"],
        "Conversion was NaN; saved in history, nothing copied"
    );
    assert!(peer.calls.is_empty());
    peer.dispose("nan").await;
    // Invalid arguments fail the launch without effects.
    let rejected = peer
        .open("bad", "height", json!({"height":"0","pixels":"16"}))
        .await
        .unwrap_err();
    assert_eq!(
        rejected["message"],
        "Viewport and pixels must be greater than 0"
    );
    assert!(peer.calls.is_empty());

    // History: newest first, explicit Copy/Remove per entry and global Clear.
    let history = peer.open("h", "menu-bar", json!({})).await.unwrap();
    assert_eq!(history["title"], "Conversion History");
    let rows = history_rows(&history);
    let titles: Vec<_> = rows
        .iter()
        .map(|r| r["props"]["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        titles,
        [
            "20pxpx / 16px → NaNvw",
            "1e2px / 16px → 16.000vw",
            "1024px / 16px → 1.563vh"
        ]
    );
    renderer_consumes(&history, &history, &json!("unused"), Some(&rows[1]));
    let actions = history["actions"].as_array().unwrap().clone();
    assert_eq!(actions.len(), 7);
    let title = |id: &Value| actions.iter().find(|a| &a["id"] == id).unwrap()["title"].clone();
    let ids = rows[1]["props"]["actionIds"].as_array().unwrap().clone();
    assert_eq!(title(&ids[0]), "Copy Value");
    assert_eq!(title(&ids[1]), "Remove from History");
    assert_eq!(
        title(&ids[2]),
        "Clear History",
        "selected-row overlay must expose clear"
    );
    let removed = peer.act("h", json!({"actionId":ids[1]})).await.unwrap();
    assert_eq!(history_rows(&removed).len(), 2);
    // Old IDs are stale after the revision changed.
    assert!(peer.act("h", json!({"actionId":ids[0]})).await.is_err());
    let clear = removed["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == "Clear History")
        .unwrap()["id"]
        .clone();
    let cleared = peer.act("h", json!({"actionId":clear})).await.unwrap();
    assert!(history_rows(&cleared).is_empty());
    assert_eq!(
        cleared["root"]["children"][0]["props"]["title"],
        "No history"
    );
    let latest_tray = peer
        .calls
        .iter()
        .rev()
        .find(|c| c["method"] == "host.tray.set")
        .unwrap();
    assert!(latest_tray["params"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["title"] == "No history"));
    peer.dispose("h").await;
    peer.shutdown().await;
}

#[tokio::test]
async fn failed_clipboard_retry_resumes_without_duplicate_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(dir.path(), GRANTS).await;
    let opened = peer.open("form", "index", json!({})).await.unwrap();
    let device = find(&opened["root"], "Form.Dropdown", Some("dropdown")).unwrap()["props"]
        ["actionId"]
        .clone();
    // Unknown device values are rejected against the current preset list.
    assert!(peer
        .act(
            "form",
            json!({"actionId":device,"formValues":{"value":"844"}})
        )
        .await
        .is_err());
    let view = peer
        .act(
            "form",
            json!({"actionId":device,"formValues":{"value":"390"}}),
        )
        .await
        .unwrap();
    let submit_id = submit(&view);
    assert_ne!(
        submit_id,
        submit(&opened),
        "device changes expire submit IDs"
    );
    let values = json!({"sizedropdown":"width","dropdown":"390","textfield":"16"});

    peer.replies.push_back(Err("clipboard denied".into()));
    let failed = peer
        .act("form", json!({"actionId":submit_id,"formValues":values}))
        .await
        .unwrap_err();
    assert!(
        failed["message"]
            .as_str()
            .unwrap()
            .contains("clipboard denied"),
        "{failed}"
    );
    // Other actions are blocked until the pending conversion is retried.
    let field = find(&view["root"], "Form.TextField", None).unwrap()["props"]["actionId"].clone();
    assert!(peer
        .act("form", json!({"actionId":field,"formValues":{"value":"1"}}))
        .await
        .is_err());
    // Hide fails after a confirmed copy: the retry repeats only hide.
    peer.replies.push_back(Ok(json!({"ok":true})));
    peer.replies.push_back(Err("focus".into()));
    assert!(peer
        .act("form", json!({"actionId":submit_id,"formValues":values}))
        .await
        .is_err());
    let done = peer
        .act("form", json!({"actionId":submit_id,"formValues":values}))
        .await
        .unwrap();
    assert_eq!(done["message"], "4.103vw copied to clipboard");
    let methods: Vec<_> = peer.methods().into_iter().map(|m| m.0).collect();
    assert_eq!(
        methods,
        [
            "host.clipboard.write",
            "host.clipboard.write",
            "host.window.visibility",
            "host.window.visibility"
        ]
    );
    peer.dispose("form").await;
    let history = peer.open("h", "menu-bar", json!({})).await.unwrap();
    let rows = history_rows(&history);
    assert_eq!(rows.len(), 1, "retry must not repeat the history write");
    assert_eq!(rows[0]["props"]["title"], "390px / 16px → 4.103vw");

    // History copy: copy confirmed, hide fails, retry only hides.
    peer.calls.clear();
    let copy = rows[0]["props"]["actionIds"][0].clone();
    peer.replies.push_back(Ok(json!({"ok":true})));
    peer.replies.push_back(Err("hide failed".into()));
    assert!(peer.act("h", json!({"actionId":copy})).await.is_err());
    let copied = peer.act("h", json!({"actionId":copy})).await.unwrap();
    assert_eq!(copied["message"], "4.103vw copied to clipboard");
    let methods: Vec<_> = peer.methods().into_iter().map(|m| m.0).collect();
    assert_eq!(
        methods,
        [
            "host.clipboard.write",
            "host.window.visibility",
            "host.window.visibility"
        ]
    );
    peer.dispose("h").await;
    peer.shutdown().await;
}

#[tokio::test]
async fn ungranted_host_capability_fails_without_reverse_rpc() {
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(
        dir.path(),
        &[
            "tray.set",
            "tray.remove",
            "background.retain",
            "background.release",
        ],
    )
    .await;
    let error = peer
        .open("w", "width", json!({"width":"1440","pixels":"16"}))
        .await
        .unwrap_err();
    assert!(
        error["message"].as_str().unwrap().contains("not granted"),
        "{error}"
    );
    assert!(peer.calls.is_empty());
    // Unknown command names are rejected by the SDK before the adapter.
    assert!(peer.open("x", "convert", json!({})).await.is_err());
    // The history write before the denied copy remains, as in the original order.
    let history = peer.open("h", "menu-bar", json!({})).await.unwrap();
    assert_eq!(history_rows(&history).len(), 1);
    peer.dispose("h").await;
    peer.shutdown().await;
}

#[tokio::test]
async fn eof_ends_the_process_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(dir.path(), GRANTS).await;
    peer.input.take();
    let status = tokio::time::timeout(Duration::from_secs(5), peer.child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(status.success());
}

fn renderer_consumes(before: &Value, response: &Value, action: &Value, row: Option<&Value>) {
    let host = std::env::var_os("TEZBAR_HOST_SOURCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Tezbar"));
    // This check intentionally requires the specified host source and its TS compiler.
    let mut child = std::process::Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/renderer-consumer.cjs"))
        .env("TEZBAR_HOST_SOURCE", host)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::to_string(
                &json!({"before":before,"response":response,"actionId":action,"row":row}),
            )
            .unwrap()
            .as_bytes(),
        )
        .unwrap();
    assert!(
        child.wait().unwrap().success(),
        "current renderer consumption failed"
    );
}

fn legacy_item(value: &str) -> Value {
    json!({"date":"2026-11-06T10:00:00.000Z","data":{"value":value,"label":format!("1440px / 16px → {value}")}})
}
fn cache_file(dir: &Path, name: &str, entries: Value) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"history":entries.to_string(),"unrelated":"private legacy value"}),
        )
        .unwrap(),
    )
    .unwrap();
    path
}
fn native_file(dir: &Path) -> std::path::PathBuf {
    dir.join("data/convert-px-to-vw-vh/storage.json")
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[tokio::test]
async fn js_cache_migration_preserves_duplicates_order_limits_source_and_idempotency() {
    for key in ["jsCacheManaged", "jsCacheFlat", "jsCacheRegistry"] {
        let dir = tempfile::tempdir().unwrap();
        let items: Vec<_> = (0..12)
            .map(|index| legacy_item(if index % 2 == 0 { "1.111vw" } else { "1.563vh" }))
            .collect();
        let path = cache_file(dir.path(), "shared.json", json!(items));
        let before = std::fs::read(&path).unwrap();
        let paths = json!({key:path});
        let mut peer = Peer::start(&binary(), dir.path()).await;
        peer.initialize_paths(dir.path(), GRANTS, paths.clone())
            .await
            .unwrap();
        let history = peer.open("m", "menu-bar", json!({})).await.unwrap();
        let rows = history_rows(&history);
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0]["props"]["id"], "12");
        assert_eq!(rows[9]["props"]["id"], "3");
        for (row, original) in rows.iter().zip(&items) {
            assert_eq!(row["props"]["title"], original["data"]["label"]);
            assert_eq!(row["props"]["subtitle"], original["date"]);
        }
        let clear = rows[0]["props"]["actionIds"][2].clone();
        peer.act("m", json!({"actionId":clear})).await.unwrap();
        let cleared = std::fs::read(native_file(dir.path())).unwrap();
        assert_eq!(read_json(&native_file(dir.path()))["history"], json!([]));
        peer.shutdown().await;
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "never modify the legacy source"
        );
        // Relaunch with changed/malformed source must not reimport a cleared history.
        std::fs::write(&path, b"malformed after migration").unwrap();
        let mut peer = Peer::start(&binary(), dir.path()).await;
        peer.initialize_paths(dir.path(), GRANTS, paths)
            .await
            .unwrap();
        assert_eq!(std::fs::read(native_file(dir.path())).unwrap(), cleared);
        peer.shutdown().await;
    }
}

#[tokio::test]
async fn migration_priority_private_native_data_and_failures() {
    let dir = tempfile::tempdir().unwrap();
    let managed = cache_file(dir.path(), "managed.json", json!([legacy_item("1.111vw")]));
    let flat = cache_file(dir.path(), "flat.json", json!([legacy_item("1.563vh")]));
    let paths = json!({"jsCacheManaged":managed,"jsCacheFlat":flat});
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize_paths(dir.path(), GRANTS, paths)
        .await
        .unwrap();
    assert_eq!(
        read_json(&native_file(dir.path()))["history"][0]["value"],
        "1.563vh"
    );
    peer.shutdown().await;

    // Any existing nonempty native store is authoritative and stays byte-identical.
    for native in [
        json!({"textfield":"private draft","unknown":"private value"}),
        json!({"historyRevision":7,"history":[],"unknown":"private value"}),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(native_file(dir.path()).parent().unwrap()).unwrap();
        let bytes = serde_json::to_vec_pretty(&native).unwrap();
        std::fs::write(native_file(dir.path()), &bytes).unwrap();
        let legacy = dir.path().join("missing.json");
        let mut peer = Peer::start(&binary(), dir.path()).await;
        peer.initialize_paths(dir.path(), GRANTS, json!({"jsCacheManaged":legacy}))
            .await
            .unwrap();
        assert_eq!(std::fs::read(native_file(dir.path())).unwrap(), bytes);
        peer.shutdown().await;
    }
    // Real process failures must reach initialize's error envelope and leave both
    // source and native data untouched; a new process may safely retry.
    for bad in [
        "not-json",
        "[]",
        "{\"history\":[]}",
        "{\"history\":\"not-json\"}",
        "{\"history\":\"[{}]\"}",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, bad).unwrap();
        let mut peer = Peer::start(&binary(), dir.path()).await;
        assert!(peer
            .initialize_paths(dir.path(), GRANTS, json!({"jsCacheManaged":path}))
            .await
            .is_err());
        assert!(!native_file(dir.path()).exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bad);
        drop(peer);
        cache_file(dir.path(), "bad.json", json!([legacy_item("1.111vw")]));
        let mut peer = Peer::start(&binary(), dir.path()).await;
        peer.initialize_paths(dir.path(), GRANTS, json!({"jsCacheManaged":path}))
            .await
            .unwrap();
        assert_eq!(
            read_json(&native_file(dir.path()))["history"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        peer.shutdown().await;
    }
    // Supplied read paths are not silently skipped when missing/unreadable.
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    let error = peer
        .initialize_paths(
            dir.path(),
            GRANTS,
            json!({"jsCacheManaged":dir.path().join("missing")}),
        )
        .await
        .unwrap_err();
    assert!(error["message"].as_str().unwrap().contains("read failed"));
    drop(peer);
    // Block the SDK destination with a regular file: migration write must fail.
    let source = cache_file(dir.path(), "shared.json", json!([legacy_item("1.111vw")]));
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    std::fs::write(
        dir.path().join("data/convert-px-to-vw-vh"),
        "private blocker",
    )
    .unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    assert!(peer
        .initialize_paths(dir.path(), GRANTS, json!({"jsCacheManaged":source}))
        .await
        .is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("data/convert-px-to-vw-vh")).unwrap(),
        "private blocker"
    );
}

#[tokio::test]
async fn tray_lifecycle_events_mutations_restart_and_failures() {
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(dir.path(), GRANTS).await;
    peer.open("history", "menu-bar", json!({})).await.unwrap();
    let tray = |peer: &Peer| {
        peer.calls
            .iter()
            .rev()
            .find(|c| c["method"] == "host.tray.set")
            .unwrap()["params"]
            .clone()
    };
    let empty = tray(&peer);
    assert_eq!(empty["resourceId"], "conversion-history");
    assert_eq!(empty["icon"], "resources/assets/logo.png");
    assert!(empty["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["title"] == "No history" && i["enabled"] == false));
    assert!(peer
        .calls
        .iter()
        .any(|c| c["method"] == "host.background.retain" && c["params"]["durable"] == true));
    peer.dispose("history").await;
    assert!(
        !peer.calls.iter().any(|c| c["method"] == "host.tray.remove"),
        "tray survives session disposal"
    );
    peer.open("w", "width", json!({"width":"1440","pixels":"16"}))
        .await
        .unwrap();
    let latest = tray(&peer);
    let item = |view: &Value, prefix: &str| {
        view["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["id"].as_str().unwrap().starts_with(prefix))
            .unwrap()["id"]
            .clone()
    };
    let copy = item(&latest, "copy-");
    let remove = item(&latest, "remove-");
    let event = |action: Value| json!({"resourceId":"conversion-history","actionId":action});
    peer.calls.clear();
    peer.request("background.event", event(copy.clone())).await;
    assert_eq!(
        peer.methods(),
        [("host.clipboard.write".into(), json!({"text":"1.111vw"}))]
    );
    peer.replies.push_back(Err("clipboard denied".into()));
    assert!(peer
        .try_request("background.event", event(copy.clone()))
        .await
        .is_ok());
    peer.request("background.event", event(copy.clone())).await;
    peer.request("background.event", event(json!("configure")))
        .await;
    assert!(peer
        .calls
        .iter()
        .any(|c| c["method"] == "host.settings.open"
            && c["params"] == json!({"surface":"extensions"})));
    assert!(peer
        .try_request(
            "background.event",
            json!({"resourceId":"other","actionId":copy})
        )
        .await
        .is_ok());
    assert!(peer
        .try_request("background.event", event(json!("invented")))
        .await
        .is_ok());
    peer.request("background.event", event(remove)).await;
    assert!(read_json(&native_file(dir.path()))["history"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(
        peer.try_request("background.event", event(copy))
            .await
            .is_ok(),
        "old revision expires tray IDs"
    );
    peer.open("w2", "width", json!({"width":"1440","pixels":"16"}))
        .await
        .unwrap();
    let populated = tray(&peer);
    peer.request("background.event", event(item(&populated, "clear-")))
        .await;
    assert!(read_json(&native_file(dir.path()))["history"]
        .as_array()
        .unwrap()
        .is_empty());
    peer.request("background.event", event(json!("refresh")))
        .await;
    // Stop releases durable registration and removes the actual tray.
    peer.request("background.event", event(json!("stop"))).await;
    assert!(peer
        .calls
        .iter()
        .any(|c| c["method"] == "host.background.release"));
    assert!(peer.calls.iter().any(|c| c["method"] == "host.tray.remove"));
    assert!(peer
        .try_request("background.event", event(json!("refresh")))
        .await
        .is_ok());
    assert!(peer
        .try_request("background.activate", json!({"key":"other"}))
        .await
        .is_err());
    // The actual restoration envelope from native.rs is key/data.
    peer.request(
        "background.activate",
        json!({"key":"conversion-history","data":{}}),
    )
    .await;
    peer.calls.clear();
    peer.request("shutdown", json!({})).await;
    assert_eq!(peer.calls.len(), 1);
    assert_eq!(peer.calls[0]["method"], "host.tray.remove");
    assert!(peer.child.wait().await.unwrap().success());
    drop(peer);
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(dir.path(), GRANTS).await;
    peer.replies.push_back(Err("tray unavailable".into()));
    assert!(peer
        .try_request(
            "background.activate",
            json!({"key":"conversion-history","data":{}})
        )
        .await
        .is_err());
    peer.replies.push_back(Ok(json!(true))); // tray set succeeds
    peer.replies.push_back(Err("durable storage denied".into()));
    assert!(peer
        .try_request(
            "background.activate",
            json!({"key":"conversion-history","data":{}})
        )
        .await
        .is_err());
    assert_eq!(
        peer.calls.last().unwrap()["method"],
        "host.tray.remove",
        "failed first activation cleans up the tray"
    );
    peer.request(
        "background.activate",
        json!({"key":"conversion-history","data":{}}),
    )
    .await;
    peer.shutdown().await;
}

#[tokio::test]
async fn synthetic_in_band_failure_is_a_robustness_probe_not_a_host_oracle() {
    let dir = tempfile::tempdir().unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize(dir.path(), GRANTS).await;
    peer.replies
        .push_back(Ok(json!({"ok":false,"error":"synthetic"})));
    let failed = peer
        .open("w", "width", json!({"width":"1440","pixels":"16"}))
        .await
        .unwrap_err();
    assert!(failed["message"].as_str().unwrap().contains("synthetic"));
    assert_eq!(peer.methods().len(), 1);
    peer.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn migration_atomic_write_failure_preserves_source_and_retries() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let source = cache_file(dir.path(), "shared.json", json!([legacy_item("1.111vw")]));
    let bytes = std::fs::read(&source).unwrap();
    let parent = native_file(dir.path()).parent().unwrap().to_owned();
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500)).unwrap();
    let mut peer = Peer::start(&binary(), dir.path()).await;
    let result = peer
        .initialize_paths(dir.path(), GRANTS, json!({"jsCacheManaged":source}))
        .await;
    // Restore permissions before any assertion so tempdir cleanup stays possible.
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        result.is_err(),
        "SDK atomic write to a read-only parent must fail"
    );
    assert!(!native_file(dir.path()).exists());
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    drop(peer);
    let mut peer = Peer::start(&binary(), dir.path()).await;
    peer.initialize_paths(dir.path(), GRANTS, json!({"jsCacheManaged":source}))
        .await
        .unwrap();
    assert_eq!(
        read_json(&native_file(dir.path()))["history"][0]["value"],
        "1.111vw"
    );
    peer.shutdown().await;
}

#[tokio::test]
async fn completed_conversion_survives_optional_tray_failure() {
    for command in ["width", "height", "index"] {
        let dir = tempfile::tempdir().unwrap();
        let mut peer = Peer::start(&binary(), dir.path()).await;
        peer.initialize(dir.path(), GRANTS).await;
        peer.open("tray", "menu-bar", json!({})).await.unwrap();
        let form = if command == "index" {
            Some(peer.open("conversion", command, json!({})).await.unwrap())
        } else {
            None
        };
        peer.calls.clear();
        peer.replies.extend([
            Ok(json!({"ok":true})),
            Ok(Value::Null),
            Err("tray unavailable".into()),
        ]);
        let action = form.as_ref().map(submit);
        let result = if let Some(id) = &action {
            peer.act("conversion", json!({"actionId":id,"formValues":{"sizedropdown":"width","dropdown":"1440","textfield":"16"}})).await
        } else {
            peer.open("conversion", command, json!({"width":"1440","height":"1024","pixels":"16"})).await
        }.unwrap();
        assert_eq!(result["mode"], "no-view");
        assert!(result["trayDiagnostic"].is_string());
        assert_eq!(peer.calls[0]["method"], "host.clipboard.write");
        assert_eq!(peer.calls[1]["method"], "host.window.visibility");
        assert_eq!(peer.calls[2]["method"], "host.tray.set");
        let saved = std::fs::read(native_file(dir.path())).unwrap();
        peer.calls.clear();
        peer.request(
            "background.event",
            json!({"resourceId":"conversion-history","actionId":"refresh"}),
        )
        .await;
        assert!(peer.calls.iter().all(|c| c["method"] == "host.tray.set"));
        assert!(peer.calls[0]["params"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == "diagnostic"));
        if let Some(id) = action {
            assert!(peer
                .act("conversion", json!({"actionId":id,"formValues":{}}))
                .await
                .is_err());
        }
        peer.dispose("conversion").await;
        assert_eq!(std::fs::read(native_file(dir.path())).unwrap(), saved);
        peer.shutdown().await;
    }
}

#[tokio::test]
async fn exact_grants_select_first_valid_package_owner() {
    let manifest: Value = serde_json::from_str(include_str!("../extension.json")).unwrap();
    for scenario in [
        "all",
        "invalid-flat",
        "absent-flat",
        "flat-no-cache",
        "native-owned",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = json!({});
        for kind in ["Flat", "Managed", "Registry"] {
            let package_key = format!("jsPackage{kind}");
            let cache_key = format!("jsCache{kind}");
            let package = dir
                .path()
                .join(manifest["migrationPaths"][&package_key].as_str().unwrap());
            let cache = dir
                .path()
                .join(manifest["migrationPaths"][&cache_key].as_str().unwrap());
            std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
            if !(scenario == "absent-flat" && kind == "Flat") {
                std::fs::write(
                    &package,
                    if scenario == "invalid-flat" && kind == "Flat" {
                        "[]"
                    } else {
                        include_str!("../package.json")
                    },
                )
                .unwrap();
            }
            paths[&package_key] = json!(package);
            if !(scenario == "flat-no-cache" && kind == "Flat") {
                cache_file(
                    cache.parent().unwrap(),
                    "shared.json",
                    json!([legacy_item(match kind {
                        "Flat" => "2.222vw",
                        "Managed" => "1.111vw",
                        _ => "3.333vw",
                    })]),
                );
                paths[&cache_key] = json!(cache);
            }
        }
        if scenario == "native-owned" {
            std::fs::create_dir_all(native_file(dir.path()).parent().unwrap()).unwrap();
            std::fs::write(
                native_file(dir.path()),
                br#"{"tezbarJsCacheMigration":1,"historyRevision":4,"history":[]}"#,
            )
            .unwrap();
        }
        let mut peer = Peer::start(&binary(), dir.path()).await;
        peer.initialize_paths(dir.path(), GRANTS, paths)
            .await
            .unwrap();
        let data = read_json(&native_file(dir.path()));
        match scenario {
            "all" => assert_eq!(data["history"][0]["value"], "2.222vw"),
            "invalid-flat" | "absent-flat" => assert_eq!(data["history"][0]["value"], "1.111vw"),
            "flat-no-cache" => assert!(data.get("history").is_none()),
            _ => assert_eq!(data["historyRevision"], 4),
        }
        peer.shutdown().await;
    }
}
