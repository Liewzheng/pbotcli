//! End-to-end command-layer tests: run the real `pbotcli` binary against a
//! mockito mock server (no real instance). Pins the `--json` output contract
//! (sequence ids, resolved names, web_url) and exit codes.
//!
//! The binary is located via cargo's `CARGO_BIN_EXE_pbotcli`. Proxies are
//! stripped from the child env so local mockito traffic is not hijacked.

use std::io::Write;
use std::process::{Command, Stdio};

use mockito::{Matcher, Server};

const BIN: &str = env!("CARGO_BIN_EXE_pbotcli");

fn run(args: &[&str], base_url: &str) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env("PLANE_BASE_URL", base_url)
        .env("PLANE_API_KEY", "test-key")
        .env("PLANE_WORKSPACE", "ws")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .output()
        .expect("failed to spawn pbotcli")
}

fn run_with_stdin(args: &[&str], base_url: &str, stdin_data: &[u8]) -> std::process::Output {
    let mut child = Command::new(BIN)
        .args(args)
        .env("PLANE_BASE_URL", base_url)
        .env("PLANE_API_KEY", "test-key")
        .env("PLANE_WORKSPACE", "ws")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn pbotcli");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(stdin_data)
        .expect("failed to write stdin");
    child.wait_with_output().expect("failed to wait")
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

const MEMBERS: &str =
    r#"[{"id":"u1","display_name":"Bot","first_name":"B","last_name":"","email":"b@x"}]"#;
const PROJECTS: &str = r#"{"results":[{"id":"p1","name":"Demo","identifier":"DEMO","created_at":"2026-01-01T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;
const STATES: &str = r##"{"results":[{"id":"s1","name":"Todo","group":"unstarted","color":"#fff","sequence":1}],"next_cursor":null,"next_page_results":false}"##;
const LABELS: &str = r##"{"results":[{"id":"l1","name":"feat","color":"#3B82F6"}],"next_cursor":null,"next_page_results":false}"##;
const ITEM: &str = r#"{"id":"wi1","name":"A task","sequence_id":1,"description_html":"<p>hi</p>","priority":"high","state":"s1","labels":["l1"],"assignees":["u1"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#;
const ITEMS: &str = r#"{"results":[{"id":"wi1","name":"A task","sequence_id":1,"description_html":"<p>hi</p>","priority":"high","state":"s1","labels":["l1"],"assignees":["u1"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;
/// Two work items: `wi2` is a child of `wi1`, so `wi show wi1` has a
/// sub-issue and `wi update wi1 --parent DEMO-2` has a sibling to resolve.
const ITEMS_TWO: &str = r#"{"results":[{"id":"wi1","name":"A task","sequence_id":1,"priority":"high","state":"s1","labels":["l1"],"assignees":["u1"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"},{"id":"wi2","name":"A parent","sequence_id":2,"parent":"wi1","project":"p1"}],"next_cursor":null,"next_page_results":false}"#;
const PROJECT: &str = r#"{"id":"p1","name":"Demo","identifier":"DEMO"}"#;

/// Mock the project-scoped reads every `wi` command makes.
fn mock_project_reads(server: &mut Server, items: &str) {
    server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MEMBERS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECT)
        .create();
    for (path, body) in [
        ("/api/v1/workspaces/ws/projects/p1/states/", STATES),
        ("/api/v1/workspaces/ws/projects/p1/labels/", LABELS),
        ("/api/v1/workspaces/ws/projects/p1/work-items/", items),
    ] {
        server
            .mock("GET", path)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
    }
}

#[test]
fn wi_ls_json_contract() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MEMBERS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    for (path, body) in [
        ("/api/v1/workspaces/ws/projects/p1/states/", STATES),
        ("/api/v1/workspaces/ws/projects/p1/labels/", LABELS),
        ("/api/v1/workspaces/ws/projects/p1/work-items/", ITEMS),
    ] {
        server
            .mock("GET", path)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
    }

    let out = run(
        &["wi", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let row = &parsed[0];
    assert_eq!(row["sequence_id"], "DEMO-1");
    assert_eq!(row["name"], "A task");
    assert_eq!(row["state_detail_name"], "Todo");
    assert_eq!(row["label_names"], "feat");
    assert_eq!(row["assignee_names"], "B"); // member full_name = first+last
    assert!(
        row["web_url"]
            .as_str()
            .unwrap_or("")
            .contains("/ws/projects/p1/issues/wi1/")
    );
}

#[test]
fn invalid_state_exits_5_with_available_list() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/states/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(STATES)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"p1","name":"Demo","identifier":"DEMO"}"#)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/labels/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(LABELS)
        .create();
    // create_work_item must not be reached
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/work-items/")
        .with_status(201)
        .with_body(ITEM)
        .expect(0)
        .create();

    let out = run(
        &[
            "wi",
            "create",
            "x",
            "-p",
            "Demo",
            "--state",
            "NoSuch",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("State 'NoSuch' not found"), "{err}");
    assert!(err.contains("Available: Todo"), "{err}");
}

#[test]
fn wi_update_parent_patches_and_reads_the_parent_back() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_TWO);
    // Created before the body-agnostic update mock so the parent body match
    // wins; mockito serves the first matching mock that still needs hits.
    let parent_patch = server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .match_body(mockito::Matcher::Json(serde_json::json!({"parent": "wi2"})))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"wi1","parent":"wi2"}"#)
        .expect(1)
        .create();
    let update_patch = server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1",
        )
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"wi1","name":"A task","sequence_id":1,"parent":"wi2","state":"s1","project":"p1"}"#)
        .create();

    let out = run(
        &[
            "wi",
            "update",
            "DEMO-1",
            "-p",
            "Demo",
            "--parent",
            "DEMO-2",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["parent"], "wi2");
    parent_patch.assert();
    update_patch.assert();
}

#[test]
fn wi_update_cross_project_parent_exits_5_without_writing() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_TWO);
    let any_patch = server
        .mock("PATCH", mockito::Matcher::Any)
        .with_status(200)
        .expect(0)
        .create();

    let out = run(
        &[
            "wi",
            "update",
            "DEMO-1",
            "-p",
            "Demo",
            "--parent",
            "OTHER-9",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("belongs to project 'OTHER'"), "{err}");
    any_patch.assert();
}

#[test]
fn wi_update_self_parent_exits_5_without_writing() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_TWO);
    let any_patch = server
        .mock("PATCH", mockito::Matcher::Any)
        .with_status(200)
        .expect(0)
        .create();

    let out = run(
        &[
            "wi",
            "update",
            "DEMO-1",
            "-p",
            "Demo",
            "--parent",
            "DEMO-1",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("own parent"), "{err}");
    any_patch.assert();
}

/// The PLANECLI-84 hijack shape: one item whose *title* contains the literal
/// `DEMO-99`, an identifier that does not exist. Nothing resolves `DEMO-99` to
/// that item — not the read path, and not the writes that share the resolver.
const ITEMS_WITH_LOOKALIKE_TITLE: &str = r#"{"results":[{"id":"wi1","name":"see DEMO-99 for the fix","sequence_id":1,"project":"p1"}],"next_cursor":null,"next_page_results":false}"#;

#[test]
fn wi_show_missing_identifier_exits_3_instead_of_landing_on_a_title_match() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_WITH_LOOKALIKE_TITLE);

    let out = run(
        &[
            "wi",
            "show",
            "DEMO-99",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Work item not found: DEMO-99"), "{err}");
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("wi1"),
        "must not print the item whose title merely contains the string"
    );
}

