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
const GRANTS: &[&str] = &["clipboard.write", "window.visibility"];

struct Peer {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    /// Every reverse request received, in order.
    calls: Vec<Value>,
    /// Scripted replies: Ok(result) or Err(message). Default: `null` result.
    replies: VecDeque<Result<Value, String>>,
    sequence: u64,
}

impl Peer {
    async fn start(binary: &Path, cwd: &Path) -> Self {
        let mut child = Command::new(binary)
            .current_dir(cwd)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
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
                        ["host.clipboard.write", "host.window.visibility"].contains(&method),
                        "unexpected host method {method}"
                    );
                    self.calls.push(frame.clone());
                    let reply = match self.replies.pop_front().unwrap_or(Ok(Value::Null)) {
                        Ok(result) => json!({"type":"response","id":frame["id"],"result":result}),
                        Err(message) => {
                            json!({"type":"response","id":frame["id"],"error":{"code":"host_error","message":message}})
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
        let hello = self.frame().await;
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["id"], ID);
        assert_eq!(hello["protocol"], 1);
        assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
        let ready = self
            .request(
                "initialize",
                json!({"protocol":1,"extensionId":ID,"platform":"macOS",
                    "packageDir":env!("CARGO_MANIFEST_DIR"),"dataDir":dir.join("data"),
                    "tempDir":dir.join("temp"),"grantedCapabilities":grants,
                    "reason":"command","migrationPaths":{}}),
            )
            .await;
        assert_eq!(ready["ready"], true);
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
    assert_eq!(typed["mode"], "unchanged");
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
    let actions = history["actions"].as_array().unwrap().clone();
    assert_eq!(actions.len(), 7);
    let title = |id: &Value| actions.iter().find(|a| &a["id"] == id).unwrap()["title"].clone();
    let ids = rows[1]["props"]["actionIds"].as_array().unwrap().clone();
    assert_eq!(title(&ids[0]), "Copy Value");
    assert_eq!(title(&ids[1]), "Remove from History");
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
    peer.replies.push_back(Ok(Value::Null));
    peer.replies
        .push_back(Ok(json!({"ok":false,"error":"focus"})));
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
    peer.replies.push_back(Ok(Value::Null));
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
    peer.initialize(dir.path(), &[]).await;
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
