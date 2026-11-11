//! The resolvers of the test cases, called against a mock server.
//!
//! The resolvers live in `pbotcli-resolve`, but their tests are here: that
//! crate has no mock server of its own, and the lockfile is outside the
//! scope of this mission — so the crate that already has one (and already
//! depends on the resolve crate) carries the tests. What matters is that
//! the functions are called, not that the call happens in a particular
//! crate.

use mockito::{Matcher, Server};
use pbotcli_client::PlaneClient;
use pbotcli_core::Config;
use pbotcli_resolve::testcase::{resolve_test_case, resolve_test_case_module};

/// A client with the cache off, so every call reaches the mock.
fn client(base: &str) -> PlaneClient {
    PlaneClient::with_cache(
        &Config {
            base_url: base.to_string(),
            api_key: "test-key".into(),
            workspace: "ws".into(),
        },
        true,
    )
    .unwrap()
}

/// A uuid-shaped id: the resolver only reads the detail endpoint for one.
const CASE_ID: &str = "11111111-1111-1111-1111-111111111111";

/// A module the registry knows, under a uuid-shaped id.
const MODULE_ID: &str = "22222222-2222-2222-2222-222222222222";

const CASE: &str = r#"{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"p","object_overview":"o","priority":"P1","preconditions":"c","environment":"e","related_feature":"","remarks":"","status":"active","steps":[],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z"}"#;

const LIST: &str = r#"{"results":[{"id":"tc1","number":"TC-CLI-001","module":"m1","module_code":"CLI","title":"Log in with a valid account","purpose":"p","object_overview":"o","priority":"P1","preconditions":"c","environment":"e","related_feature":"","remarks":"","status":"active","steps":[],"issue_links":[],"created_at":"2026-11-10T08:29:37Z","updated_at":"2026-11-10T08:29:37Z"}],"next_cursor":null,"next_page_results":false}"#;

const MODULES: &str = r#"[{"id":"22222222-2222-2222-2222-222222222222","code":"CLI","description":"The command line of pbot","created_at":"2026-11-10T08:26:36Z","updated_at":"2026-11-10T08:26:36Z"}]"#;

#[tokio::test]
async fn a_uuid_is_read_from_the_detail_endpoint() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            format!("/api/v1/workspaces/ws/projects/p1/test-cases/{CASE_ID}/").as_str(),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(CASE)
        .create_async()
        .await;
    let client = client(&server.url());
    let case = resolve_test_case(&client, "p1", CASE_ID).await.unwrap();
    assert_eq!(case.number, "TC-CLI-001");
    m.assert_async().await;
}

#[tokio::test]
async fn a_number_is_found_through_the_search_of_the_server() {
    let mut server = Server::new_async().await;
    // The search of the server matches the number and the title, and the
    // voided cases are asked for as well — a voided case keeps its number.
    let m = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("search".to_string(), "TC-CLI-001".to_string()),
            Matcher::UrlEncoded("status".to_string(), "all".to_string()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(LIST)
        .create_async()
        .await;
    let client = client(&server.url());
    let case = resolve_test_case(&client, "p1", "TC-CLI-001")
        .await
        .unwrap();
    assert_eq!(case.id, "tc1");
    m.assert_async().await;
}

#[tokio::test]
async fn a_title_is_found_through_the_search_of_the_server() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::UrlEncoded(
            "search".to_string(),
            "Log in with a valid account".to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(LIST)
        .create_async()
        .await;
    let client = client(&server.url());
    let case = resolve_test_case(&client, "p1", "Log in with a valid account")
        .await
        .unwrap();
    assert_eq!(case.number, "TC-CLI-001");
}

#[tokio::test]
async fn the_whole_list_is_the_fallback_for_a_title_the_search_misses() {
    let mut server = Server::new_async().await;
    // The search of the server finds nothing, the whole list does: the
    // resolver falls back to it. (The mock that answers the search is
    // registered last, because mockito answers with the last matching
    // mock.)
    // The search mock is registered first: mockito answers with the first
    // matching mock that still expects a hit, so the search has to come
    // before the catch-all.
    let search = server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::UrlEncoded(
            "search".to_string(),
            "login with a valid".to_string(),
        ))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[],"next_cursor":null,"next_page_results":false}"#)
        .create_async()
        .await;
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(LIST)
        .create_async()
        .await;
    let client = client(&server.url());
    let case = resolve_test_case(&client, "p1", "login with a valid")
        .await
        .unwrap();
    assert_eq!(case.number, "TC-CLI-001");
    search.assert_async().await;
}

