//! The client side of the batch feature (`pbot run`).
//!
//! Every endpoint of the feature lives under the project of the URL
//! (`/api/v1/workspaces/{ws}/projects/{pid}/test-runs/…`) and answers its
//! failures with the stable reason codes of [`crate::tcerror`], so every
//! request here carries the mapper of the feature.
//!
//! The reads go through the disk cache like the rest of the workspace, and
//! every write invalidates the keys it made stale — a case that is added to
//! a batch, a verdict that is written, a conclusion that is recorded: the
//! batch and the cases it holds change together.

use std::time::Duration;

use crate::{PlaneClient, body_json};
use pbotcli_core::PlaneError;
use pbotcli_types::testrun::{
    TestRun, TestRunCase, TestRunCaseWrite, TestRunFilters, TestRunStep, TestRunStepWrite,
    TestRunSummary, TestRunWrite,
};

/// How long the list of the batches stays fresh.
const TTL_TEST_RUNS: Duration = Duration::from_secs(60);
/// How long the case records of a batch stays fresh.
const TTL_TEST_RUN_CASES: Duration = Duration::from_secs(60);

/// Cache key prefix of the list of the batches of a project.
///
/// A write invalidates this prefix, and the key of a filtered list starts
/// with it — so a write clears the filtered lists as well.
fn runs_prefix(workspace: &str, project_id: &str) -> String {
    format!("test_runs:{workspace}:{project_id}")
}

/// Cache key of the list of the batches of a project.
///
/// The filters are part of the key: a filtered read must not be served to
/// an unfiltered one. Their signature is a hash of the raw filter values,
/// so two different filter sets never share a key.
fn runs_key(workspace: &str, project_id: &str, filters: &TestRunFilters) -> String {
    format!(
        "{}:{}",
        runs_prefix(workspace, project_id),
        filter_signature(filters)
    )
}

/// The signature of a filter set: a 64-bit FNV-1a hash of the raw values.
fn filter_signature(filters: &TestRunFilters) -> String {
    let raw = format!(
        "{}\u{1f}{}",
        filters.status.as_deref().unwrap_or_default(),
        filters.run_date.as_deref().unwrap_or_default(),
    );
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in raw.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Cache key of one batch.
fn run_key(workspace: &str, project_id: &str, run_id: &str) -> String {
    format!("test_run:{workspace}:{project_id}:{run_id}")
}

/// Cache key of the case records of one batch.
fn cases_key(workspace: &str, project_id: &str, run_id: &str) -> String {
    format!("test_run_cases:{workspace}:{project_id}:{run_id}")
}

/// The path of the batches of a project.
fn runs_path(workspace: &str, project_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace}/projects/{project_id}/test-runs/")
}

/// The path of one batch.
fn run_path(workspace: &str, project_id: &str, run_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace}/projects/{project_id}/test-runs/{run_id}/")
}

/// The path of the case records of one batch.
fn cases_path(workspace: &str, project_id: &str, run_id: &str) -> String {
    format!("/api/v1/workspaces/{workspace}/projects/{project_id}/test-runs/{run_id}/cases/")
}

/// The path of one case record of one batch.
fn case_path(workspace: &str, project_id: &str, run_id: &str, case_id: &str) -> String {
    format!(
        "/api/v1/workspaces/{workspace}/projects/{project_id}/test-runs/{run_id}/cases/{case_id}/"
    )
}

/// The path of one step record of one case record of one batch.
fn step_path(
    workspace: &str,
    project_id: &str,
    run_id: &str,
    case_id: &str,
    sequence_id: i64,
) -> String {
    format!(
        "/api/v1/workspaces/{workspace}/projects/{project_id}/test-runs/{run_id}/cases/{case_id}/steps/{sequence_id}/"
    )
}