#[test]
fn wi_update_missing_identifier_exits_3_without_writing() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_WITH_LOOKALIKE_TITLE);
    let mutating_patch = server
        .mock("PATCH", mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create();
    let mutating_post = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create();

    let out = run(
        &[
            "wi",
            "update",
            "DEMO-99",
            "-p",
            "Demo",
            "--name",
            "renamed",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Work item not found: DEMO-99"), "{err}");
    mutating_patch.assert();
    mutating_post.assert();
}

#[test]
fn wi_create_missing_parent_identifier_exits_3_without_writing() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_WITH_LOOKALIKE_TITLE);
    let mutating_post = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create();
    let mutating_patch = server
        .mock("PATCH", mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create();

    let out = run(
        &[
            "wi",
            "create",
            "a child",
            "-p",
            "Demo",
            "--parent",
            "DEMO-99",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Work item not found: DEMO-99"), "{err}");
    mutating_post.assert();
    mutating_patch.assert();
}

#[test]
fn wi_show_lists_sub_issues() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_TWO);
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .create();

    let out = run(
        &["wi", "show", "DEMO-1", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let sub_issues = parsed["sub_issues"].as_array().expect("sub_issues array");
    assert_eq!(sub_issues.len(), 1);
    assert_eq!(sub_issues[0]["id"], "wi2");
    assert_eq!(sub_issues[0]["sequence_id"], "DEMO-2");
    assert_eq!(sub_issues[0]["name"], "A parent");
}

#[test]
fn relations_add_rejects_an_unknown_type_before_any_request() {
    let server = Server::new();
    let out = run(
        &[
            "relations",
            "add",
            "DEMO-1",
            "--type",
            "blocks",
            "--to",
            "DEMO-2",
            "--json",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Unknown relation type 'blocks'"), "{err}");
    assert!(err.contains("blocked_by"), "{err}");
}

#[test]
fn relations_add_posts_the_type_and_resolved_targets() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_TWO);
    let post = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/relations/",
        )
        .match_body(mockito::Matcher::Json(serde_json::json!({
            "relation_type": "blocking",
            "issues": ["wi2"],
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"blocking":[{"project_id":"p1","issue_id":"wi2"}]}"#)
        .expect(1)
        .create();

    let out = run(
        &[
            "relations",
            "add",
            "DEMO-1",
            "--type",
            "blocking",
            "--to",
            "DEMO-2",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["blocking"][0]["issue_id"], "wi2");
    post.assert();
}

#[test]
fn relations_ls_prints_the_raw_buckets() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS_TWO);
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/relations/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"blocking":[],"blocked_by":[{"project_id":"p1","issue_id":"wi2"}],"duplicate":[],"relates_to":[],"start_after":[],"start_before":[],"finish_after":[],"finish_before":[]}"#,
        )
        .create();

    let out = run(
        &[
            "relations",
            "ls",
            "DEMO-1",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["blocked_by"][0]["issue_id"], "wi2");
}

#[test]
fn relations_rm_reports_the_missing_endpoint() {
    let server = Server::new();
    let out = run(
        &[
            "relations",
            "rm",
            "DEMO-1",
            "--type",
            "blocking",
            "--to",
            "DEMO-2",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no relations delete endpoint"), "{err}");
}

// ---------------------------------------------------------------------------
// comment create / update — --body-file, --body-md-file, and `-` for stdin.
//
// Each test proves one slice of the PLANECLI-73 contract:
//   - the four body flags are mutually exclusive (clap parse error, no HTTP),
//   - a missing / empty file produces Validation + exit 5 with the flag in
//     the hint,
//   - the file / stdin path renders byte-identical `comment_html` to the
//     inline `--body` form (the equivalence to `--body-md` includes the
//     `![alt](<uuid>)` image path),
//   - a body larger than the 131072-byte single-argument cap posts cleanly
//     through `--body-file`.
// ---------------------------------------------------------------------------

const COMMENT_BODY: &str = "see `code` <here> & 中文\n\nsecond paragraph";
const COMMENT_BODY_MD: &str =
    "# heading\n\n- a\n- b\n\nsee ![shot](11111111-2222-3333-4444-555555555555)";
const COMMENT_RESPONSE: &str = r#"{"id":"c1","comment_html":"","comment_stripped":"","actor":"u1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#;

/// Mock the reads every `comment` command makes on the way to its write —
/// `locate_issue` lists projects, the project detail, the project's items,
/// and the readback of the comment needs the member map.
fn mock_comment_reads(server: &mut Server, items: &str) {
    mock_project_reads(server, items);
}

/// Write `body` to a unique temp file and return the path.
fn write_temp_body(body: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "pbot-comment-test-{}-{}.txt",
        std::process::id(),
        COMMENT_TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ));
    std::fs::write(&path, body).expect("failed to write temp body");
    path
}

static COMMENT_TEMP_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[test]
fn comment_create_body_file_matches_inline_body_byte_for_byte() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let expected_html = pbotcli_html::body_to_html(COMMENT_BODY);
    let post = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/comments/",
        )
        .match_body(Matcher::Json(
            serde_json::json!({"comment_html": expected_html}),
        ))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(COMMENT_RESPONSE)
        .expect(2)
        .create();

    let inline = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body",
            COMMENT_BODY,
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        inline.status.success(),
        "inline: stderr: {}",
        String::from_utf8_lossy(&inline.stderr)
    );

    let file = write_temp_body(COMMENT_BODY);
    let from_file = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            file.to_string_lossy().as_ref(),
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        from_file.status.success(),
        "from_file: stderr: {}",
        String::from_utf8_lossy(&from_file.stderr)
    );

    post.assert();
    std::fs::remove_file(&file).ok();
}

#[test]
fn comment_create_body_md_file_matches_inline_body_md_with_image() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let expected_html = pbotcli_html::md_to_html(COMMENT_BODY_MD);
    let post = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/comments/",
        )
        .match_body(Matcher::Json(
            serde_json::json!({"comment_html": expected_html}),
        ))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(COMMENT_RESPONSE)
        .expect(2)
        .create();

    let inline = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-md",
            COMMENT_BODY_MD,
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        inline.status.success(),
        "inline: stderr: {}",
        String::from_utf8_lossy(&inline.stderr)
    );

    let file = write_temp_body(COMMENT_BODY_MD);
    let from_file = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-md-file",
            file.to_string_lossy().as_ref(),
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        from_file.status.success(),
        "from_file: stderr: {}",
        String::from_utf8_lossy(&from_file.stderr)
    );

    post.assert();
    std::fs::remove_file(&file).ok();
}

#[test]
fn comment_create_two_body_flags_rejected_at_parse_with_no_http() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let post = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(201)
        .expect(0)
        .create();

    let out = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body",
            "x",
            "--body-md",
            "y",
            "--json",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cannot be used with"),
        "stderr did not name the conflicting flags: {err}"
    );
    post.assert();

    let out2 = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            "/tmp/whatever",
            "--body-md-file",
            "/tmp/whatever2",
            "--json",
        ],
        &server.url(),
    );
    assert_eq!(out2.status.code(), Some(2));
    post.assert();
}

#[test]
fn comment_create_missing_body_file_exits_5_with_flag_in_message() {
    let server = Server::new();
    let out = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            "/no/such/file/pbot-missing.txt",
            "--json",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--body-file"),
        "error message should name --body-file: {err}"
    );
    assert!(err.contains("/no/such/file"), "{err}");
}

#[test]
fn comment_create_empty_body_file_exits_5() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let post = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(201)
        .expect(0)
        .create();
    let file = write_temp_body("");

    let out = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            file.to_string_lossy().as_ref(),
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("empty"), "{err}");
    assert!(err.contains("--body-file"), "{err}");
    post.assert();
    std::fs::remove_file(&file).ok();
}

#[test]
fn comment_create_non_utf8_body_file_exits_5_with_flag_in_message() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let post = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(201)
        .expect(0)
        .create();
    // `\xff\xfe` is not valid UTF-8 — `std::fs::read_to_string` returns
    // `InvalidData` ("stream did not contain valid UTF-8"), which the helper
    // wraps in `PlaneError::validation_with_hint` so the user sees a clear
    // validation message naming the flag.
    let file = write_temp_body("");

    {
        use std::io::Write;
        let mut f = std::fs::File::create(&file).expect("create");
        f.write_all(b"\xff\xfe").expect("write");
    }

    let out = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            file.to_string_lossy().as_ref(),
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--body-file"),
        "error message should name --body-file: {err}"
    );
    assert!(
        err.to_lowercase().contains("utf-8"),
        "error message should mention the UTF-8 cause: {err}"
    );
    post.assert();
    std::fs::remove_file(&file).ok();
}

#[test]
fn comment_create_oversized_body_succeeds_via_file() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    // ~200 KB body — above the kernel single-argument cap (MAX_ARG_STRLEN,
    // 131072 bytes on Linux) where argv-based --body is impossible.
    let body: String = "a".repeat(200_000);
    let post = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/comments/",
        )
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(COMMENT_RESPONSE)
        .expect(1)
        .create();

    let file = write_temp_body(&body);
    let out = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            file.to_string_lossy().as_ref(),
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    post.assert();
    std::fs::remove_file(&file).ok();
}

#[test]
fn comment_create_body_from_stdin_matches_inline_body() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let expected_html = pbotcli_html::body_to_html(COMMENT_BODY);
    let post = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/comments/",
        )
        .match_body(Matcher::Json(
            serde_json::json!({"comment_html": expected_html}),
        ))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(COMMENT_RESPONSE)
        .expect(1)
        .create();

    let out = run_with_stdin(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body",
            "-",
            "--json",
            "--no-cache",
        ],
        &server.url(),
        COMMENT_BODY.as_bytes(),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    post.assert();
}

#[test]
fn comment_create_empty_body_stdin_hints_name_the_file_flag() {
    // An empty stdin reports the **file** flag (`--body-file` for the
    // `--body` route, `--body-md-file` for the `--body-md` route) — the
    // inline flags take body text, so suggesting them as a file path would
    // be misleading. Run both routes and the matching file route to pin the
    // mapping and the unchanged file wording.
    let file = write_temp_body("");

    for (route_flag, file_flag) in [
        ("--body", "--body-file"),
        ("--body-md", "--body-md-file"),
        ("--body-file", "--body-file"),
        ("--body-md-file", "--body-md-file"),
    ] {
        let mut server = Server::new();
        mock_comment_reads(&mut server, ITEMS);
        let post = server
            .mock("POST", mockito::Matcher::Any)
            .with_status(201)
            .expect(0)
            .create();

        let mut args: Vec<&str> = vec!["comment", "create", "DEMO-1", "-p", "Demo", route_flag];
        let stdin_or_path;
        let mut stdin_data: Option<Vec<u8>> = None;
        if route_flag == "--body" || route_flag == "--body-md" {
            stdin_or_path = "-".to_string();
            stdin_data = Some(Vec::new());
        } else {
            stdin_or_path = file.to_string_lossy().into_owned();
        }
        args.push(&stdin_or_path);
        args.push("--json");
        args.push("--no-cache");

        let out = if let Some(data) = stdin_data {
            run_with_stdin(&args, &server.url(), &data)
        } else {
            run(&args, &server.url())
        };
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            out.status.code(),
            Some(5),
            "route {route_flag}: expected exit 5, got {:?}; stderr: {stderr}",
            out.status.code()
        );
        assert!(
            stderr.contains(file_flag),
            "route {route_flag}: hint should name {file_flag}; stderr: {stderr}"
        );
        post.assert();
    }
}

#[test]
fn comment_create_dot_slash_dash_is_a_literal_file_not_stdin() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let post = server
        .mock("POST", mockito::Matcher::Any)
        .with_status(201)
        .with_body(COMMENT_RESPONSE)
        .expect(0)
        .create();

    // `./-` is a path, not stdin — there is no file named `-`, so it should
    // fail with Validation (exit 5), not consume stdin.
    let out = run(
        &[
            "comment",
            "create",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            "./-",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("./-"), "{err}");
    assert!(err.contains("--body-file"), "{err}");
    post.assert();
}