#[tokio::test]
async fn a_case_that_does_not_exist_is_a_not_found() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"results":[],"next_cursor":null,"next_page_results":false}"#)
        .create_async()
        .await;
    let client = client(&server.url());
    let err = resolve_test_case(&client, "p1", "nothing like this")
        .await
        .unwrap_err();
    assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_NOT_FOUND);
    assert!(err.to_string().contains("Test case not found"));
}

#[tokio::test]
async fn a_permission_failure_is_not_rewritten_into_a_not_found() {
    let mut server = Server::new_async().await;
    let remedy = "Service principal 'Bot' lacks scope for read test_case in project p1. A workspace admin must add the required scope for this service principal at Workspace settings -> Developer -> Service principals -> Manage scopes.";
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
        .match_query(Matcher::Any)
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(serde_json::json!({"detail": remedy}).to_string())
        .create_async()
        .await;
    let client = client(&server.url());
    let err = resolve_test_case(&client, "p1", "TC-CLI-001")
        .await
        .unwrap_err();
    // A refusal of the server is not a missing case: the exit code and the
    // remedy of the server have to survive.
    assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_AUTH);
    assert!(err.to_string().contains("Manage scopes."));
}

#[tokio::test]
async fn a_module_code_is_matched_whatever_its_case() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-case-modules/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MODULES)
        .create_async()
        .await;
    let client = client(&server.url());
    let module = resolve_test_case_module(&client, "p1", "cli")
        .await
        .unwrap();
    assert_eq!(module.id, MODULE_ID);
    assert_eq!(module.code, "CLI");
    m.assert_async().await;
}

#[tokio::test]
async fn a_module_uuid_is_read_out_of_the_registry() {
    let mut server = Server::new_async().await;
    let m = server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-case-modules/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MODULES)
        .create_async()
        .await;
    let client = client(&server.url());
    let module = resolve_test_case_module(&client, "p1", MODULE_ID)
        .await
        .unwrap();
    // The registry knows the uuid, so the caller gets its code as well.
    assert_eq!(module.id, MODULE_ID);
    assert_eq!(module.code, "CLI");
    assert_eq!(module.description, "The command line of pbot");
    m.assert_async().await;
}

#[tokio::test]
async fn a_module_uuid_the_registry_does_not_know_is_left_to_the_server() {
    let mut server = Server::new_async().await;
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-case-modules/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MODULES)
        .create_async()
        .await;
    let client = client(&server.url());
    let unknown = "33333333-3333-3333-3333-333333333333";
    let module = resolve_test_case_module(&client, "p1", unknown)
        .await
        .unwrap();
    // The server is the authority on which modules belong to the project,
    // and it answers `module_not_in_project`; the uuid travels as it is.
    assert_eq!(module.id, unknown);
    assert!(module.code.is_empty());
}

#[tokio::test]
async fn a_module_code_that_is_not_registered_is_a_not_found() {
    let mut server = Server::new_async().await;
    server
        .mock(
            "GET",
            "/api/v1/workspaces/ws/projects/p1/test-case-modules/",
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MODULES)
        .create_async()
        .await;
    let client = client(&server.url());
    let err = resolve_test_case_module(&client, "p1", "NOPE")
        .await
        .unwrap_err();
    assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_NOT_FOUND);
    let hint = err.hint().expect("a hint");
    assert!(hint.contains("pbot tc module ls"), "{hint}");
}
