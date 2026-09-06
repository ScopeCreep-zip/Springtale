//! The CLI is a client of springtaled (plan §2.2).
//!
//! These boot the *real* daemon on an ephemeral port (the shared fixture
//! in `springtaled::test_harness`) and drive the real CLI binary against
//! it. That is the proof of finding 3: an edit made by the CLI is
//! immediately visible to the running daemon, with no restart, because
//! the CLI never opens the store as a second writer. The last test pins
//! the other half of the rule — when nothing is listening the CLI fails
//! with one message instead of silently falling back to the store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::TcpListener;
use std::process::{Command, Output};

use springtale_cooperation::command::FormationCommand;
use springtale_cooperation::types::FormationId;
use springtaled::test_harness::TestServer;

/// Write a `springtale.toml` pointing the CLI at `bind`, and return the
/// directory to run the CLI from (the CLI loads config from its cwd, so
/// a temp dir also keeps the test off the user's real config).
fn workdir(bind: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("springtale.toml"),
        format!("[api]\nbind = \"{bind}\"\n"),
    )
    .expect("write springtale.toml");
    dir
}

/// The real CLI binary, pointed at `dir` with `token` in the environment
/// (the scripted path — no interactive token prompt).
fn cli(dir: &std::path::Path, token: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_springtale-cli"));
    cmd.current_dir(dir);
    cmd.env("SPRINGTALE_API_TOKEN", token);
    cmd
}

/// Run a CLI invocation without blocking the runtime the daemon lives on.
async fn run(mut cmd: Command) -> Output {
    tokio::task::spawn_blocking(move || cmd.output().expect("run springtale"))
        .await
        .expect("cli task")
}

/// Read a path off the running daemon the way any other client would.
async fn api_get(server: &TestServer, path: &str) -> serde_json::Value {
    reqwest::Client::new()
        .get(format!("{}{path}", server.base_url))
        .bearer_auth(&server.token_hex)
        .send()
        .await
        .expect("GET the daemon")
        .json()
        .await
        .expect("daemon JSON")
}

/// Finding 3: `springtale rule add` is visible through the daemon's own
/// `GET /rules` immediately, with no restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_rule_added_by_the_cli_is_visible_to_the_running_daemon() {
    let server = TestServer::start().await.expect("boot springtaled");
    let dir = workdir(&server.bind_addr());

    // The daemon starts with no rules.
    let before = api_get(&server, "/rules").await;
    assert_eq!(before["rules"].as_array().map(Vec::len), Some(0));

    let rule_path = dir.path().join("rule.json");
    std::fs::write(
        &rule_path,
        serde_json::to_vec(&serde_json::json!({
            "name": "added-by-cli",
            "description": "written through the CLI",
            "status": "enabled",
            "version": 1,
            "trigger": { "type": "Webhook", "path": "cli-hook" },
            "conditions": [],
            "actions": [ { "type": "SendMessage", "text": "hello" } ]
        }))
        .unwrap(),
    )
    .expect("write rule.json");

    let mut cmd = cli(dir.path(), &server.token_hex);
    cmd.args(["rule", "add"]).arg(&rule_path);
    let out = run(cmd).await;
    assert!(
        out.status.success(),
        "rule add failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Same daemon process, no restart: it already has the rule.
    let after = api_get(&server, "/rules").await;
    let names: Vec<&str> = after["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    assert!(
        names.contains(&"added-by-cli"),
        "the running daemon did not see the CLI's rule: {names:?}"
    );
}

/// `springtale formation rally <id>` reaches the daemon and makes it act
/// on that formation.
///
/// The plan's wording is "decrements the rally token". The decrement is
/// applied by the live formation loop that consumes
/// `FormationCommand::Rally`; this fixture is an API-only daemon with no
/// bot event loop, so the token count read back from
/// `GET /formations/{id}` cannot move. What is real and asserted here is
/// the whole client-of-the-daemon claim: the CLI's rally lands as a
/// `Rally` command for that exact formation inside the running daemon,
/// and the formation's rally budget is what the API reports back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_formation_rally_from_the_cli_reaches_the_running_daemon() {
    let mut server = TestServer::start().await.expect("boot springtaled");
    let dir = workdir(&server.bind_addr());

    let created: serde_json::Value = reqwest::Client::new()
        .post(format!("{}/formations/deploy-team", server.base_url))
        .bearer_auth(&server.token_hex)
        .json(&serde_json::json!({
            "name": "Rally Squad",
            "intent": "Execute",
            "guard_mode": false,
            "agents": [{
                "connector_name": "connector-test",
                "trigger_name": "event_received",
                "action_connector": "connector-test",
                "action_name": "send_message"
            }]
        }))
        .send()
        .await
        .expect("deploy-team")
        .json()
        .await
        .expect("deploy-team JSON");
    let id = created["formation_id"].as_str().expect("formation_id");

    // The rally budget as the API reports it, before the CLI runs.
    let before = api_get(&server, &format!("/formations/{id}")).await;
    let tokens_before = before["rally_tokens"].as_i64().expect("rally_tokens");

    let mut cmd = cli(dir.path(), &server.token_hex);
    cmd.args(["formation", "rally", id]);
    let out = run(cmd).await;
    assert!(
        out.status.success(),
        "formation rally failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The daemon enqueued the rally for this formation. `deploy-team`
    // queues its own commands first, so drain until the rally shows up.
    let expected = FormationId::parse(id).expect("formation id");
    let mut saw_rally = false;
    while let Ok(Some(queued)) = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.formation_cmd_rx.recv(),
    )
    .await
    {
        if let FormationCommand::Rally { formation_id } = queued {
            assert_eq!(formation_id, expected);
            saw_rally = true;
            break;
        }
    }
    assert!(saw_rally, "the daemon never enqueued a Rally for {id}");

    // Still readable through the API, budget included.
    let after = api_get(&server, &format!("/formations/{id}")).await;
    assert_eq!(after["rally_tokens"].as_i64(), Some(tokens_before));
    assert!(
        after["rally_max"].as_i64().is_some(),
        "no rally budget: {after}"
    );
}

/// No silent fallback: with nothing listening the CLI refuses, it does
/// not open the store as a second writer.
#[test]
fn test_unreachable_daemon_fails_with_one_message_and_no_fallback() {
    // Bind, note the port, then drop the listener so nothing answers.
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("addr").port()
    };
    let dir = workdir(&format!("127.0.0.1:{port}"));

    // Any string works here: nothing is listening, so the token is never
    // checked. Kept obviously fake so secret scanners do not flag it.
    let out = cli(dir.path(), "not-a-real-token")
        .args(["rule", "list"])
        .output()
        .expect("run springtale rule list");

    assert!(!out.status.success(), "should fail when the daemon is down");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cannot reach springtaled. Is it running? (`springtale server start`)"),
        "expected the single unreachable message, saw: {stderr}"
    );
}