#[test]
fn comment_update_supports_body_file_for_symmetry() {
    let mut server = Server::new();
    mock_comment_reads(&mut server, ITEMS);
    let expected_html = pbotcli_html::body_to_html(COMMENT_BODY);
    let post = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/work-items/wi1/comments/c1",
        )
        .match_body(Matcher::Json(
            serde_json::json!({"comment_html": expected_html}),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(COMMENT_RESPONSE)
        .expect(1)
        .create();

    let file = write_temp_body(COMMENT_BODY);
    let out = run(
        &[
            "comment",
            "update",
            "c1",
            "--issue",
            "DEMO-1",
            "-p",
            "Demo",
            "--body-file",
            file.to_string_lossy().as_ref(),
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    post.assert();
    std::fs::remove_file(&file).ok();
}

/// A service principal answers with `name` and `type`, and no person fields.
const ME_SERVICE_PRINCIPAL: &str =
    r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal"}"#;
const ME_HUMAN: &str = r#"{"id":"u9","display_name":"Jicai Liu","first_name":"Jicai","last_name":"Liu","email":"j@x"}"#;

fn mock_me(server: &mut Server, body: &str) {
    server
        .mock("GET", "/api/v1/users/me/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create();
}

#[test]
fn whoami_json_keeps_the_service_principal_identity() {
    let mut server = Server::new();
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);
    let out = run(&["whoami", "--json"], &server.url());
    assert_eq!(out.status.code(), Some(0));
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["name"], "kimicode-bot");
    assert_eq!(parsed["type"], "service_principal");
    // The existing person keys stay in the object, unchanged and unset.
    for key in ["display_name", "first_name", "last_name", "email"] {
        assert!(parsed[key].is_null(), "{key} = {}", parsed[key]);
    }
}

#[test]
fn whoami_table_names_the_service_identity() {
    let mut server = Server::new();
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);
    let out = run(&["whoami"], &server.url());
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("kimicode-bot"), "{err}");
    assert!(err.contains("service_principal"), "{err}");
    // No person fields came back, so no person rows are invented.
    for field in ["display_name", "first_name", "last_name", "email"] {
        assert!(!err.contains(field), "{field} rendered in {err}");
    }
}

#[test]
fn whoami_table_keeps_the_human_rows() {
    let mut server = Server::new();
    mock_me(&mut server, ME_HUMAN);
    let out = run(&["whoami"], &server.url());
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Jicai Liu"), "{err}");
    for field in ["display_name", "first_name", "last_name", "email"] {
        assert!(err.contains(field), "{field} missing from {err}");
    }
}

#[test]
fn doc_show_with_project_unaffected_by_workspace_hint_change() {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS);
    let page_uuid = "5963a34f-1038-43b2-90aa-cc2b183a30e2";
    let project_page = server
        .mock(
            "GET",
            format!("/api/v1/workspaces/ws/projects/p1/pages/{page_uuid}/").as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"5963a34f-1038-43b2-90aa-cc2b183a30e2","name":"Spec","description_html":"<p>body</p>","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#)
        .create();
    let any_other = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create();

    let out = run(
        &[
            "doc",
            "show",
            page_uuid,
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["id"], page_uuid);
    assert_eq!(parsed["name"], "Spec");
    project_page.assert();
    any_other.assert();
}

#[test]
fn doc_show_without_project_hints_at_minus_p_on_workspace_404() {
    let mut server = Server::new();
    let workspace_page_get = server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/pages/7880a16b-2067-47e1-8bba-c75246545b6d/",
        )
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"Not found"}"#)
        .expect_at_least(1)
        .create();
    let any_other = server
        .mock("GET", mockito::Matcher::Any)
        .with_status(500)
        .expect(0)
        .create();

    let out = run(
        &[
            "doc",
            "show",
            "7880a16b-2067-47e1-8bba-c75246545b6d",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Resource not found (HTTP 404)"),
        "expected the underlying 404 message to be preserved: {err}"
    );
    assert!(
        err.contains("Hint:"),
        "expected a hint line on stderr: {err}"
    );
    assert!(
        err.contains("-p <project>"),
        "expected the hint to name the -p flag: {err}"
    );
    workspace_page_get.assert();
    any_other.assert();
}

/// A `doc` command that cannot work without a project must be rejected locally:
/// exit 5, a hint naming `-p`, and no HTTP request of any method at all. The
/// server has no workspace-level pages route, so a request here could only ever
/// come back 404.
fn assert_doc_requires_project(args: &[&str]) {
    let mut server = Server::new();
    let mut negatives = vec![
        server
            .mock("GET", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
        server
            .mock("POST", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
        server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
        server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
    ];

    let mut full: Vec<&str> = args.to_vec();
    full.extend_from_slice(&["--json", "--no-cache"]);
    let out = run(&full, &server.url());
    assert_eq!(
        out.status.code(),
        Some(5),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Hint:"), "expected a hint line: {err}");
    assert!(
        err.contains("-p <project>"),
        "expected the hint to name -p: {err}"
    );
    for mock in &mut negatives {
        mock.assert();
    }
}

#[test]
fn doc_list_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&["doc", "list"]);
}

#[test]
fn doc_create_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&["doc", "create", "--title", "Spec", "--content-raw", "body"]);
}

#[test]
fn doc_update_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&["doc", "update", "Spec", "--title", "New"]);
}

#[test]
fn doc_archive_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&["doc", "archive", "Spec"]);
}

/// The `--dry-run` previews are not exempt from the `-p` requirement: a plan for
/// an action that could never run is not a useful preview. Before this branch
/// they printed the plan without `-p` (and the `create` one without any request
/// at all), so this is a user-visible change, not just a refactor.
#[test]
fn doc_create_dry_run_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&[
        "doc",
        "create",
        "--title",
        "Spec",
        "--content-raw",
        "body",
        "--dry-run",
    ]);
}

#[test]
fn doc_update_dry_run_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&["doc", "update", "Spec", "--title", "New", "--dry-run"]);
}

/// One page in the demo project, so a name-based `doc update` can resolve it.
const PROJECT_PAGES: &str = r#"{"results":[{"id":"pg1","name":"Spec","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;

/// With `-p` the preview still works: the plan is printed, the exit code is 0,
/// and no mutating request is sent.
fn assert_dry_run_preview(args: &[&str], expect_line: &str) {
    let mut server = Server::new();
    mock_project_reads(&mut server, ITEMS);
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/pages/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECT_PAGES)
        .create();
    let mut negatives = vec![
        server
            .mock("POST", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
        server
            .mock("PATCH", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
        server
            .mock("DELETE", mockito::Matcher::Any)
            .with_status(500)
            .expect(0)
            .create(),
    ];

    let mut full: Vec<&str> = args.to_vec();
    full.extend_from_slice(&["--json", "--no-cache"]);
    let out = run(&full, &server.url());
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("dry run: no writes will be performed"),
        "expected the preview header: {err}"
    );
    assert!(
        err.contains(expect_line),
        "expected {expect_line:?} in the preview: {err}"
    );
    assert!(
        err.contains("summary: 0 to upload, 0 kept, 0 error(s), 0 unprocessed"),
        "expected the summary line: {err}"
    );
    for mock in &mut negatives {
        mock.assert();
    }
}

#[test]
fn doc_create_dry_run_with_project_still_prints_the_plan() {
    assert_dry_run_preview(
        &[
            "doc",
            "create",
            "--title",
            "Spec",
            "-p",
            "Demo",
            "--content-raw",
            "body",
            "--dry-run",
        ],
        "project=p1 page=(new page \"Spec\")",
    );
}

#[test]
fn doc_update_dry_run_with_project_still_prints_the_plan() {
    assert_dry_run_preview(
        &[
            "doc",
            "update",
            "Spec",
            "-p",
            "Demo",
            "--title",
            "New",
            "--dry-run",
        ],
        "project=p1 page=pg1 (Spec)",
    );
}

#[test]
fn doc_delete_without_project_exits_5_without_a_request() {
    assert_doc_requires_project(&["doc", "delete", "Spec"]);
}

// ---------------------------------------------------------------------------
// Service-principal identity, `me`, and the cross-project scan
// (PLANECLI-79 / PLANECLI-92).
// ---------------------------------------------------------------------------

/// Two projects of which the credential may read only the second. That is the
/// shape a service principal hits: Plane grants it per project, so every
/// project without a grant answers 403.
const PROJECTS_ONE_UNREADABLE: &str = r#"{"results":[
  {"id":"p-locked","name":"Locked","identifier":"LOCKED","created_at":"2026-01-01T00:00:00Z"},
  {"id":"p1","name":"Demo","identifier":"DEMO","created_at":"2026-01-01T00:00:00Z"}
],"next_cursor":null,"next_page_results":false}"#;

const NO_ITEMS: &str = r#"{"results":[],"next_cursor":null,"next_page_results":false}"#;

/// The item is assigned to the service principal's *shadow bot* member
/// (`u-shadow`), not to the SP's own id (`sp1`). Resolving `me` to the SP id
/// filters this row out, which is the bug: `--assignee me` matched nothing.
const ITEMS_ASSIGNED_TO_THE_SHADOW_BOT: &str = r#"{"results":[{"id":"wi1","name":"A task","sequence_id":1,"priority":"high","state":"s1","labels":["l1"],"assignees":["u-shadow"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;

/// The same row, assigned to a member id given in UUID form (for the paths that
/// take an explicit id rather than `me`).
const ITEMS_ASSIGNED_TO_A_UUID: &str = r#"{"results":[{"id":"wi1","name":"A task","sequence_id":1,"priority":"high","state":"s1","labels":["l1"],"assignees":["17af336a-126d-4f0f-a067-25e7d90a2d78"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;

