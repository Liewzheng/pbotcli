//! End-to-end tests of the `run` command group: the real `pbotcli` binary
//! against a mockito mock server (no real instance).
//!
//! The tests pin the `--json` output contract (ADR-0005), the exit codes
//! and the semantics of the feature that cannot be undone: the verdict of
//! a step and the conclusion of a case are written once, a case that is
//! already in a batch is refused, and a completed batch is frozen.

use std::process::Command;

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

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

const PROJECTS: &str = r#"{"results":[{"id":"p1","name":"Demo","identifier":"DEMO","created_at":"2026-01-01T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;
const ME: &str =
    r#"{"id":"u1","display_name":"Bot","first_name":"B","last_name":"","email":"b@x"}"#;
const TC_CASES: &str = r#"{"results":[{"id":"tc1","number":"TC-CLI-007","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"Make sure the login path works","object_overview":"The login form","priority":"P1","preconditions":"The account exists","environment":"Windows 10","related_feature":"","remarks":"","status":"active","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The login form is shown"}],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z"}],"next_cursor":null,"next_page_results":false}"#;
const RUN: &str = r#"{"id":"r1","number":"RUN-20261109-01","run_date":"2026-11-09","version_number":"1.2.3","build_number":"42","commit_hash":"abc1234","environment":"bench","executed_by":"u1","executed_by_name":"Bot","status":"in_progress","retest_of":null,"retest_of_number":null,"cases":[{"id":"rc1","test_case":"tc1","case_number":"TC-CLI-007","case_module_code":"CLI","case_title":"Log in with a valid account","case_purpose":"Make sure the login path works","case_object_overview":"The login form","case_priority":"P1","case_preconditions":"The account exists","case_environment":"Windows 10","conclusion":"","evidence":"","remarks":"","actual_result_summary":"","na_reason":"","na_criteria_source":"","manual_executor":"","manual_condition":"","steps":[{"id":"s1","sequence_id":3,"action_snapshot":"Log in","expected_result_snapshot":"The dashboard is shown","conclusion":"unset","actual_value":"","attachments":[]}],"attachments":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z"}],"created_at":"2026-11-09T08:29:37Z","updated_at":"2026-11-09T08:29:37Z","created_by":{"id":"u1","display_name":"Bot","first_name":"B","last_name":""}}"#;
const RUN_CASES: &str = r#"[{"id":"rc1","test_case":"tc1","case_number":"TC-CLI-007","case_module_code":"CLI","case_title":"Log in with a valid account","case_purpose":"Make sure the login path works","case_object_overview":"The login form","case_priority":"P1","case_preconditions":"The account exists","case_environment":"Windows 10","conclusion":"","evidence":"","remarks":"","actual_result_summary":"","na_reason":"","na_criteria_source":"","manual_executor":"","manual_condition":"","steps":[{"id":"s1","sequence_id":3,"action_snapshot":"Log in","expected_result_snapshot":"The dashboard is shown","conclusion":"unset","actual_value":"","attachments":[]}],"attachments":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z"}]"#;

/// The reads every `run` command makes before it touches the feature.
fn mock_run_reads(server: &mut Server) {
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    server
        .mock("GET", "/api/v1/users/me/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(ME)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(TC_CASES)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-runs/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"results":[{"id":"r1","number":"RUN-20261109-01","run_date":"2026-11-09","version_number":"1.2.3","build_number":"42","commit_hash":"abc1234","environment":"bench","executed_by":"u1","executed_by_name":"Bot","status":"in_progress","retest_of":null,"retest_of_number":null,"cases":[],"created_at":"2026-11-09T08:29:37Z","updated_at":"2026-11-09T08:29:37Z"}],"next_cursor":null,"next_page_results":false}"#,
        )
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-runs/r1/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(RUN)
        .create();
}

/// The case records the batch holds — empty unless a test says
/// otherwise.
fn mock_run_cases(server: &mut Server, body: &str) {
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/",
        )
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create();
}

