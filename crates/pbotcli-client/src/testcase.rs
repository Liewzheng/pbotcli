//! The client side of the test-case feature.
//!
//! Every endpoint of the feature lives under the project of the URL
//! (`/api/v1/workspaces/{ws}/projects/{pid}/…`) and answers its failures
//! with the stable reason codes of [`crate::tcerror`], so every request
//! here carries the mapper of the feature.
//!
//! The reads go through the disk cache like the rest of the workspace, and
//! every write invalidates the keys it made stale — the step table and the
//! work-item links of a case belong to the case, so a write to one of them
//! invalidates the list of the cases as well.

use std::time::Duration;

use crate::{PlaneClient, body_json};
use pbotcli_core::PlaneError;
use pbotcli_types::testcase::{
    TestCase, TestCaseFilters, TestCaseIssueLink, TestCaseIssueWrite, TestCaseModule,
    TestCaseModuleWrite, TestCaseStep, TestCaseStepsResult, TestCaseStepsWrite, TestCaseWrite,
};

/// How long the registry of the module codes stays fresh.
const TTL_TEST_CASE_MODULES: Duration = Duration::from_secs(300);
/// How long the list of the cases stays fresh.
const TTL_TEST_CASES: Duration = Duration::from_secs(60);
/// How long the step table of a case stays fresh.
const TTL_TEST_CASE_STEPS: Duration = Duration::from_secs(60);
/// How long the work-item links of a case stay fresh.
const TTL_TEST_CASE_ISSUES: Duration = Duration::from_secs(60);

/// Cache key of the registry of the module codes.
fn modules_key(workspace: &str, project_id: &str) -> String {
    format!("test_case_modules:{workspace}:{project_id}")
}

/// Cache key prefix of the list of the cases of a project.
///
/// A write invalidates this prefix, and the key of a filtered list starts
/// with it — so a write clears the filtered lists as well. The cache drops
/// every entry whose key starts with the prefix, and its sanitizer maps
/// every character on its own, so a prefix of the raw keys is a prefix of
/// the sanitized ones.
fn cases_prefix(workspace: &str, project_id: &str) -> String {
    format!("test_cases:{workspace}:{project_id}")
}

/// Cache key of the list of the cases of a project.
///
/// The filters are part of the key: a filtered read must not be served to
/// an unfiltered one. Their signature is a hash of the raw filter values,
/// so two different filter sets never share a key.
fn cases_key(workspace: &str, project_id: &str, filters: &TestCaseFilters) -> String {
    format!(
        "{}:{}",
        cases_prefix(workspace, project_id),
        filter_signature(filters)
    )
}