/// The member list for an SP credential: the human members plus the shadow bot
/// user the server composes for the SP (`svc-<name> (bot)` / `svc-<name>`).
const MEMBERS_WITH_SHADOW_BOT: &str = r#"[
  {"id":"u-human","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu","email":"j@x"},
  {"id":"u-shadow","display_name":"svc-kimicode-bot (bot)","first_name":"svc-kimicode-bot","last_name":"Bot","email":"ai+hash@x"}
]"#;

/// Members with no shadow bot row: the SP cannot be resolved to a member.
const MEMBERS_WITHOUT_SHADOW_BOT: &str = r#"[
  {"id":"u-human","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu","email":"j@x"}
]"#;

/// Forward-compatible `/users/me/` payload naming the SP's human owner.
const ME_SP_WITH_OWNER: &str = r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":{"id":"u-human","display_name":"Jicai Liu"}}"#;

fn mock_list(server: &mut Server, path: &str, body: &str) {
    server
        .mock("GET", path)
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create();
}

#[test]
fn wi_show_without_project_skips_a_project_it_cannot_read() {
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    // The project the credential has no grant for.
    let denied = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p-locked/work-items/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"detail":"Service principal 'Kimicode' has no active grant for project p-locked."}"#,
        )
        .create();
    // ... and the readable one that holds the item.
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/wi1",
        ITEM,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(
        &[
            "wi",
            "show",
            "A task",
            "--no-comments",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "a 403 on one project must not abort the scan — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    denied.assert();
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["sequence_id"], "DEMO-1");
    // The scan was partial, so the user is told which project it could not read.
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Locked"), "skipped project unnamed: {err}");
}

#[test]
fn wi_show_without_project_names_the_skipped_project_when_nothing_matched() {
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p-locked/work-items/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        NO_ITEMS,
    );
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(
        &["wi", "show", "nowhere-99", "--json", "--no-cache"],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Work item not found: nowhere-99"), "{err}");
    assert!(
        err.contains("Locked") && err.contains("-p"),
        "the hint must name the skipped project and the flag that targets one: {err}"
    );
}

#[test]
fn wi_show_without_project_still_fails_loudly_on_a_non_access_error() {
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    // A 500 is not a per-project access failure: it must abort, not be skipped.
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p-locked/work-items/")
        .match_query(Matcher::Any)
        .with_status(500)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"upstream boom"}"#)
        .create();
    let readable = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/work-items/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEMS)
        .expect(0)
        .create();

    let out = run(
        &["wi", "show", "A task", "--json", "--no-cache"],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(4));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("HTTP 500"), "{err}");
    readable.assert();
    assert!(
        String::from_utf8_lossy(&out.stdout).is_empty(),
        "a failed scan must print nothing on stdout"
    );
}

#[test]
fn assignee_me_under_a_service_principal_resolves_to_its_shadow_bot_member() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS_ASSIGNED_TO_THE_SHADOW_BOT,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        MEMBERS_WITH_SHADOW_BOT,
    );
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let rows = parsed.as_array().expect("an array of work items");
    assert_eq!(
        rows.len(),
        1,
        "the shadow bot's item must survive the filter"
    );
    assert_eq!(rows[0]["sequence_id"], "DEMO-1");
}

#[test]
fn assignee_me_under_a_service_principal_without_a_shadow_bot_fails_with_a_hint() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS_ASSIGNED_TO_THE_SHADOW_BOT,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        MEMBERS_WITHOUT_SHADOW_BOT,
    );
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    // The failure names the credential kind and the SP, and points at the next
    // command — never an empty list.
    assert!(err.contains("service principal"), "{err}");
    assert!(err.contains("kimicode-bot"), "{err}");
    assert!(err.contains("pbot user ls"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).is_empty(), "{err}");
}

#[test]
fn wi_assign_me_under_a_service_principal_names_the_identity() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        MEMBERS_WITH_SHADOW_BOT,
    );
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);
    // The write must carry the shadow bot's member id, not the SP's id.
    server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .match_body(Matcher::Json(
            serde_json::json!({"assignees": ["u-shadow"]}),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();

    let out = run(
        &["wi", "assign", "DEMO-1", "-p", "Demo", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "Work item assigned to kimicode-bot (service principal) as its shadow bot member \
             'svc-kimicode-bot (bot)' (member id u-shadow)."
        ),
        "{err}"
    );
}

#[test]
fn whoami_renders_the_owner_when_the_payload_names_one() {
    let mut server = Server::new();
    mock_me(&mut server, ME_SP_WITH_OWNER);
    let out = run(&["whoami", "--no-cache"], &server.url());
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("kimicode-bot (SP owned by Jicai Liu)"),
        "{err}"
    );
    // `--json` keeps the raw payload, owner included.
    let out = run(&["whoami", "--json", "--no-cache"], &server.url());
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["owner"]["display_name"], "Jicai Liu");
}

#[test]
fn whoami_degrades_to_the_plain_marker_without_an_owner_name() {
    let mut server = Server::new();
    // What the server sends today: no owner at all.
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);
    let out = run(&["whoami", "--no-cache"], &server.url());
    assert_eq!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("kimicode-bot (service principal)"), "{err}");
    assert!(!err.contains("SP owned by"), "{err}");
}

#[test]
fn wi_ls_without_project_skips_a_project_it_cannot_read() {
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    // The listing spans projects, so the unreadable one must not abort it.
    let denied = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p-locked/work-items/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(&["wi", "ls", "--json", "--no-cache"], &server.url());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    denied.assert();
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let rows = parsed.as_array().expect("an array of work items");
    assert_eq!(
        rows.len(),
        1,
        "the readable project's row must still be listed"
    );
    assert_eq!(rows[0]["sequence_id"], "DEMO-1");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Locked"), "skipped project unnamed: {err}");
}

#[test]
fn wi_show_without_project_skips_a_project_that_answers_404_for_it() {
    // A project the credential cannot see at all answers 404 rather than 403;
    // it is skipped the same way.
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p-locked/work-items/")
        .match_query(Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"Not found."}"#)
        .create();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/wi1",
        ITEM,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(
        &[
            "wi",
            "show",
            "A task",
            "--no-comments",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "a 404 on one project must not abort the scan — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["sequence_id"], "DEMO-1");
}

#[test]
fn wi_ls_assignee_me_human_credential_output_is_unchanged() {
    // The identity work is service-principal-only: a human `me` that matches
    // nothing prints no note at all, exactly as before.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);
    mock_me(&mut server, ME_HUMAN);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("note:"),
        "human path must stay unchanged: {err}"
    );
    assert!(!err.contains("service principal"), "{err}");
}

#[test]
fn wi_ls_assignee_me_empty_result_says_what_it_resolved_to_and_what_to_run() {
    // The service-principal case is the opposite: an unattended caller must be
    // able to read the principal, the shadow bot member it mapped to, that
    // member's id, and the next command — from the output alone.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    // ITEMS is assigned to u1, not to the shadow bot member u-shadow.
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        MEMBERS_WITH_SHADOW_BOT,
    );
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    // who am I
    assert!(err.contains("kimicode-bot (service principal)"), "{err}");
    // why `me` maps where it does, and to which member (name + UUID)
    assert!(
        err.contains("ServicePrincipal id is not a workspace member row"),
        "{err}"
    );
    assert!(
        err.contains("'svc-kimicode-bot (bot)' (member id u-shadow)"),
        "{err}"
    );
    // what to run next
    assert!(err.contains("pbot wi ls -p <project> --assignee"), "{err}");
    assert!(err.contains("pbot user ls"), "{err}");
}

// ---------------------------------------------------------------------------
// Round 3: the identity note on every `me` write, the row's own member name,
// the guard's real scope, and the member-list degradation.
// ---------------------------------------------------------------------------

/// The SP credential plus a member list holding its shadow bot row.
fn mock_sp_credential(server: &mut Server) {
    mock_list(
        server,
        "/api/v1/workspaces/ws/members/",
        MEMBERS_WITH_SHADOW_BOT,
    );
    mock_me(server, ME_SERVICE_PRINCIPAL);
}

/// The project reads for one project, with the SP credential's member list (its
/// shadow bot row) and `/users/me/` — the whole setup a `me` write needs.
///
/// Built here rather than layered on [`mock_project_reads`] because mockito
/// serves the *first* mock that matches a path: a second `/members/` mock would
/// never be reached, and the shadow bot row would be invisible.
fn mock_sp_project_reads(server: &mut Server, items: &str) {
    mock_list(server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(server, "/api/v1/workspaces/ws/projects/p1/", PROJECT);
    mock_list(
        server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        items,
    );
    mock_list(server, "/api/v1/workspaces/ws/projects/p1/states/", STATES);
    mock_list(server, "/api/v1/workspaces/ws/projects/p1/labels/", LABELS);
    mock_sp_credential(server);
}

#[test]
fn wi_create_assignee_me_under_a_service_principal_prints_the_identity_note() {
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS);
    // The write must carry the shadow bot's member id, not the SP's id.
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/work-items/")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"assignees": ["u-shadow"]}),
        ))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();

    let out = run(
        &[
            "wi",
            "create",
            "New task",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "Work item assigned to kimicode-bot (service principal) as its shadow bot member \
             'svc-kimicode-bot (bot)' (member id u-shadow)."
        ),
        "{err}"
    );
}

