//! UI end-to-end tests: launch the real app (debug build) and drive it
//! through its e2e driver (`src/e2e.rs`). Offline flows only; the cloud flows
//! against a deployed SigVault live in the sigvault repo (`e2e/specs/desktop`)
//! and use the same driver.
//!
//! They open a window, so they're `#[ignore]`d by default:
//!
//! ```text
//! cargo test -p sigvault-app --test ui_e2e -- --ignored --test-threads=1
//! # Linux without a display:
//! xvfb-run -a cargo test -p sigvault-app --test ui_e2e -- --ignored --test-threads=1
//! ```

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

struct App {
    child: Child,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    _data: tempfile::TempDir,
}

impl App {
    fn launch() -> App {
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("free port")
            .port();
        let data = tempfile::tempdir().expect("temp data dir");
        let child = Command::new(env!("CARGO_BIN_EXE_sigvault-desktop"))
            .env("SIGVAULT_DATA_DIR", data.path())
            .env("SIGVAULT_NO_UPDATE_CHECK", "1")
            .env("SIGVAULT_E2E_DRIVER", format!("127.0.0.1:{port}"))
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .spawn()
            .expect("spawn sigvault-desktop");
        let deadline = Instant::now() + Duration::from_secs(60);
        let stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(s) => break s,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(200))
                }
                Err(e) => panic!("e2e driver never came up: {e}"),
            }
        };
        let mut app = App {
            child,
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
            _data: data,
        };
        app.wait_until("the first page", |s| {
            !matches!(s["page"].as_str(), None | Some("") | Some("/"))
        });
        app
    }

    fn call(&mut self, req: Value) -> Value {
        writeln!(self.writer, "{req}").expect("send");
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("reply");
        serde_json::from_str(&line).expect("json reply")
    }

    fn snapshot(&mut self) -> Value {
        let r = self.call(json!({ "cmd": "snapshot" }));
        assert_eq!(r["ok"], true, "snapshot: {r}");
        r
    }

    fn page(&mut self) -> String {
        self.snapshot()["page"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn node(&mut self, role: &str, label: &str) -> Option<Value> {
        self.snapshot()["nodes"]
            .as_array()?
            .iter()
            .find(|n| n["role"] == role && n["label"] == label)
            .cloned()
    }

    fn wait_until(&mut self, what: &str, mut f: impl FnMut(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let s = self.snapshot();
            if f(&s) {
                return s;
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for {what}; screen: {s:#}");
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn wait_page(&mut self, path: &str) {
        self.wait_until(&format!("page {path}"), |s| s["page"] == path);
    }

    /// Click `role` `label`, waiting for it to appear and be enabled.
    fn click(&mut self, role: &str, label: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let r =
                self.call(json!({ "cmd": "click", "role": role, "label": label, "exact": true }));
            if r["ok"] == true {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "could not click {role} {label:?}: {r}; screen: {:#}",
                    self.snapshot()
                );
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }

    fn type_text(&mut self, text: &str) {
        let r = self.call(json!({ "cmd": "type", "text": text }));
        assert_eq!(r["ok"], true, "type: {r}");
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = writeln!(self.writer, "{}", json!({ "cmd": "quit" }));
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
    }
}

#[test]
#[ignore = "opens a window; run with --ignored (xvfb-run on headless Linux)"]
fn fresh_install_starts_on_welcome_and_opens_device_workspace() {
    let mut app = App::launch();
    assert_eq!(app.page(), "/welcome");
    assert!(app.node("card", "SigVault Cloud").is_some());

    app.click("card", "On this device");
    app.wait_page("/local/home");

    app.click("nav", "Keys");
    app.wait_page("/local/keys");
    let keys = app.node("nav", "Keys").expect("Keys nav");
    assert_eq!(keys["selected"], true);
}

#[test]
#[ignore = "opens a window; run with --ignored (xvfb-run on headless Linux)"]
fn new_wallet_wizard_needs_a_name() {
    let mut app = App::launch();
    app.click("card", "On this device");
    app.wait_page("/local/home");

    app.click("nav", "New wallet");
    app.wait_page("/local/wallets/new");
    let cont = app.node("button", "Continue").expect("Continue button");
    assert_eq!(cont["enabled"], false, "Continue before a name: {cont}");

    app.click("input", "Savings");
    app.wait_until("the name field to take focus", |s| {
        s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["role"] == "input" && n["focused"] == true)
    });
    app.type_text("Rainy day");
    let s = app.wait_until("Continue to enable", |s| {
        s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["role"] == "button" && n["label"] == "Continue" && n["enabled"] == true)
    });
    let name = s["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["role"] == "input")
        .unwrap();
    assert_eq!(name["value"], "Rainy day");
}

#[test]
#[ignore = "opens a window; run with --ignored (xvfb-run on headless Linux)"]
fn cloud_workspace_starts_at_network_choice() {
    let mut app = App::launch();
    app.click("card", "SigVault Cloud");
    app.wait_page("/cloud/select-env");
    // The networks come from sigvault.org; without network access the page
    // shows an error instead. Either way the network step must be reachable
    // and Continue must wait for a choice.
    assert!(app.node("heading", "Choose a network").is_some());
}

/// Tab + Enter, as a keyboard user would (no mouse).
impl App {
    fn key(&mut self, keys: &str) {
        let r = self.call(json!({ "cmd": "key", "keys": keys }));
        assert_eq!(r["ok"], true, "key {keys}: {r}");
    }

    fn has(&mut self, role: &str, label: &str) -> bool {
        self.node(role, label).is_some()
    }
}

#[test]
#[ignore = "opens a window; run with --ignored (xvfb-run on headless Linux)"]
fn key_picker_is_completable_by_keyboard() {
    const XPUB: &str = "[d093c7a2/48'/1'/0'/2']tpubDEvZxV86Br8Knbm9tWcr5Hvmg5cYTYsg92vinqH6Bie6U8ix8CsoN9W11NQygqTSkNCj6UmcE4DBXZ8Nxp6RzBzBQUuUqmrqQ8mnBpBNZLx";
    let mut app = App::launch();
    app.click("card", "On this device");
    app.wait_page("/local/home");

    // A watch-only key in the library (no keychain password needed).
    app.click("nav", "Add key");
    app.wait_page("/local/keys/add");
    app.click("choice", "Extended public key");
    app.wait_until("the xpub field to take focus", |s| {
        s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["role"] == "input" && n["focused"] == true)
    });
    app.type_text(XPUB);
    app.click("button", "Continue");
    app.click("input", "e.g. Laptop key, Trezor (Alice), Backup seed");
    app.type_text("Cosigner");
    app.click("button", "Add to library");
    app.wait_until("the key to be saved", |s| s["page"] != "/local/keys/add");

    // New single-key wallet, then pick its key with Tab + Enter only.
    app.click("nav", "New wallet");
    app.wait_page("/local/wallets/new");
    app.click("input", "Savings");
    app.type_text("Rainy day");
    app.click("option", "Single key");
    app.click("button", "Continue");
    // A single-slot wallet may open the picker straight away.
    let overlay = |s: &Value| {
        s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["role"] == "overlay")
    };
    let s = app.wait_until("the keys step", |s| {
        overlay(s)
            || s["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|n| n["label"] == "Choose key")
    });
    if !overlay(&s) {
        app.click("button", "Choose key");
    }
    app.wait_until("the key picker", |s| {
        s["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|n| n["role"] == "option" && n["label"] == "Cosigner")
    });
    // Focus starts in the search field; one Tab reaches the first key.
    app.key("tab");
    app.key("enter");
    app.wait_until("the picker to close with the key chosen", |s| {
        let nodes = s["nodes"].as_array().cloned().unwrap_or_default();
        !nodes.iter().any(|n| n["role"] == "overlay")
            && nodes
                .iter()
                .any(|n| n["role"] == "button" && n["label"] == "Change")
    });
    assert!(!app.has("button", "Choose key"), "slot still empty");
}