/// The signature of a filter set: a 64-bit FNV-1a hash of the raw values.
fn filter_signature(filters: &TestCaseFilters) -> String {
    let raw = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        filters.module.as_deref().unwrap_or_default(),
        filters.priority.as_deref().unwrap_or_default(),
        filters.issue.as_deref().unwrap_or_default(),
        filters.status.as_deref().unwrap_or_default(),
        filters.search.as_deref().unwrap_or_default(),
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in raw.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Cache key of the step table of one case.
fn steps_key(workspace: &str, project_id: &str, case_id: &str) -> String {
    format!("test_case_steps:{workspace}:{project_id}:{case_id}")
}

/// Cache key of the work-item links of one case.
fn issues_key(workspace: &str, project_id: &str, case_id: &str) -> String {
    format!("test_case_issues:{workspace}:{project_id}:{case_id}")
}

impl PlaneClient {
    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-case-modules/`
    ///
    /// The registry answers a plain array (not the cursor envelope of the
    /// other list endpoints).
    pub async fn list_test_case_modules(
        &self,
        project_id: &str,
    ) -> Result<Vec<TestCaseModule>, PlaneError> {
        let key = modules_key(&self.workspace, project_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-case-modules/",
            self.workspace, project_id
        );
        self.cached_list(&key, TTL_TEST_CASE_MODULES, async move {
            self.request_json_with::<Vec<TestCaseModule>>(
                reqwest::Method::GET,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await
        })
        .await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/test-case-modules/`
    ///
    /// The server owns the rules of a module code (2-6 uppercase letters,
    /// unique inside the project) and answers a refused code with its own
    /// stable code — the client does not second-guess it.
    pub async fn create_test_case_module(
        &self,
        project_id: &str,
        body: &TestCaseModuleWrite,
    ) -> Result<TestCaseModule, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-case-modules/",
            self.workspace, project_id
        );
        let result = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate(&modules_key(&self.workspace, project_id));
        result
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-cases/`
    ///
    /// The filters are the ones of the endpoint: `module` (a uuid or a
    /// code), `priority`, `issue` (a work-item uuid), `status` (`active`
    /// is the default of the server, `all` widens it) and `search` (the
    /// number and the title of a case).
    pub async fn list_test_cases(
        &self,
        project_id: &str,
        filters: &TestCaseFilters,
    ) -> Result<Vec<TestCase>, PlaneError> {
        let key = cases_key(&self.workspace, project_id, filters);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/",
            self.workspace, project_id
        );
        let query = filters_query(filters);
        self.cached_list(&key, TTL_TEST_CASES, async move {
            self.paginate_with(&path, crate::testcase_error_mapper, &query)
                .await
        })
        .await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/`
    pub async fn get_test_case(
        &self,
        project_id: &str,
        case_id: &str,
    ) -> Result<TestCase, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/",
            self.workspace, project_id, case_id
        );
        self.request_json_with(
            reqwest::Method::GET,
            &path,
            &[],
            None,
            crate::testcase_error_mapper,
        )
        .await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/test-cases/`
    ///
    /// The number of the case is assigned by the server; the client never
    /// writes one.
    pub async fn create_test_case(
        &self,
        project_id: &str,
        body: &TestCaseWrite,
    ) -> Result<TestCase, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/",
            self.workspace, project_id
        );
        let result = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_case(project_id, None);
        result
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/`
    pub async fn update_test_case(
        &self,
        project_id: &str,
        case_id: &str,
        body: &TestCaseWrite,
    ) -> Result<TestCase, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/",
            self.workspace, project_id, case_id
        );
        let result = self
            .request_json_with(
                reqwest::Method::PATCH,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_case(project_id, Some(case_id));
        result
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/`
    ///
    /// The server refuses the removal of a case to a caller that is not an
    /// admin of the project; the refusal carries the server's own sentence.
    pub async fn delete_test_case(
        &self,
        project_id: &str,
        case_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/",
            self.workspace, project_id, case_id
        );
        let result: Result<serde_json::Value, PlaneError> = self
            .request_json_with(
                reqwest::Method::DELETE,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_case(project_id, Some(case_id));
        result.map(|_| ())
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/steps/`
    pub async fn list_test_case_steps(
        &self,
        project_id: &str,
        case_id: &str,
    ) -> Result<Vec<TestCaseStep>, PlaneError> {
        let key = steps_key(&self.workspace, project_id, case_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/steps/",
            self.workspace, project_id, case_id
        );
        self.cached_list(&key, TTL_TEST_CASE_STEPS, async move {
            self.request_json_with::<Vec<TestCaseStep>>(
                reqwest::Method::GET,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await
        })
        .await
    }

    /// `PUT /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/steps/`
    ///
    /// The whole step table is replaced; the numbers of the steps are
    /// assigned by the server.
    pub async fn replace_test_case_steps(
        &self,
        project_id: &str,
        case_id: &str,
        body: &TestCaseStepsWrite,
    ) -> Result<TestCaseStepsResult, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/steps/",
            self.workspace, project_id, case_id
        );
        let result = self
            .request_json_with(
                reqwest::Method::PUT,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_case(project_id, Some(case_id));
        result
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/issues/`
    pub async fn list_test_case_issues(
        &self,
        project_id: &str,
        case_id: &str,
    ) -> Result<Vec<TestCaseIssueLink>, PlaneError> {
        let key = issues_key(&self.workspace, project_id, case_id);
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/issues/",
            self.workspace, project_id, case_id
        );
        self.cached_list(&key, TTL_TEST_CASE_ISSUES, async move {
            self.request_json_with::<Vec<TestCaseIssueLink>>(
                reqwest::Method::GET,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await
        })
        .await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/issues/`
    pub async fn add_test_case_issue(
        &self,
        project_id: &str,
        case_id: &str,
        issue_id: &str,
    ) -> Result<TestCaseIssueLink, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/issues/",
            self.workspace, project_id, case_id
        );
        let body = TestCaseIssueWrite {
            issue_id: issue_id.to_string(),
        };
        let result = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                Some(&body_json(&body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_case(project_id, Some(case_id));
        result
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{pid}/test-cases/{id}/issues/{issue_id}/`
    pub async fn remove_test_case_issue(
        &self,
        project_id: &str,
        case_id: &str,
        issue_id: &str,
    ) -> Result<(), PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-cases/{}/issues/{}/",
            self.workspace, project_id, case_id, issue_id
        );
        let result: Result<serde_json::Value, PlaneError> = self
            .request_json_with(
                reqwest::Method::DELETE,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_case(project_id, Some(case_id));
        result.map(|_| ())
    }

    /// Drop the cache of a case after a write to it.
    ///
    /// The step table and the work-item links are part of the case, so a
    /// write to any of them makes the list of the cases stale as well.
    fn invalidate_test_case(&self, project_id: &str, case_id: Option<&str>) {
        self.invalidate(&cases_prefix(&self.workspace, project_id));
        if let Some(case_id) = case_id {
            self.invalidate(&steps_key(&self.workspace, project_id, case_id));
            self.invalidate(&issues_key(&self.workspace, project_id, case_id));
        }
    }
}

/// The query parameters of the list of the cases.
fn filters_query(filters: &TestCaseFilters) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::new();
    for (key, value) in [
        ("module", filters.module.as_deref()),
        ("priority", filters.priority.as_deref()),
        ("issue", filters.issue.as_deref()),
        ("status", filters.status.as_deref()),
        ("search", filters.search.as_deref()),
    ] {
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            query.push((key, value.to_string()));
        }
    }
    query
}

#[cfg(test)]
mod tests {
    use super::*;
    use pbotcli_types::testcase::TestCaseStepWrite;

    fn cfg(base: &str) -> pbotcli_core::Config {
        pbotcli_core::Config {
            base_url: base.to_string(),
            api_key: "test-key".into(),
            workspace: "ws".into(),
        }
    }

    #[tokio::test]
    async fn a_write_carries_only_the_fields_that_were_set() {
        let mut server = mockito::Server::new_async().await;
        // The body is pinned exactly: the nulls are dropped, so the server
        // reads every field that is missing as "leave this alone".
        let m = server
            .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-cases/")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "title": "A title",
            })))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"tc1","number":"TC-CLI-001"}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let created = client
            .create_test_case(
                "p1",
                &TestCaseWrite {
                    title: Some("A title".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        // The number is the server's, never the client's.
        assert_eq!(created.number, "TC-CLI-001");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn the_step_table_is_sent_as_a_whole() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("PUT", "/api/v1/workspaces/ws/projects/p1/test-cases/tc1/steps/")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "steps": [
                    {"action": "Start pbot", "expected_result": "The form is shown"},
                    {"action": "Log in", "expected_result": "The dashboard is shown"}
                ]
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"updated_at":"2026-11-11T00:00:00Z","steps":[{"id":"s1","sequence_id":1,"action":"Start pbot","expected_result":"The form is shown"}]}"#,
            )
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let result = client
            .replace_test_case_steps(
                "p1",
                "tc1",
                &TestCaseStepsWrite {
                    steps: vec![
                        TestCaseStepWrite {
                            action: "Start pbot".into(),
                            expected_result: "The form is shown".into(),
                        },
                        TestCaseStepWrite {
                            action: "Log in".into(),
                            expected_result: "The dashboard is shown".into(),
                        },
                    ],
                },
            )
            .await
            .unwrap();
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.updated_at, "2026-11-11T00:00:00Z");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn the_filters_reach_the_list_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("module".to_string(), "CLI".to_string()),
                mockito::Matcher::UrlEncoded("priority".to_string(), "P1".to_string()),
                mockito::Matcher::UrlEncoded("status".to_string(), "all".to_string()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"results":[],"next_cursor":null,"next_page_results":false}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let filters = TestCaseFilters {
            module: Some("CLI".into()),
            priority: Some("P1".into()),
            status: Some("all".into()),
            ..Default::default()
        };
        let cases = client.list_test_cases("p1", &filters).await.unwrap();
        assert!(cases.is_empty());
        m.assert_async().await;
    }

    #[tokio::test]
    async fn a_refused_write_keeps_the_stable_code_of_the_server() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-case-modules/")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"error":"The module code is not valid.","code":"module_code_format","errors":[
                    {"code":"module_code_format","message":"模块码必须是 2 至 6 位大写字母。","path":"code","field":"code"}]}"#,
            )
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client
            .create_test_case_module(
                "p1",
                &TestCaseModuleWrite {
                    code: Some("TOO-LONG".into()),
                    description: Some("x".into()),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        let message = err.to_string();
        assert!(message.contains("module_code_format"), "{message}");
        assert!(
            message.contains("模块码必须是 2 至 6 位大写字母。"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn a_permission_failure_is_an_auth_error_with_the_remedy() {
        let mut server = mockito::Server::new_async().await;
        let remedy = "Service principal 'Bot' lacks scope for read test_case in project p1. A workspace admin must add the required scope for this service principal at Workspace settings -> Developer -> Service principals -> Manage scopes.";
        server
            .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-cases/")
            .match_query(mockito::Matcher::Any)
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(serde_json::json!({"detail": remedy}).to_string())
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client
            .list_test_cases("p1", &TestCaseFilters::default())
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_AUTH);
        assert!(err.to_string().contains("Manage scopes."));
    }

    #[test]
    fn the_prefix_a_write_invalidates_covers_the_filtered_lists() {
        let filters = TestCaseFilters {
            module: Some("CLI".into()),
            priority: Some("P1".into()),
            ..Default::default()
        };
        let key = cases_key("ws", "p1", &filters);
        assert!(
            key.starts_with(&cases_prefix("ws", "p1")),
            "a write invalidates the prefix, so it has to clear the filtered lists: {key}"
        );
        // The prefix of one resource does not swallow the prefix of another.
        assert!(!modules_key("ws", "p1").starts_with(&cases_prefix("ws", "p1")));
        assert!(!steps_key("ws", "p1", "tc1").starts_with(&cases_prefix("ws", "p1")));
        assert!(!issues_key("ws", "p1", "tc1").starts_with(&cases_prefix("ws", "p1")));
    }

    #[test]
    fn two_filter_sets_never_share_a_cache_key() {
        let base = TestCaseFilters {
            module: Some("CLI".into()),
            ..Default::default()
        };
        let other = TestCaseFilters {
            module: Some("PWR".into()),
            ..Default::default()
        };
        assert_ne!(
            filter_signature(&base),
            filter_signature(&other),
            "a filtered read must not be served to another filter"
        );
        assert_eq!(filter_signature(&base), filter_signature(&base));
    }
}