#[test]
fn wi_create_assignee_explicit_name_prints_no_identity_note() {
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/work-items/")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();

    let out = run(
        &[
            "wi",
            "create",
            "New task",
            "-p",
            "Demo",
            "--assignee",
            "e25ef580-126d-4f0f-a067-25e7d90a2d78",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    // An explicit member id is not `me`: no identity note, no service-principal
    // wording anywhere in the output.
    assert!(!err.contains("Work item assigned to"), "{err}");
    assert!(!err.contains("service principal"), "{err}");
}

#[test]
fn wi_update_assignee_me_under_a_service_principal_prints_the_identity_note() {
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS);
    server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .match_body(Matcher::PartialJson(
            serde_json::json!({"assignees": ["u-shadow"]}),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();

    let out = run(
        &[
            "wi",
            "update",
            "DEMO-1",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "Work item assigned to kimicode-bot (service principal) as its shadow bot member \
             'svc-kimicode-bot (bot)' (member id u-shadow)."
        ),
        "{err}"
    );
}

#[test]
fn wi_update_assignee_explicit_name_prints_no_identity_note() {
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS);
    server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();

    let out = run(
        &[
            "wi",
            "update",
            "DEMO-1",
            "-p",
            "Demo",
            "--assignee",
            "e25ef580-126d-4f0f-a067-25e7d90a2d78",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("Work item assigned to"), "{err}");
    assert!(!err.contains("service principal"), "{err}");
}

#[test]
fn wi_assign_explicit_name_prints_no_identity_note() {
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS);
    server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/work-items/wi1")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ITEM)
        .expect(1)
        .create();

    let out = run(
        &[
            "wi",
            "assign",
            "DEMO-1",
            "-p",
            "Demo",
            "--assignee",
            "e25ef580-126d-4f0f-a067-25e7d90a2d78",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    // The pre-existing line, unchanged for an explicit reference.
    assert!(err.contains("Work item assigned to "), "{err}");
    assert!(!err.contains("service principal"), "{err}");
    assert!(!err.contains("shadow bot"), "{err}");
}

#[test]
fn assignee_me_note_uses_the_matched_member_rows_own_name() {
    // The row's `display_name` is what `pbot user ls` shows, so the note must
    // quote it rather than a name re-derived from the principal — here the row's
    // name differs from the server's composition, and `first_name` is what
    // carries the bot marker.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        r#"[{"id":"u-shadow","display_name":"Kimicode bot (legacy)","first_name":"svc-kimicode-bot","last_name":"Bot"}]"#,
    );
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("'Kimicode bot (legacy)' (member id u-shadow)"),
        "the row's own name must be quoted: {err}"
    );
    assert!(
        !err.contains("svc-kimicode-bot (bot)"),
        "the composed name must not replace the row's name: {err}"
    );
}

#[test]
fn wi_ls_without_project_assignee_me_skips_a_denied_project() {
    // The cross-project skip and the `me` filter together: the denied project
    // must not hide the readable one's row for the shadow bot member.
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p-locked/work-items/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS_ASSIGNED_TO_THE_SHADOW_BOT,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_sp_credential(&mut server);

    let out = run(
        &["wi", "ls", "--assignee", "me", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let rows = parsed.as_array().expect("an array of work items");
    assert_eq!(rows.len(), 1, "the readable project's row must survive");
    assert_eq!(rows[0]["sequence_id"], "DEMO-1");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Locked"), "skipped project unnamed: {err}");
}

#[test]
fn wi_ls_states_denial_is_not_reported_as_a_project_skip() {
    // Only the work-item read decides a skip. A project whose items are readable
    // but whose states are not must fail loudly instead of being dropped from
    // the listing behind a "no read access" warning.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/states/")
        .match_query(Matcher::Any)
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"Not found."}"#)
        .create();
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(
        &["wi", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("no read access"),
        "a states failure must not be reported as a project denial: {err}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).is_empty(), "{err}");
}

#[test]
fn wi_ls_degrades_the_assignee_names_when_the_member_list_is_denied() {
    // Secondary enrichment (ADR-0006): an unreadable member list blanks the
    // assignee names, names the call that was denied, and still lists the items.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();

    let out = run(
        &["wi", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(1));
    // No member map to resolve through: the id stands in for the name (the
    // pre-existing fallback), which the warning has to describe truthfully.
    assert_eq!(parsed[0]["assignee_names"], "u1");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("could not read the workspace member list"),
        "{err}"
    );
    // `render.rs` falls back to the id's first 8 characters, so the warning says
    // that rather than "their ids".
    assert!(
        err.contains("assignee names will show the member id's first 8 characters"),
        "{err}"
    );
    assert!(err.contains("pbot user ls"), "{err}");
    assert!(!err.contains("no read access to"), "{err}");
}

#[test]
fn wi_ls_assignee_me_hints_when_the_member_list_is_denied() {
    // The filter cannot be answered without the member list, and dropping it
    // would return rows the caller did not ask for: the failure carries the next
    // command instead of the bare API error.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Cannot filter by assignee 'me'"), "{err}");
    assert!(err.contains("pbot user ls"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).is_empty(), "{err}");
}

// ---------------------------------------------------------------------------
// Round 3 (cont.): truthful skip-warning shapes, and the id-reference path when
// the member list is unreadable.
// ---------------------------------------------------------------------------

#[test]
fn wi_ls_single_project_denial_does_not_advise_the_flag_that_was_used() {
    // `-p Demo` already targeted one project: the warning must not tell the
    // caller to "pass -p" again.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/work-items/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(
        &["wi", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no read access to Demo"), "{err}");
    assert!(!err.contains("pass -p"), "{err}");
    // The verb and the scanned count stay, so the note still says what happened.
    assert!(err.contains("listed 0 of 1 projects"), "{err}");
}

#[test]
fn wi_ls_all_projects_skipped_says_so() {
    // Every project denied: "0 of 2 projects" would be a strange way to say it.
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    for path in [
        "/api/v1/workspaces/ws/projects/p-locked/work-items/",
        "/api/v1/workspaces/ws/projects/p1/work-items/",
    ] {
        server
            .mock("GET", path)
            .match_query(Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"detail":"no active grant"}"#)
            .create();
    }
    mock_list(&mut server, "/api/v1/workspaces/ws/members/", MEMBERS);

    let out = run(&["wi", "ls", "--json", "--no-cache"], &server.url());
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout_of(&out).trim(), "[]");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("listed none of 2 projects"), "{err}");
    assert!(err.contains("Locked") && err.contains("Demo"), "{err}");
}

#[test]
fn assignee_me_with_a_nameless_principal_says_what_is_missing() {
    // An SP payload with no `name` cannot identify a shadow bot member; the
    // error must say so instead of quoting a member name built from nothing.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        MEMBERS_WITH_SHADOW_BOT,
    );
    mock_me(&mut server, r#"{"id":"sp1","type":"service_principal"}"#);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("answered /users/me/ without a name"), "{err}");
    assert!(err.contains("sp1"), "{err}");
    assert!(err.contains("pbot user ls"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).is_empty(), "{err}");
}

#[test]
fn wi_ls_assignee_me_prints_no_note_when_the_member_has_rows() {
    // The empty-result note is for an empty result: a filter that matched must
    // stay quiet, or the note would contradict the output.
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS_ASSIGNED_TO_THE_SHADOW_BOT);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("note:"), "{err}");
}

#[test]
fn wi_ls_assignee_with_an_id_degrades_when_the_member_list_is_denied() {
    // An explicit id needs no member list to be usable: an unreadable list costs
    // the display name, not the filter.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS_ASSIGNED_TO_A_UUID,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .create();

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "17af336a-126d-4f0f-a067-25e7d90a2d78",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let rows = parsed.as_array().expect("an array of work items");
    assert_eq!(rows.len(), 1, "the id filter must still run");
    assert_eq!(rows[0]["sequence_id"], "DEMO-1");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("could not read the workspace member list"),
        "{err}"
    );
}

// ---------------------------------------------------------------------------
// Round 4: lazy member reads, filter interaction, the full-name fallback, and
// the all-projects-skipped hint.
// ---------------------------------------------------------------------------

/// Rows that carry no assignees at all: nothing for the member map to resolve.
const ITEMS_WITHOUT_ASSIGNEES: &str = r#"{"results":[{"id":"wi1","name":"A task","sequence_id":1,"priority":"high","state":"s1","labels":["l1"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;

#[test]
fn wi_ls_does_not_read_the_member_list_when_nothing_needs_it() {
    // No `--assignee` and no row carries an assignee: the member list is not
    // enrichment anyone will read, so it must not be fetched — and a denial must
    // not produce a warning about names that were never going to be resolved.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS_WITHOUT_ASSIGNEES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    let members = server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"no active grant"}"#)
        .expect(0)
        .create();

    let out = run(
        &["wi", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    members.assert();
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(1));
    assert_eq!(parsed[0]["assignee_names"], "");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("member list"), "{err}");
}

#[test]
fn wi_ls_assignee_me_state_filter_emptying_the_list_prints_no_note() {
    // The empty-result note answers "the `me` resolution found nothing", not
    // "the listing is empty": when `--assignee me` matched and a later filter
    // emptied the rows, the note must stay away (reng filed the opposite claim).
    let mut server = Server::new();
    mock_sp_project_reads(&mut server, ITEMS_ASSIGNED_TO_THE_SHADOW_BOT);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--state",
            "NoSuchState",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(stdout_of(&out).trim(), "[]");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("note:"), "{err}");
}

#[test]
fn assignee_me_note_falls_back_to_the_rows_full_name() {
    // A row with no `display_name` is named by its full name — the middle
    // fallback of `member_display_name`, which the composition must not replace.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/states/",
        STATES,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/labels/",
        LABELS,
    );
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/members/",
        r#"[{"id":"u-shadow","first_name":"svc-kimicode-bot","last_name":"Bot"}]"#,
    );
    mock_me(&mut server, ME_SERVICE_PRINCIPAL);

    let out = run(
        &[
            "wi",
            "ls",
            "-p",
            "Demo",
            "--assignee",
            "me",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("'svc-kimicode-bot Bot' (member id u-shadow)"),
        "the row's full name must be quoted: {err}"
    );
}