#[test]
fn run_create_sends_the_header_without_a_number_and_prints_the_number_of_the_server() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let create = server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-runs/")
        .match_body(Matcher::Json(serde_json::json!({
            "run_date": "2026-11-09",
            "version_number": "1.2.3",
            "build_number": "42",
            "commit_hash": "abc1234",
            "environment": "bench",
            "executed_by": "u1",
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(RUN)
        .create();

    let out = run(
        &[
            "run",
            "create",
            "-p",
            "Demo",
            "--version",
            "1.2.3",
            "--build",
            "42",
            "--commit",
            "abc1234",
            "--env",
            "bench",
            "--run-date",
            "2026-11-09",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    // The number of the batch is the server's — the CLI never builds one.
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["number"], "RUN-20261109-01");
    assert_eq!(parsed["executed_by_name"], "Bot");
    // The body carries no number and no status: the server owns both.
    create.assert();
}

#[test]
fn run_ls_prints_the_batches_of_the_project() {
    let mut server = Server::new();
    mock_run_reads(&mut server);

    let out = run(
        &["run", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed[0]["number"], "RUN-20261109-01");
    assert_eq!(parsed[0]["run_date"], "2026-11-09");
    assert_eq!(parsed[0]["build_number"], "42");
}

#[test]
fn run_show_prints_the_header_the_cases_and_their_steps() {
    let mut server = Server::new();
    mock_run_reads(&mut server);

    let out = run(
        &["run", "show", "RUN-20261109-01", "-p", "Demo", "--no-cache"],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let err = stderr_of(&out);
    assert!(err.contains("RUN-20261109-01"), "{err}");
    assert!(err.contains("TC-CLI-007"), "{err}");
    // The steps of the case are printed with their verdict.
    assert!(err.contains("unset"), "{err}");
}

#[test]
fn run_show_json_carries_the_steps_and_the_files() {
    let mut server = Server::new();
    mock_run_reads(&mut server);

    let out = run(
        &[
            "run",
            "show",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let case = &parsed["cases"][0];
    assert_eq!(case["case_number"], "TC-CLI-007");
    assert_eq!(case["steps"][0]["sequence_id"], 3);
    assert_eq!(case["steps"][0]["conclusion"], "unset");
    assert!(case["attachments"].as_array().unwrap().is_empty());
}

#[test]
fn run_add_sends_the_uuids_of_the_cases() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, "[]");
    let add = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/",
        )
        .match_body(Matcher::Json(serde_json::json!({
            "test_case_ids": ["tc1"],
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(RUN_CASES)
        .create();

    let out = run(
        &[
            "run",
            "add",
            "RUN-20261109-01",
            "TC-CLI-007",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(stderr_of(&out).contains("TC-CLI-007"));
    add.assert();
}

#[test]
fn run_step_writes_the_verdict_and_the_measured_value() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let step = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .match_body(Matcher::Json(serde_json::json!({
            "conclusion": "fail",
            "actual_value": "the password was refused",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"s1","sequence_id":3,"conclusion":"fail","actual_value":"the password was refused"}"#)
        .create();

    let out = run(
        &[
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "fail",
            "--actual",
            "the password was refused",
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    step.assert();
}

#[test]
fn run_step_sends_the_measured_value_of_a_file_byte_for_byte() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    // The Markdown reaches the server as it was written: no conversion,
    // no rewriting, and the blank lines inside it stay.
    let notes = "# the log\n\n* the password was refused\n* and the account is locked\n";
    let dir = std::env::temp_dir().join(format!("pbotcli_run_notes_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("notes.md");
    std::fs::write(&path, notes).expect("write");
    let step = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .match_body(Matcher::Json(serde_json::json!({
            "conclusion": "fail",
            "actual_value": notes,
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"s1","sequence_id":3,"conclusion":"fail"}"#)
        .create();

    let out = run(
        &[
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "fail",
            "--actual-file",
            path.to_str().expect("path"),
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    step.assert();
}

#[test]
fn run_conclude_writes_the_conclusion_and_the_evidence() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let conclude = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
        )
        .match_body(Matcher::Json(serde_json::json!({
            "conclusion": "FAIL",
            "evidence": "see the report",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"rc1","case_number":"TC-CLI-007","conclusion":"FAIL","evidence":"see the report"}"#)
        .create();

    let out = run(
        &[
            "run",
            "conclude",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--conclusion",
            "FAIL",
            "--evidence",
            "see the report",
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    conclude.assert();
}

#[test]
fn run_finalize_closes_the_batch() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let finalize = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/finalize/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(RUN.replace("\"status\":\"in_progress\"", "\"status\":\"completed\""))
        .create();

    let out = run(
        &[
            "run",
            "finalize",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    assert!(stderr_of(&out).contains("RUN-20261109-01"));
    finalize.assert();
}

#[test]
fn run_summary_prints_the_counts_and_the_deviations() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/summary/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"total":1,"pass":0,"fail":1,"na":0,"manual":1,"deviations":[{"case_number":"TC-CLI-007","summary":"the password was refused"}]}"#,
        )
        .create();

    let out = run(
        &[
            "run",
            "summary",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["available"], true);
    assert_eq!(parsed["total"], 1);
    assert_eq!(parsed["fail"], 1);
    // The counts overlap — a case that failed and was never executed on
    // one of its steps is counted twice.
    assert_eq!(parsed["manual"], 1);
    assert_eq!(parsed["deviations"][0]["case_number"], "TC-CLI-007");
}

#[test]
fn run_summary_says_so_when_the_instance_does_not_serve_the_endpoint() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/summary/",
        )
        .with_status(404)
        .with_header("content-type", "application/json")
        .with_body(r#"{"detail":"Not found."}"#)
        .create();

    let out = run(
        &[
            "run",
            "summary",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    // An endpoint the instance does not serve is not a crash.
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["available"], false);
    assert_eq!(parsed["number"], "RUN-20261109-01");
}

#[test]
fn run_step_without_a_confirmation_writes_nothing() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    // The write may not be made: the command is not on a terminal and it
    // was not confirmed with --yes.
    let step = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .expect(0)
        .with_status(200)
        .create();

    let out = run(
        &[
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "fail",
            "--actual",
            "the password was refused",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("--yes"), "{err}");
    step.assert();
}

#[test]
fn run_conclude_without_a_confirmation_writes_nothing() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let conclude = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
        )
        .expect(0)
        .with_status(200)
        .create();

    let out = run(
        &[
            "run",
            "conclude",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--conclusion",
            "FAIL",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("--yes"), "{err}");
    conclude.assert();
}

#[test]
fn a_case_that_is_already_in_the_batch_is_refused_with_its_number() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, RUN_CASES);
    // The batch holds the case already, so the CLI refuses before it
    // writes — and it names the case.
    let add = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/",
        )
        .expect(0)
        .with_status(201)
        .create();

    let out = run(
        &[
            "run",
            "add",
            "RUN-20261109-01",
            "TC-CLI-007",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("duplicate_case_number"), "{err}");
    assert!(err.contains("TC-CLI-007"), "{err}");
    add.assert();
}

#[test]
fn a_duplicate_that_arrives_as_a_conflict_reads_like_the_same_reason() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, "[]");
    // The race of two concurrent adds: the server refuses with the same
    // code, and the CLI reads it like the refusal of the check above.
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/")
        .with_status(409)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The test case is already in this batch.","code":"duplicate_case_number","errors":[
                {"code":"duplicate_case_number","message":"同一个用例在批次中重复出现。","path":"test_case_ids","field":"test_case_ids"}]}"#,
        )
        .create();

    let out = run(
        &[
            "run",
            "add",
            "RUN-20261109-01",
            "TC-CLI-007",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("duplicate_case_number"), "{err}");
    assert!(err.contains("同一个用例在批次中重复出现"), "{err}");
}

#[test]
fn a_conclusion_that_is_final_is_never_worked_around() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let conclude = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
        )
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The payload is not valid.","code":"invalid_payload","errors":[
                {"code":"conclusion_is_final","message":"结论一经写进批次不再改。","path":"conclusion","field":"conclusion"}]}"#,
        )
        .create();

    let out = run(
        &[
            "run",
            "conclude",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--conclusion",
            "FAIL",
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    // The stable code of the server is shown, and the CLI does not try
    // to work around it.
    assert!(err.contains("conclusion_is_final"), "{err}");
    // The write is made once — there is no retry.
    conclude.assert();
}

#[test]
fn a_frozen_batch_is_refused_once_and_never_retried() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, "[]");
    let add = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/",
        )
        .with_status(409)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"A completed batch is frozen.","code":"run_completed","errors":[
                {"code":"run_completed","message":"批次已完成后不能再写入。"}]}"#,
        )
        .create();

    let out = run(
        &[
            "run",
            "add",
            "RUN-20261109-01",
            "TC-CLI-007",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("run_completed"), "{err}");
    // The refusal is printed once: the CLI does not retry a frozen
    // batch on its own.
    add.assert();
}

#[test]
fn a_refused_finalize_prints_every_reason_with_its_case_and_step() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-runs/r1/finalize/")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The batch is not valid.","code":"invalid_batch","errors":[
                {"code":"evidence_missing","message":"缺少必填项：证据。","path":"cases[TC-CLI-007].evidence","field":"evidence","case_number":"TC-CLI-007","step_sequence":null},
                {"code":"fail_without_actual_value","message":"失败的步骤必须有实测值。","path":"cases[TC-CLI-007].steps[3].actual_value","field":"actual_value","case_number":"TC-CLI-007","step_sequence":3}]}"#,
        )
        .create();

    let out = run(
        &[
            "run",
            "finalize",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    // Every reason is printed with its code, its place and the sentence
    // of the server.
    assert!(err.contains("evidence_missing"), "{err}");
    assert!(err.contains("缺少必填项：证据"), "{err}");
    assert!(err.contains("fail_without_actual_value"), "{err}");
    assert!(err.contains("case TC-CLI-007, step 3"), "{err}");
}

#[test]
fn a_case_that_carries_results_is_not_taken_out_of_the_batch() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let remove = server
        .mock(
            "DELETE",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
        )
        .with_status(409)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"error":"The case record already carries execution results and cannot be removed from the batch; re-run the case in a new batch.","code":"run_case_removal_after_execution","errors":[
                {"code":"run_case_removal_after_execution","message":"该用例已记录执行结果，不能从批次移除；重跑请作废本批次。","path":"","field":""}]}"#,
        )
        .create();

    let out = run(
        &[
            "run",
            "remove",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("run_case_removal_after_execution"), "{err}");
    remove.assert();
}

#[test]
fn a_verdict_the_server_would_refuse_is_refused_before_the_request() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let step = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .expect(0)
        .with_status(200)
        .create();

    let out = run(
        &[
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "na",
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    // N/A is a conclusion of a case, not a verdict of a step.
    assert!(err.contains("pass, fail or unset"), "{err}");
    step.assert();
}

#[test]
fn run_finalize_refuses_a_batch_without_an_executor() {
    let mut server = Server::new();
    // The batch carries no executor: the server would refuse it, so the
    // CLI says so before the request.
    let without_executor = RUN
        .replace("\"executed_by\":\"u1\"", "\"executed_by\":\"\"")
        .replace("\"executed_by_name\":\"Bot\"", "\"executed_by_name\":\"\"");
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-runs/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"results":[{"id":"r1","number":"RUN-20261109-01","run_date":"2026-11-09","version_number":"1.2.3","build_number":"42","commit_hash":"abc1234","environment":"bench","executed_by":"","executed_by_name":"","status":"in_progress","cases":[]}],"next_cursor":null,"next_page_results":false}"#,
        )
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-runs/r1/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(without_executor)
        .create();
    let finalize = server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/finalize/",
        )
        .expect(0)
        .with_status(200)
        .create();

    let out = run(
        &[
            "run",
            "finalize",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = stderr_of(&out);
    assert!(err.contains("executor"), "{err}");
    finalize.assert();
}

#[test]
fn run_conclude_accepts_an_image_as_the_evidence_of_a_case() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, RUN_CASES);
    let dir = std::env::temp_dir().join(format!("pbotcli_run_shot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("shot.png");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n").expect("write");
    let upload_url = format!("{}/upload", server.url());
    // The file is bound to the case record: the upload is the bind.
    let register = server
        .mock("POST", "/api/v1/workspaces/ws/assets/")
        .match_body(Matcher::Json(serde_json::json!({
            "name": "shot.png",
            "type": "image/png",
            "size": 8,
            "entity_type": "TEST_RUN_CASE_EVIDENCE",
            "entity_identifier": "rc1",
            "project_id": "p1",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "asset_id": "a1",
                "asset_url": "http://x/a1",
                "upload_data": {"url": upload_url, "fields": {}},
            })
            .to_string(),
        )
        .create();
    server.mock("POST", "/upload").with_status(200).create();
    server
        .mock("PATCH", "/api/v1/workspaces/ws/assets/a1/")
        .with_status(204)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/assets/a1/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"asset_id":"a1","asset_url":"http://x/a1"}"#)
        .create();
    // The evidence of the case is the file alone: the body carries the
    // conclusion and nothing else.
    let conclude = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
        )
        .match_body(Matcher::Json(serde_json::json!({
            "conclusion": "FAIL",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"rc1","case_number":"TC-CLI-007","conclusion":"FAIL"}"#)
        .create();

    let out = run(
        &[
            "run",
            "conclude",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--conclusion",
            "FAIL",
            "--image",
            path.to_str().expect("path"),
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    register.assert();
    conclude.assert();
}

#[test]
fn run_step_binds_an_image_to_the_step_record() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, RUN_CASES);
    let dir = std::env::temp_dir().join(format!("pbotcli_run_step_shot_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("shot.png");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n").expect("write");
    let upload_url = format!("{}/upload", server.url());
    // The step record is read first: its uuid is what the file is bound
    // to.
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"s1","sequence_id":3,"conclusion":"unset"}"#)
        .create();
    let register = server
        .mock("POST", "/api/v1/workspaces/ws/assets/")
        .match_body(Matcher::Json(serde_json::json!({
            "name": "shot.png",
            "type": "image/png",
            "size": 8,
            "entity_type": "TEST_RUN_STEP_RECORD",
            "entity_identifier": "s1",
            "project_id": "p1",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            serde_json::json!({
                "asset_id": "a2",
                "asset_url": "http://x/a2",
                "upload_data": {"url": upload_url, "fields": {}},
            })
            .to_string(),
        )
        .create();
    server.mock("POST", "/upload").with_status(200).create();
    server
        .mock("PATCH", "/api/v1/workspaces/ws/assets/a2/")
        .with_status(204)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/assets/a2/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"asset_id":"a2","asset_url":"http://x/a2"}"#)
        .create();
    let step = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .match_body(Matcher::Json(serde_json::json!({
            "conclusion": "fail",
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"s1","sequence_id":3,"conclusion":"fail"}"#)
        .create();

    let out = run(
        &[
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "fail",
            "--image",
            path.to_str().expect("path"),
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    std::fs::remove_dir_all(&dir).ok();
    assert!(out.status.success(), "stderr: {}", stderr_of(&out));
    register.assert();
    step.assert();
}

#[test]
fn a_value_and_a_file_are_not_mixed() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    let step = server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .expect(0)
        .with_status(200)
        .create();

    let out = run(
        &[
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "fail",
            "--actual",
            "a log",
            "--actual-file",
            "/tmp/notes.md",
            "--yes",
            "-p",
            "Demo",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    assert!(stderr_of(&out).contains("--actual"));
    step.assert();
}

#[test]
fn the_whole_flow_of_a_batch_runs_from_the_create_to_the_summary() {
    let mut server = Server::new();
    mock_run_reads(&mut server);
    mock_run_cases(&mut server, "[]");
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-runs/")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(RUN)
        .create();
    server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/",
        )
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(RUN_CASES)
        .create();
    server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"s1","sequence_id":3,"conclusion":"fail","actual_value":"the password was refused"}"#)
        .create();
    server
        .mock(
            "PATCH",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"rc1","case_number":"TC-CLI-007","conclusion":"FAIL","evidence":"see the report"}"#)
        .create();
    server
        .mock(
            "POST",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/finalize/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(RUN.replace("\"status\":\"in_progress\"", "\"status\":\"completed\""))
        .create();
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-runs/r1/summary/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"total":1,"pass":0,"fail":1,"na":0,"manual":1,"deviations":[{"case_number":"TC-CLI-007","summary":"the password was refused"}]}"#,
        )
        .create();

    let steps: Vec<Vec<&str>> = vec![
        vec![
            "run",
            "create",
            "-p",
            "Demo",
            "--version",
            "1.2.3",
            "--build",
            "42",
            "--commit",
            "abc1234",
            "--env",
            "bench",
            "--run-date",
            "2026-11-09",
        ],
        vec!["run", "add", "RUN-20261109-01", "TC-CLI-007", "-p", "Demo"],
        vec![
            "run",
            "step",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--step",
            "3",
            "--verdict",
            "fail",
            "--actual",
            "the password was refused",
            "--yes",
            "-p",
            "Demo",
        ],
        vec![
            "run",
            "conclude",
            "RUN-20261109-01",
            "--case",
            "TC-CLI-007",
            "--conclusion",
            "FAIL",
            "--evidence",
            "see the report",
            "--yes",
            "-p",
            "Demo",
        ],
        vec!["run", "finalize", "RUN-20261109-01", "-p", "Demo"],
        vec!["run", "summary", "RUN-20261109-01", "-p", "Demo", "--json"],
    ];
    for step in steps {
        let mut args = step.clone();
        args.push("--no-cache");
        let out = run(&args, &server.url());
        assert!(out.status.success(), "{args:?} failed: {}", stderr_of(&out));
    }
    // The summary of the closed batch is the last word of the flow.
    let out = run(
        &[
            "run",
            "summary",
            "RUN-20261109-01",
            "-p",
            "Demo",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    assert_eq!(parsed["total"], 1);
    assert_eq!(parsed["fail"], 1);
}