impl PlaneClient {
    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-runs/`
    ///
    /// The filters are the ones of the endpoint: `status` (`draft`,
    /// `in_progress`, `completed`, or `all` to widen it) and `run_date`
    /// (the day of the execution).
    pub async fn list_test_runs(
        &self,
        project_id: &str,
        filters: &TestRunFilters,
    ) -> Result<Vec<TestRun>, PlaneError> {
        let key = runs_key(&self.workspace, project_id, filters);
        let path = runs_path(&self.workspace, project_id);
        let query = filters_query(filters);
        self.cached_list(&key, TTL_TEST_RUNS, async move {
            self.paginate_with(&path, crate::testcase_error_mapper, &query)
                .await
        })
        .await
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/`
    ///
    /// The read of one batch is not cached: the cache holds the lists,
    /// and a batch is read once per command.
    pub async fn get_test_run(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> Result<TestRun, PlaneError> {
        let path = run_path(&self.workspace, project_id, run_id);
        self.request_json_with(
            reqwest::Method::GET,
            &path,
            &[],
            None,
            crate::testcase_error_mapper,
        )
        .await
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/test-runs/`
    ///
    /// The number of the batch is assigned by the server — it allocates it
    /// from the day the batch carries — so the client never writes one.
    pub async fn create_test_run(
        &self,
        project_id: &str,
        body: &TestRunWrite,
    ) -> Result<TestRun, PlaneError> {
        let path = runs_path(&self.workspace, project_id);
        let result = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_run(project_id, None);
        result
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/`
    ///
    /// The endpoint answers a plain array (not the cursor envelope of the
    /// list of the batches).
    pub async fn list_test_run_cases(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> Result<Vec<TestRunCase>, PlaneError> {
        let key = cases_key(&self.workspace, project_id, run_id);
        let path = cases_path(&self.workspace, project_id, run_id);
        self.cached_list(&key, TTL_TEST_RUN_CASES, async move {
            self.request_json_with::<Vec<TestRunCase>>(
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

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/`
    ///
    /// The body carries the uuids of the test cases; the server snapshots
    /// every one of them into the batch. A case that is already in the
    /// batch is refused with `duplicate_case_number` — as a 409.
    pub async fn add_test_run_cases(
        &self,
        project_id: &str,
        run_id: &str,
        case_ids: &[String],
    ) -> Result<Vec<TestRunCase>, PlaneError> {
        let path = cases_path(&self.workspace, project_id, run_id);
        let body = serde_json::json!({ "test_case_ids": case_ids });
        let result = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                Some(&body),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_run(project_id, Some(run_id));
        result
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/{rc}/`
    pub async fn get_test_run_case(
        &self,
        project_id: &str,
        run_id: &str,
        case_id: &str,
    ) -> Result<TestRunCase, PlaneError> {
        let path = case_path(&self.workspace, project_id, run_id, case_id);
        self.request_json_with(
            reqwest::Method::GET,
            &path,
            &[],
            None,
            crate::testcase_error_mapper,
        )
        .await
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/{rc}/`
    ///
    /// The conclusion of a case record is written once — the server
    /// refuses a second write with `conclusion_is_final`.
    pub async fn update_test_run_case(
        &self,
        project_id: &str,
        run_id: &str,
        case_id: &str,
        body: &TestRunCaseWrite,
    ) -> Result<TestRunCase, PlaneError> {
        let path = case_path(&self.workspace, project_id, run_id, case_id);
        let result = self
            .request_json_with(
                reqwest::Method::PATCH,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_run(project_id, Some(run_id));
        result
    }

    /// `DELETE /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/{rc}/`
    ///
    /// A record that already carries a verdict or a conclusion is refused
    /// with 409 `run_case_removal_after_execution`: removing it would hand
    /// out a fresh record inside the same batch.
    pub async fn remove_test_run_case(
        &self,
        project_id: &str,
        run_id: &str,
        case_id: &str,
    ) -> Result<(), PlaneError> {
        let path = case_path(&self.workspace, project_id, run_id, case_id);
        let result: Result<serde_json::Value, PlaneError> = self
            .request_json_with(
                reqwest::Method::DELETE,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_run(project_id, Some(run_id));
        result.map(|_| ())
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/{rc}/steps/{n}/`
    ///
    /// The read is what tells the caller the uuid of the step record — the
    /// identifier a file of the step is bound to.
    pub async fn get_test_run_step(
        &self,
        project_id: &str,
        run_id: &str,
        case_id: &str,
        sequence_id: i64,
    ) -> Result<TestRunStep, PlaneError> {
        let path = step_path(&self.workspace, project_id, run_id, case_id, sequence_id);
        self.request_json_with(
            reqwest::Method::GET,
            &path,
            &[],
            None,
            crate::testcase_error_mapper,
        )
        .await
    }

    /// `PATCH /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/cases/{rc}/steps/{n}/`
    ///
    /// The verdict of a step is written once — the server refuses a second
    /// write with `conclusion_is_final`.
    pub async fn update_test_run_step(
        &self,
        project_id: &str,
        run_id: &str,
        case_id: &str,
        sequence_id: i64,
        body: &TestRunStepWrite,
    ) -> Result<TestRunStep, PlaneError> {
        let path = step_path(&self.workspace, project_id, run_id, case_id, sequence_id);
        let result = self
            .request_json_with(
                reqwest::Method::PATCH,
                &path,
                &[],
                Some(&body_json(body)?),
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_run(project_id, Some(run_id));
        result
    }

    /// `POST /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/finalize/`
    ///
    /// The server runs the validity rules over the batch and answers every
    /// problem it finds, one entry per problem. A refused finalize leaves
    /// the batch open — it is not frozen by a failed check.
    pub async fn finalize_test_run(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> Result<TestRun, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-runs/{}/finalize/",
            self.workspace, project_id, run_id
        );
        let result = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await;
        self.invalidate_test_run(project_id, Some(run_id));
        result
    }

    /// `GET /api/v1/workspaces/{ws}/projects/{pid}/test-runs/{id}/summary/`
    ///
    /// `Ok(None)` when the instance does not serve the endpoint: a 404
    /// that does not name the batch is the endpoint missing, not the batch
    /// missing. The counts the endpoint answers are not a partition of the
    /// total — `manual` overlaps `fail` — so the CLI never checks their
    /// sum against the total.
    pub async fn get_test_run_summary(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> Result<Option<TestRunSummary>, PlaneError> {
        let path = format!(
            "/api/v1/workspaces/{}/projects/{}/test-runs/{}/summary/",
            self.workspace, project_id, run_id
        );
        match self
            .request_json_with::<TestRunSummary>(
                reqwest::Method::GET,
                &path,
                &[],
                None,
                crate::testcase_error_mapper,
            )
            .await
        {
            Ok(summary) => Ok(Some(summary)),
            // The batch is missing when the server names it; an endpoint
            // this instance does not serve answers a 404 without a name.
            Err(PlaneError::NotFound { message, .. }) if !names_the_batch(&message) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Drop the cache of a batch after a write to it.
    ///
    /// The case records of a batch belong to it, so a write to any of them
    /// makes the list of the batches stale as well.
    fn invalidate_test_run(&self, project_id: &str, run_id: Option<&str>) {
        self.invalidate(&runs_prefix(&self.workspace, project_id));
        if let Some(run_id) = run_id {
            self.invalidate(&run_key(&self.workspace, project_id, run_id));
            self.invalidate(&cases_key(&self.workspace, project_id, run_id));
        }
    }
}

/// True when the message of a 404 names the batch that is missing.
///
/// The server answers `{"error": "Test run not found"}` for a batch that
/// does not exist and a plain `{"detail": "Not found."}` for an endpoint
/// the instance does not serve — the two are told apart by the name.
fn names_the_batch(message: &str) -> bool {
    message.to_lowercase().contains("test run not found")
}

/// The query parameters of the list of the batches.
fn filters_query(filters: &TestRunFilters) -> Vec<(&'static str, String)> {
    let mut query: Vec<(&'static str, String)> = Vec::new();
    for (key, value) in [
        ("status", filters.status.as_deref()),
        ("run_date", filters.run_date.as_deref()),
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
    use pbotcli_types::testrun::TestRunCaseWrite;

    fn cfg(base: &str) -> pbotcli_core::Config {
        pbotcli_core::Config {
            base_url: base.to_string(),
            api_key: "test-key".into(),
            workspace: "ws".into(),
        }
    }

    #[tokio::test]
    async fn the_create_body_carries_no_number_and_no_status() {
        let mut server = mockito::Server::new_async().await;
        // The body is pinned exactly: the nulls are dropped, so the server
        // reads every field that is missing as "leave this alone" — and
        // the number is never written by the client.
        let m = server
            .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-runs/")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "run_date": "2026-11-09",
                "version_number": "1.2.3",
                "build_number": "42",
                "commit_hash": "abc1234",
                "environment": "bench",
                "executed_by": "u1",
            })))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"r1","number":"RUN-20261109-01"}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let created = client
            .create_test_run(
                "p1",
                &TestRunWrite {
                    run_date: Some("2026-11-09".into()),
                    version_number: Some("1.2.3".into()),
                    build_number: Some("42".into()),
                    commit_hash: Some("abc1234".into()),
                    environment: Some("bench".into()),
                    executed_by: Some("u1".into()),
                    status: None,
                    retest_of: None,
                },
            )
            .await
            .unwrap();
        // The number is the server's, never the client's.
        assert_eq!(created.number, "RUN-20261109-01");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn the_cases_are_added_with_their_uuids() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock(
                "POST",
                "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "test_case_ids": ["tc1", "tc2"],
            })))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"rc1","case_number":"TC-CLI-007"}]"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let cases = client
            .add_test_run_cases("p1", "r1", &["tc1".to_string(), "tc2".to_string()])
            .await
            .unwrap();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].case_number, "TC-CLI-007");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn a_duplicate_case_arrives_as_a_conflict_with_its_code() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("POST", "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/")
            .with_status(409)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"error":"The test case is already in this batch.","code":"duplicate_case_number","errors":[
                    {"code":"duplicate_case_number","message":"同一个用例在批次中重复出现。","path":"test_case_ids","field":"test_case_ids"}]}"#,
            )
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client
            .add_test_run_cases("p1", "r1", &["tc1".to_string()])
            .await
            .unwrap_err();
        // A 409 of a known reason is a refusal of the write, not a
        // transport failure.
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("duplicate_case_number"));
    }

    #[tokio::test]
    async fn the_verdict_of_a_step_is_written_on_its_own_row() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock(
                "PATCH",
                "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/steps/3/",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "conclusion": "fail",
                "actual_value": "the password was refused",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"s1","sequence_id":3,"conclusion":"fail"}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let step = client
            .update_test_run_step(
                "p1",
                "r1",
                "rc1",
                3,
                &TestRunStepWrite {
                    conclusion: Some("fail".into()),
                    actual_value: Some("the password was refused".into()),
                },
            )
            .await
            .unwrap();
        assert_eq!(step.conclusion, "fail");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn the_conclusion_of_a_case_is_written_with_the_other_fields() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock(
                "PATCH",
                "/api/v1/workspaces/ws/projects/p1/test-runs/r1/cases/rc1/",
            )
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "conclusion": "FAIL",
                "evidence": "see the screenshot",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"rc1","case_number":"TC-CLI-007","conclusion":"FAIL"}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let case = client
            .update_test_run_case(
                "p1",
                "r1",
                "rc1",
                &TestRunCaseWrite {
                    conclusion: Some("FAIL".into()),
                    evidence: Some("see the screenshot".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(case.conclusion, "FAIL");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn a_summary_the_instance_does_not_serve_degrades_to_nothing() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock(
                "GET",
                "/api/v1/workspaces/ws/projects/p1/test-runs/r1/summary/",
            )
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"detail":"Not found."}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let summary = client.get_test_run_summary("p1", "r1").await.unwrap();
        assert!(
            summary.is_none(),
            "an endpoint the instance does not serve is not a crash"
        );
    }

    #[tokio::test]
    async fn a_missing_batch_is_still_a_missing_batch() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock(
                "GET",
                "/api/v1/workspaces/ws/projects/p1/test-runs/r1/summary/",
            )
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error":"Test run not found"}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client.get_test_run_summary("p1", "r1").await.unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_NOT_FOUND);
    }

    #[tokio::test]
    async fn the_summary_is_read_with_its_counts_and_deviations() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock(
                "GET",
                "/api/v1/workspaces/ws/projects/p1/test-runs/r1/summary/",
            )
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"total":1,"pass":0,"fail":1,"na":0,"manual":1,
                    "deviations":[{"case_number":"TC-CLI-007","summary":"the password was refused"}]}"#,
            )
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let summary = client
            .get_test_run_summary("p1", "r1")
            .await
            .unwrap()
            .expect("a summary");
        assert_eq!(summary.total, 1);
        assert_eq!(summary.fail, 1);
        assert_eq!(summary.deviations.len(), 1);
    }

    #[tokio::test]
    async fn the_filters_reach_the_list_endpoint() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/api/v1/workspaces/ws/projects/p1/test-runs/")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("status".to_string(), "completed".to_string()),
                mockito::Matcher::UrlEncoded("run_date".to_string(), "2026-11-09".to_string()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"results":[],"next_cursor":null,"next_page_results":false}"#)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let filters = TestRunFilters {
            status: Some("completed".into()),
            run_date: Some("2026-11-09".into()),
        };
        let runs = client.list_test_runs("p1", &filters).await.unwrap();
        assert!(runs.is_empty());
        m.assert_async().await;
    }

    #[test]
    fn the_prefix_a_write_invalidates_covers_the_filtered_lists() {
        let filters = TestRunFilters {
            status: Some("completed".into()),
            ..Default::default()
        };
        let key = runs_key("ws", "p1", &filters);
        assert!(
            key.starts_with(&runs_prefix("ws", "p1")),
            "a write invalidates the prefix, so it has to clear the filtered lists: {key}"
        );
        // The prefix of one resource does not swallow the prefix of another.
        assert!(!run_key("ws", "p1", "r1").starts_with(&runs_prefix("ws", "p1")));
        assert!(!cases_key("ws", "p1", "r1").starts_with(&runs_prefix("ws", "p1")));
    }

    #[test]
    fn two_filter_sets_never_share_a_cache_key() {
        let base = TestRunFilters {
            status: Some("draft".into()),
            ..Default::default()
        };
        let other = TestRunFilters {
            status: Some("completed".into()),
            ..Default::default()
        };
        assert_ne!(
            filter_signature(&base),
            filter_signature(&other),
            "a filtered read must not be served to another filter"
        );
        assert_eq!(filter_signature(&base), filter_signature(&base));
    }

    #[test]
    fn a_missing_batch_is_told_apart_from_a_missing_endpoint() {
        assert!(names_the_batch("Not found (HTTP 404): Test run not found"));
        assert!(!names_the_batch("Not found (HTTP 404): Not found."));
    }
}