#[test]
fn wi_show_without_project_reports_when_every_project_was_skipped() {
    // Every project denied: the hint must list them all and say what to do,
    // rather than reporting a plain "not found" over a search that never ran.
    let mut server = Server::new();
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/",
        PROJECTS_ONE_UNREADABLE,
    );
    for path in [
        "/api/v1/workspaces/ws/projects/p-locked/work-items/",
        "/api/v1/workspaces/ws/projects/p1/work-items/",
    ] {
        server
            .mock("GET", path)
            .match_query(Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"detail":"no active grant"}"#)
            .create();
    }

    let out = run(
        &["wi", "show", "nowhere-99", "--json", "--no-cache"],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Work item not found: nowhere-99"), "{err}");
    assert!(err.contains("skipped 2 project(s)"), "{err}");
    assert!(err.contains("Locked") && err.contains("Demo"), "{err}");
    assert!(err.contains("Pass -p <project>"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).is_empty(), "{err}");
}

// ---------------------------------------------------------------------------
// The `tc` command group: the test cases of a project.
//
// The server owns the rules of a case (which fields are mandatory, what a
// valid expected result is, the format of a module code) and answers a
// refused write with a stable reason code. The CLI prints that code and the
// sentence of the server, and the exit code tells a validation failure (5)
// from a transport or permission failure (4 / 2).
// ---------------------------------------------------------------------------

const TC_MODULES: &str = r#"[{"id":"m1","code":"CLI","description":"The command line of pbot","created_at":"2026-11-10T08:26:36Z","updated_at":"2026-11-10T08:26:36Z"}]"#;
const TC_CASES: &str = r#"{"results":[{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"Make sure the login path works","object_overview":"The login form","priority":"P1","preconditions":"The account exists","environment":"Windows 10","related_feature":"","remarks":"","status":"active","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z","created_by":{"id":"u1","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu"}}],"next_cursor":null,"next_page_results":false}"#;
const TC_CASE: &str = r#"{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"Make sure the login path works","object_overview":"The login form","priority":"P1","preconditions":"The account exists","environment":"Windows 10","related_feature":"","remarks":"","status":"active","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z","created_by":{"id":"u1","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu"}}"#;
const TC_CASES_WITH_LINK: &str = r#"{"results":[{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"Make sure the login path works","object_overview":"The login form","priority":"P1","preconditions":"The account exists","environment":"Windows 10","related_feature":"","remarks":"","status":"active","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}],"issue_links":[{"id":"l1","issue_id":"wi1","issue_identifier":"DEMO-1","issue_name":"A task","created_at":"2026-11-10T08:29:37Z"}],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z","created_by":{"id":"u1","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu"}}],"next_cursor":null,"next_page_results":false}"#;
const TC_STEPS: &str = r#"[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}]"#;
const TC_ISSUES: &str = r#"[{"id":"l1","issue_id":"wi1","issue_identifier":"DEMO-1","issue_name":"A task","created_at":"2026-11-10T08:29:37Z"}]"#;

/// The reads every `tc` command makes before it touches the feature.
fn mock_tc_reads(server: &mut Server) {
    mock_list(server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        server,
        "/api/v1/workspaces/ws/projects/p1/test-case-modules/",
        TC_MODULES,
    );
    mock_list(
        server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/",
        TC_CASES,
    );
    mock_list(
        server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
}

#[test]
fn tc_ls_json_contract() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);

    let out = run(
        &["tc", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let row = &parsed[0];
    // The whole view is pinned, not a few fields of it: the keys of the
    // `--json` output are a contract (ADR-0005), so a renamed key has to
    // break a test.
    assert_eq!(
        row,
        &serde_json::json!({
            "id": "tc1",
            "number": "TC-CLI-001",
            "module": "m1",
            "module_code": "CLI",
            "title": "Log in with a valid account",
            "purpose": "Make sure the login path works",
            "object_overview": "The login form",
            "priority": "P1",
            "preconditions": "The account exists",
            "environment": "Windows 10",
            "related_feature": "",
            "remarks": "",
            "status": "active",
            "steps": [{
                "id": "s1",
                "sequence_id": 1,
                "action": "Start pbot",
                "expected_result": "The login form is shown"
            }],
            "issue_links": [],
            "created_at": "2026-11-10T08:29:37Z",
            "updated_at": "2026-11-10T08:29:37Z",
            "created_by": "Jicai Liu",
        }),
        "the whole view of a case is pinned"
    );
    // The table goes to stderr, the JSON to stdout.
    assert!(String::from_utf8_lossy(&out.stderr).is_empty());
}

#[test]
fn tc_ls_sends_the_filters_to_the_server() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    let m = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("module".to_string(), "CLI".to_string()),
            Matcher::UrlEncoded("priority".to_string(), "P1".to_string()),
            Matcher::UrlEncoded("status".to_string(), "all".to_string()),
            Matcher::UrlEncoded("search".to_string(), "login".to_string()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TC_CASES)
        .create();

    let out = run(
        &[
            "tc",
            "ls",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--priority",
            "P1",
            "--status",
            "all",
            "--search",
            "login",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    m.assert();
}

#[test]
fn tc_ls_resolves_the_issue_filter_to_a_work_item_uuid() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/work-items/",
        ITEMS,
    );
    // The endpoint filters by the uuid of the work item, not by its
    // identifier — the CLI resolves the identifier first.
    let m = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::UrlEncoded("issue".to_string(), "wi1".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TC_CASES)
        .create();

    let out = run(
        &[
            "tc",
            "ls",
            "-p",
            "Demo",
            "--issue",
            "DEMO-1",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    m.assert();
}

#[test]
fn tc_show_renders_the_header_the_steps_and_the_links() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);

    let out = run(
        &[
            "tc",
            "show",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["number"], "TC-CLI-001");
    assert_eq!(parsed["title"], "Log in with a valid account");
    assert_eq!(parsed["steps"][0]["action"], "Start pbot");
    assert_eq!(parsed["issue_links"].as_array().unwrap().len(), 0);
}

#[test]
fn tc_show_prints_the_steps_and_the_links_as_tables() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/",
        TC_CASES_WITH_LINK,
    );

    let out = run(
        &["tc", "show", "TC-CLI-001", "-p", "Demo", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("TC-CLI-001"), "{err}");
    assert!(err.contains("The login form is shown"), "{err}");
    assert!(err.contains("DEMO-1"), "{err}");
    // The table never lands on stdout.
    assert!(stdout_of(&out).is_empty());
}

#[test]
fn tc_create_prints_the_number_the_server_assigned() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let m = server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        // The body is pinned exactly: the CLI sends the module, the header
        // and the steps, and nothing else — in particular no number.
        .match_body(Matcher::Json(serde_json::json!({
            "module": "m1",
            "title": "Log in with a valid account",
            "purpose": "Make sure the login path works",
            "object_overview": "The login form",
            "priority": "P1",
            "preconditions": "The account exists",
            "environment": "Windows 10",
            "steps": [{"action": "Start pbot", "expected_result": "The login form is shown"}],
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(TC_CASE)
        .create();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "Log in with a valid account",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--environment",
            "Windows 10",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("TC-CLI-001"), "{err}");
    m.assert();
}

#[test]
fn tc_create_reads_the_steps_from_a_json_file() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let m = server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_body(Matcher::Json(serde_json::json!({
            "module": "m1",
            "title": "A case",
            "purpose": "Make sure the login path works",
            "object_overview": "The login form",
            "priority": "P1",
            "preconditions": "The account exists",
            "environment": "Windows 10",
            "steps": [
                {"action": "Start pbot", "expected_result": "The login form is shown"},
                {"action": "Log in", "expected_result": "The dashboard is shown"}
            ],
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(TC_CASE)
        .create();

    let dir = std::env::temp_dir().join(format!("pbotcli_tc_steps_json_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("steps.json");
    std::fs::write(
        &path,
        r#"[{"action": "Start pbot", "expected": "The login form is shown"},
            {"action": "Log in", "expected": "The dashboard is shown"}]"#,
    )
    .expect("write the steps file");

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--environment",
            "Windows 10",
            "--steps-file",
            path.to_str().unwrap(),
            "--no-cache",
        ],
        &server.url(),
    );
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    m.assert();
}

#[test]
fn tc_create_reports_the_reason_code_of_a_refused_case() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The test case is not valid.","code":"invalid_test_case","errors":[
                {"code":"expected_is_unjudgeable","message":"预期结果必须可观测；正常、正确、无异常这类写法无法判定。","path":"cases[].steps[1].expected_result","field":"expected_result","case_number":null,"step_sequence":1}]}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--environment",
            "Windows 10",
            "--step",
            "Start pbot | works normally",
            "--no-cache",
        ],
        &server.url(),
    );
    // A validation failure is exit 5 — not the exit code of a transport error.
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    // The stable code and the sentence of the server, never a guess of the
    // CLI's own.
    assert!(err.contains("invalid_test_case"), "{err}");
    assert!(err.contains("expected_is_unjudgeable"), "{err}");
    assert!(err.contains("预期结果必须可观测"), "{err}");
    assert!(err.contains("step 1"), "{err}");
}

