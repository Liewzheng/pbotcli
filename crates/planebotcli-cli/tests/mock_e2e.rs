//! End-to-end command-layer tests: run the real `planebotcli` binary against a
//! mockito mock server (no real instance). Pins the `--json` output contract
//! (sequence ids, resolved names, web_url) and exit codes.
//!
//! The binary is located via cargo's `CARGO_BIN_EXE_planebotcli`. Proxies are
//! stripped from the child env so local mockito traffic is not hijacked.

use std::io::Write;
use std::process::{Command, Stdio};

use mockito::{Matcher, Server};

const BIN: &str = env!("CARGO_BIN_EXE_planebotcli");

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
        .expect("failed to spawn planebotcli")
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
        .expect("failed to spawn planebotcli");
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
    let expected_html = planebotcli_html::body_to_html(COMMENT_BODY);
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
    let expected_html = planebotcli_html::md_to_html(COMMENT_BODY_MD);
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
    let expected_html = planebotcli_html::body_to_html(COMMENT_BODY);
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
    let expected_html = planebotcli_html::body_to_html(COMMENT_BODY);
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