#[test]
fn tc_a_conflict_with_a_stable_code_reads_like_the_same_code_from_a_400() {
    // The same reason may arrive as a 400 or as a 409 (a race). Both are
    // the same class of failure, so both print the same readable error.
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .with_status(409)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The case is already in the batch.","code":"invalid_batch","errors":[
                {"code":"duplicate_case_number","message":"同一个用例在批次中重复出现。","path":"cases[TC-CLI-001]","field":"","case_number":"TC-CLI-001","step_sequence":null}]}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--environment",
            "Windows 10",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("duplicate_case_number"), "{err}");
    assert!(err.contains("同一个用例在批次中重复出现。"), "{err}");
    assert!(err.contains("TC-CLI-001"), "{err}");
}

#[test]
fn tc_reports_the_missing_evidence_of_a_batch() {
    // The code of a batch is reported the same way as the code of a case:
    // the stable code and the sentence of the server.
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The batch is not valid.","code":"invalid_batch","errors":[
                {"code":"evidence_missing","message":"缺少必填项：证据。","path":"cases[TC-PWR-001].evidence","field":"evidence","case_number":"TC-PWR-001","step_sequence":null}]}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--environment",
            "Windows 10",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("evidence_missing"), "{err}");
    assert!(err.contains("缺少必填项：证据。"), "{err}");
    assert!(err.contains("TC-PWR-001"), "{err}");
}

#[test]
fn tc_echoes_the_remedy_of_a_permission_failure_verbatim() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    let remedy = "Service principal 'Kimicode IsletspaceWorkstation' lacks scope for read test_case in project p1. A workspace admin must add the required scope for this service principal at Workspace settings -> Developer -> Service principals -> Manage scopes.";
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(serde_json::json!({"detail": remedy}).to_string())
        .create();

    let out = run(&["tc", "ls", "-p", "Demo", "--no-cache"], &server.url());
    // A permission failure is exit 2 — a script can tell it from a
    // validation failure (5) and from a transport failure (4).
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Manage scopes."), "{err}");
    assert!(err.contains("lacks scope for read test_case"), "{err}");
}

#[test]
fn tc_update_writes_only_the_fields_that_were_given() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let m = server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/")
        // The body is pinned exactly: the five fields that were given, and
        // none of the ones that were not.
        .match_body(Matcher::Json(serde_json::json!({
            "title": "A new title",
            "purpose": "A new purpose",
            "environment": "Linux",
            "related_feature": "The login path",
            "remarks": "Checked on the desktop client",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TC_CASE)
        .create();

    let out = run(
        &[
            "tc",
            "update",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--title",
            "A new title",
            "--purpose",
            "A new purpose",
            "--environment",
            "Linux",
            "--related-feature",
            "The login path",
            "--remarks",
            "Checked on the desktop client",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("TC-CLI-001"));
    m.assert();
}

#[test]
fn tc_update_refuses_to_change_the_module_or_the_number() {
    // No endpoint is mocked: the refusal happens before any request, so a
    // request would answer 501 and change the exit code of the command.
    let server = Server::new();

    for (flag, value, expected) in [
        ("--module", "CLI", "module of a test case cannot be changed"),
        (
            "--number",
            "TC-CLI-002",
            "number of a test case cannot be changed",
        ),
    ] {
        let out = run(
            &[
                "tc",
                "update",
                "TC-CLI-001",
                "-p",
                "Demo",
                flag,
                value,
                "--no-cache",
            ],
            &server.url(),
        );
        assert_eq!(out.status.code(), Some(5), "{flag}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(expected), "{flag}: {err}");
    }
}

#[test]
fn tc_steps_reads_the_step_table() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/",
        TC_STEPS,
    );

    let out = run(
        &[
            "tc",
            "steps",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed[0]["sequence_id"], 1);
    assert_eq!(parsed[0]["action"], "Start pbot");
}

#[test]
fn tc_steps_replaces_the_whole_table() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    // The table is read before it is replaced, so the command can say how
    // many rows the replacement drops.
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/",
        TC_STEPS,
    );
    let m = server
        .mock("PUT", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/")
        .match_body(Matcher::Json(serde_json::json!({
            "steps": [
                {"action": "Start pbot", "expected_result": "The login form is shown"},
                {"action": "Log in", "expected_result": "The dashboard is shown"}
            ]
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"updated_at":"2026-11-11T00:00:00Z","steps":[
                {"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"},
                {"id":"s2","sequence_id":2,"action":"Log in","expected_result":"The dashboard is shown"}]}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "steps",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--step",
            "Start pbot | The login form is shown",
            "--step",
            "Log in | The dashboard is shown",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().unwrap().len(), 2);
    assert_eq!(parsed[1]["sequence_id"], 2);
    m.assert();
}

#[test]
fn tc_issues_links_and_unlinks_a_work_item() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let link = server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/issues/")
        .match_body(Matcher::Json(serde_json::json!({"issue_id": "wi1"})))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"id":"l1","issue_id":"wi1","issue_identifier":"DEMO-1","issue_name":"A task","created_at":"2026-11-10T08:29:37Z"}"#,
        )
        .create();
    let unlink = server
        .mock(
            "DELETE",
            "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/issues/wi1/",
        )
        .with_status(204)
        .create();

    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/issues/",
        TC_ISSUES,
    );

    // Without --add or --remove the links of the case are listed.
    let out = run(
        &[
            "tc",
            "issues",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed[0]["issue_identifier"], "DEMO-1");
    assert_eq!(parsed[0]["issue_id"], "wi1");

    let out = run(
        &[
            "tc",
            "issues",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--add",
            "DEMO-1",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["issue_identifier"], "DEMO-1");
    link.assert();

    let out = run(
        &[
            "tc",
            "issues",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--remove",
            "DEMO-1",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    unlink.assert();
}

#[test]
fn tc_delete_reports_the_permission_failure_of_a_member() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    server
        .mock(
            "DELETE",
            "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/",
        )
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"Only a project admin can delete a test case."}"#)
        .create();

    let out = run(
        &["tc", "delete", "TC-CLI-001", "-p", "Demo", "--no-cache"],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Only a project admin can delete a test case."),
        "{err}"
    );
}

#[test]
fn tc_delete_removes_a_case() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let m = server
        .mock(
            "DELETE",
            "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/",
        )
        .with_status(204)
        .create();

    let out = run(
        &["tc", "delete", "TC-CLI-001", "-p", "Demo", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("TC-CLI-001"));
    m.assert();
}

#[test]
fn tc_module_ls_lists_the_registered_codes() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-case-modules/",
        TC_MODULES,
    );

    let out = run(
        &["tc", "module", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed[0]["code"], "CLI");
    assert_eq!(parsed[0]["description"], "The command line of pbot");
}

#[test]
fn tc_module_create_registers_a_code() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    let m = server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-case-modules/")
        .match_body(Matcher::Json(serde_json::json!({
            "code": "PWR",
            "description": "The power modes",
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"id":"m2","code":"PWR","description":"The power modes","created_at":"2026-11-10T08:26:36Z","updated_at":"2026-11-10T08:26:36Z"}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "module",
            "create",
            "-p",
            "Demo",
            "--code",
            "PWR",
            "--description",
            "The power modes",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("PWR"));
    m.assert();
}

#[test]
fn tc_module_create_reports_the_code_of_a_refused_module_code() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-case-modules/")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The module code is not valid.","code":"module_code_format","errors":[
                {"code":"module_code_format","message":"模块码必须是 2 至 6 位大写字母。","path":"code","field":"code"}]}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "module",
            "create",
            "-p",
            "Demo",
            "--code",
            "TOO-LONG",
            "--description",
            "x",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("module_code_format"), "{err}");
    assert!(err.contains("模块码必须是 2 至 6 位大写字母。"), "{err}");
}

#[test]
fn tc_update_without_a_field_is_refused_before_any_request() {
    // No endpoint is mocked: a request would answer 501 and change the exit
    // code of the command.
    let server = Server::new();

    let out = run(
        &["tc", "update", "TC-CLI-001", "-p", "Demo", "--no-cache"],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Nothing to update."), "{err}");
}

#[test]
fn tc_steps_refuses_an_empty_table() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);

    let out = run(
        &[
            "tc",
            "steps",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--steps-file",
            "/dev/null",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("at least one step"), "{err}");
}

#[test]
fn tc_steps_says_how_many_rows_the_replacement_drops() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/",
        TC_STEPS,
    );
    server
        .mock("PUT", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"updated_at":"2026-11-11T00:00:00Z","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}]}"#,
        )
        .create();

    // The table of the case holds one step and the replacement holds one
    // too, so nothing is said.
    let out = run(
        &[
            "tc",
            "steps",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!String::from_utf8_lossy(&out.stderr).contains("Replacing"));

    // A table of two steps against a table of one says what it replaces.
    let out = run(
        &[
            "tc",
            "steps",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--step",
            "Start pbot | The login form is shown",
            "--step",
            "Log in | The dashboard is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Replacing the 1 step(s) of TC-CLI-001 with 2 step(s)."),
        "{err}"
    );
}

#[test]
fn tc_show_finds_a_voided_case_by_its_number() {
    // A voided case keeps its number, so the lookup has to ask for the
    // voided cases as well.
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    let m = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::UrlEncoded("status".to_string(), "all".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"results":[{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"A voided case","purpose":"p","object_overview":"o","priority":"P1","preconditions":"c","environment":"e","related_feature":"","remarks":"","status":"void","steps":[],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z"}],"next_cursor":null,"next_page_results":false}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "show",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["status"], "void");
    m.assert();
}

#[test]
fn tc_create_refuses_a_module_code_that_is_not_registered() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "NOPE",
            "--title",
            "A case",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--environment",
            "Windows 10",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(3));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Test case module not found: NOPE"), "{err}");
    assert!(err.contains("pbot tc module ls"), "{err}");
}

#[test]
fn tc_issues_reports_the_link_that_is_already_there() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/issues/")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The work item is already linked to this test case.","code":"issue_link_exists","errors":[
                {"code":"issue_link_exists","message":"该工作项已经关联到这个用例。","path":"issue_id","field":"issue_id"}]}"#,
        )
        .create();

    let out = run(
        &[
            "tc",
            "issues",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--add",
            "DEMO-1",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("issue_link_exists"), "{err}");
    assert!(err.contains("该工作项已经关联到这个用例。"), "{err}");
}

/// Two cases, so the limit of the list has something to cut.
const TC_CASES_TWO: &str = r#"{"results":[{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"Make sure the login path works","object_overview":"The login form","priority":"P1","preconditions":"The account exists","environment":"Windows 10","related_feature":"","remarks":"","status":"active","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z","created_by":{"id":"u1","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu"}},{"id":"tc2","number":"TC-CLI-002","module":"m1","module_code":"CLI","title":"Log out again","purpose":"Make sure the way out works","object_overview":"The dashboard","priority":"P2","preconditions":"The user is logged in","environment":"Windows 10","related_feature":"","remarks":"","status":"active","steps":[{"id":"s2","sequence_id":1,"action":"Open the menu","expected_result":"The way out is there"}],"issue_links":[],"created_at":"2026-11-10T08:30:37Z","updated_at":"2026-11-10T08:30:37Z","created_by":{"id":"u1","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu"}}],"next_cursor":null,"next_page_results":false}"#;

#[test]
fn tc_create_sends_every_header_flag() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    // Every write field of the case is pinned by name: a typo in the
    // serializer (or in the body builder) would turn a flag into a no-op,
    // and the server would swallow the unknown key without a word.
    let m = server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_body(Matcher::Json(serde_json::json!({
            "module": "m1",
            "title": "A case",
            "purpose": "Make sure the login path works",
            "object_overview": "The login form",
            "priority": "P1",
            "preconditions": "The account exists",
            "environment": "Windows 10",
            "related_feature": "The login path",
            "remarks": "Checked on the desktop client",
            "status": "active",
            "steps": [{"action": "Start pbot", "expected_result": "The login form is shown"}],
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(TC_CASE)
        .create();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "Make sure the login path works",
            "--object-overview",
            "The login form",
            "--priority",
            "P1",
            "--precondition",
            "The account exists",
            "--env",
            "Windows 10",
            "--related-feature",
            "The login path",
            "--remarks",
            "Checked on the desktop client",
            "--status",
            "active",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    m.assert();
}

#[test]
fn tc_create_needs_at_least_one_step() {
    // No endpoint is mocked: the refusal happens before any request.
    let server = Server::new();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "p",
            "--object-overview",
            "o",
            "--priority",
            "P1",
            "--precondition",
            "c",
            "--environment",
            "e",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("at least one step"), "{err}");
    assert!(err.contains("--step"), "{err}");
}

#[test]
fn tc_create_refuses_a_priority_the_server_does_not_know() {
    let server = Server::new();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "p",
            "--object-overview",
            "o",
            "--priority",
            "P9",
            "--precondition",
            "c",
            "--environment",
            "e",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Invalid priority 'P9'"), "{err}");
    assert!(err.contains("P0, P1, P2, P3"), "{err}");
}

#[test]
fn tc_update_says_how_many_rows_the_step_replacement_drops() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    // The table of the case holds one step and the update sends two.
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/",
        TC_STEPS,
    );
    let m = server
        .mock("PATCH", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/")
        .match_body(Matcher::Json(serde_json::json!({
            "steps": [
                {"action": "Start pbot", "expected_result": "The login form is shown"},
                {"action": "Log in", "expected_result": "The dashboard is shown"}
            ]
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TC_CASE)
        .create();

    let out = run(
        &[
            "tc",
            "update",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--step",
            "Start pbot | The login form is shown",
            "--step",
            "Log in | The dashboard is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Replacing the 1 step(s) of TC-CLI-001 with 2 step(s)."),
        "the replacement of a step table is never silent: {err}"
    );
    m.assert();
}

#[test]
fn tc_ls_cuts_the_rows_at_the_limit_and_says_so() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/",
        TC_CASES_TWO,
    );

    let out = run(
        &["tc", "ls", "-p", "Demo", "-l", "1", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().unwrap().len(), 1);
    assert_eq!(parsed[0]["number"], "TC-CLI-001");
    // A list that hides rows without saying so is the one place where the
    // output and the reality diverge.
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Showing 1 of 2 test cases."), "{err}");
}

#[test]
fn tc_ls_shows_every_row_when_the_limit_cuts_nothing() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    mock_list(
        &mut server,
        "/api/v1/workspaces/ws/projects/p1/test-cases/",
        TC_CASES_TWO,
    );

    let out = run(
        &["tc", "ls", "-p", "Demo", "-l", "10", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed.as_array().unwrap().len(), 2);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("Showing"));
}

#[test]
fn tc_create_help_offers_no_flag_that_is_its_own_alias() {
    // A flag whose alias is its own name is noise in the discovery surface:
    // `--object-overview` is not an alias of `--object-overview`.
    for command in [["tc", "create", "--help"], ["tc", "update", "--help"]] {
        let out = run(&command, "http://127.0.0.1:1");
        assert!(out.status.success(), "{command:?}");
        let help = stdout_of(&out);
        assert!(help.contains("[alias: --preconditions]"), "{help}");
        assert!(help.contains("[alias: --env]"), "{help}");
        assert!(!help.contains("[alias: --object-overview]"), "{help}");
        assert!(!help.contains("[alias: --related-feature]"), "{help}");
    }
}

#[test]
fn tc_create_help_makes_the_mandatory_fields_mandatory() {
    let out = run(&["tc", "create", "--help"], "http://127.0.0.1:1");
    assert!(out.status.success());
    let help = stdout_of(&out);
    // The server refuses a case without these six, so the usage line has
    // to ask for them. (The options list mentions every flag either way,
    // so the usage line is what the assertion looks at.)
    let usage = help
        .lines()
        .find(|line| line.starts_with("Usage:"))
        .expect("the help has a usage line");
    for flag in [
        "--module <MODULE>",
        "--title <TITLE>",
        "--purpose <PURPOSE>",
        "--object-overview <OBJECT_OVERVIEW>",
        "--priority <PRIORITY>",
        "--precondition <PRECONDITION>",
        "--environment <ENVIRONMENT>",
    ] {
        assert!(
            usage.contains(flag),
            "{flag} is missing from the usage line"
        );
    }
}

#[test]
fn tc_update_refuses_an_empty_step_table() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let dir = std::env::temp_dir().join(format!("pbotcli_tc_empty_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("steps.json");
    std::fs::write(&path, "[]").expect("write the steps file");

    let out = run(
        &[
            "tc",
            "update",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--steps-file",
            path.to_str().unwrap(),
            "--no-cache",
        ],
        &server.url(),
    );
    std::fs::remove_dir_all(&dir).ok();
    // A replacement that empties the table is refused, exactly as `tc steps`
    // refuses it.
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("at least one step"), "{err}");
}

#[test]
fn tc_create_refuses_a_status_the_server_does_not_know() {
    let server = Server::new();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "p",
            "--object-overview",
            "o",
            "--priority",
            "P1",
            "--precondition",
            "c",
            "--environment",
            "e",
            "--status",
            "archived",
            "--step",
            "Start pbot | The login form is shown",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Invalid status 'archived'"), "{err}");
    assert!(err.contains("active, void"), "{err}");
}

#[test]
fn tc_create_reports_a_steps_file_that_cannot_be_read() {
    let server = Server::new();

    let out = run(
        &[
            "tc",
            "create",
            "-p",
            "Demo",
            "--module",
            "CLI",
            "--title",
            "A case",
            "--purpose",
            "p",
            "--object-overview",
            "o",
            "--priority",
            "P1",
            "--precondition",
            "c",
            "--environment",
            "e",
            "--steps-file",
            "/no/such/file/pbotcli-steps.json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Cannot read the steps file"), "{err}");
    assert!(err.contains("--steps-file"), "{err}");
}

#[test]
fn tc_ls_needs_a_project() {
    let server = Server::new();

    let out = run(&["tc", "ls", "--no-cache"], &server.url());
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("Project is required"), "{err}");
    assert!(err.contains("-p/--project"), "{err}");
}

#[test]
fn tc_issues_refuses_to_add_and_remove_at_once() {
    let server = Server::new();

    let out = run(
        &[
            "tc",
            "issues",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--add",
            "DEMO-1",
            "--remove",
            "DEMO-2",
            "--no-cache",
        ],
        &server.url(),
    );
    // The two flags are exclusive: clap refuses the combination before
    // anything is sent.
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("cannot be used with"), "{err}");
}

#[test]
fn tc_ls_normalizes_the_filters_to_the_vocabulary_of_the_server() {
    let mut server = Server::new();
    mock_list(&mut server, "/api/v1/workspaces/ws/projects/", PROJECTS);
    // The server stores the priority uppercased and compares the status
    // literally, so the CLI normalizes both.
    let m = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("priority".to_string(), "P1".to_string()),
            Matcher::UrlEncoded("status".to_string(), "all".to_string()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TC_CASES)
        .create();

    let out = run(
        &[
            "tc",
            "ls",
            "-p",
            "Demo",
            "--priority",
            "p1",
            "--status",
            "ALL",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    m.assert();
}

#[test]
fn tc_delete_answers_with_json_when_it_is_asked_for() {
    let mut server = Server::new();
    mock_tc_reads(&mut server);
    let m = server
        .mock(
            "DELETE",
            "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/",
        )
        .with_status(204)
        .create();

    let out = run(
        &[
            "tc",
            "delete",
            "TC-CLI-001",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["number"], "TC-CLI-001");
    assert_eq!(parsed["deleted"], true);
    m.assert();
}
