//! DTOs for the batch side of the test-case feature.
//!
//! A **batch** is one continuous execution of test cases. It holds
//! **case records** — the snapshot of a case that was added to the
//! batch — and every case record holds the **step records** that carry
//! the verdict (pass / fail / unset) and the measured value of one step.
//!
//! The field names are the contract: the v1 surface reuses the app
//! serializers verbatim, so the shapes below are the shapes the server
//! hands out. Every read field but the id is optional-tolerant
//! (`#[serde(default)]`) because the server omits a field it has not
//! populated (a batch without an executor, a step record without a
//! verdict).
//!
//! Two rules of the feature are visible in these types:
//!
//! * the number of a batch (`RUN-<YYYYMMDD>-<NN>`) is assigned by the
//!   server from the day the batch carries — the client never writes it
//!   and never builds one;
//! * the conclusion of a case record and the verdict of a step record
//!   are written **once**: the server refuses a second write with
//!   `conclusion_is_final`, and the fields around them stay writable.

use serde::{Deserialize, Serialize};

use crate::Member;
use crate::null_to_empty;

/// The conclusions a case record of a batch may carry.
///
/// The server stores them uppercased (`validate_conclusion` of the
/// write serializer).
pub const RUN_CASE_CONCLUSIONS: [&str; 4] = ["PASS", "FAIL", "N/A", "MANUAL"];

/// The verdicts a step record of a batch may carry.
///
/// The server stores them lowercased. `unset` is the placeholder of a
/// step that was never executed — it is **not** a verdict, and a
/// measured value without a verdict is an unfinished record.
pub const STEP_VERDICTS: [&str; 3] = ["pass", "fail", "unset"];

/// A batch as it is read.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRun {
    pub id: String,
    /// The number the server assigned (`RUN-<YYYYMMDD>-<NN>`).
    #[serde(default)]
    pub number: String,
    /// The day of the execution (`YYYY-MM-DD`).
    #[serde(default)]
    pub run_date: String,
    #[serde(default)]
    pub version_number: String,
    #[serde(default)]
    pub build_number: String,
    #[serde(default)]
    pub commit_hash: String,
    #[serde(default)]
    pub environment: String,
    /// The uuid of the user that executed the batch.
    ///
    /// The server answers `null` for a batch that was created without an
    /// executor, so the field tolerates a `null` and reads it as the
    /// empty string.
    #[serde(default, deserialize_with = "null_to_empty")]
    pub executed_by: String,
    /// The name of that user — the snapshot the server keeps next to
    /// the id, so the history of the batch survives the deletion of the
    /// user.
    #[serde(default)]
    pub executed_by_name: String,
    /// draft | in_progress | completed.
    #[serde(default)]
    pub status: String,
    /// The uuid of the batch this one re-tests.
    #[serde(default)]
    pub retest_of: Option<String>,
    /// The number of that batch.
    ///
    /// Read-only, and it survives the soft delete of the original: a
    /// `null` here means "there is no original batch", never "the
    /// original batch is gone".
    #[serde(default)]
    pub retest_of_number: Option<String>,
    #[serde(default)]
    pub cases: Vec<TestRunCase>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// The creator of the batch, as the server hands it out: the same
    /// user payload the member list carries.
    #[serde(default)]
    pub created_by: Option<Member>,
}

/// A batch as it is written.
///
/// Every field is optional: the client strips the ones it does not set,
/// so an unset field means "leave this alone" (ADR-0007).
///
/// The number of a batch is not a field — the server allocates it from
/// the day of the batch. The `status` is here because the server
/// accepts one, but the CLI never writes it: a batch is completed
/// through the finalize endpoint, which runs the validity rules first.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestRunWrite {
    pub run_date: Option<String>,
    pub version_number: Option<String>,
    pub build_number: Option<String>,
    pub commit_hash: Option<String>,
    pub environment: Option<String>,
    /// The uuid of the user that executed the batch.
    pub executed_by: Option<String>,
    /// draft | in_progress — the server refuses `completed` here and
    /// completes the batch through `finalize`.
    pub status: Option<String>,
    /// The uuid of the batch this one re-tests.
    pub retest_of: Option<String>,
}

impl TestRunWrite {
    /// True when the body carries no field at all — a write of nothing.
    pub fn is_empty(&self) -> bool {
        self.run_date.is_none()
            && self.version_number.is_none()
            && self.build_number.is_none()
            && self.commit_hash.is_none()
            && self.environment.is_none()
            && self.executed_by.is_none()
            && self.status.is_none()
            && self.retest_of.is_none()
    }
}

/// The filters the list endpoint of the batches understands.
#[derive(Debug, Clone, Default)]
pub struct TestRunFilters {
    /// draft | in_progress | completed | all.
    pub status: Option<String>,
    /// The day of the execution (`YYYY-MM-DD`).
    pub run_date: Option<String>,
}

/// One case record of a batch — the snapshot of a test case.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRunCase {
    pub id: String,
    /// The uuid of the test case this record was made of.
    ///
    /// The server answers `null` for a record whose test case was
    /// deleted, so the field tolerates a `null` and reads it as the empty
    /// string — the same shape the other absent fields of the record
    /// carry.
    #[serde(default, deserialize_with = "null_to_empty")]
    pub test_case: String,
    #[serde(default)]
    pub case_number: String,
    #[serde(default)]
    pub case_module_code: String,
    #[serde(default)]
    pub case_title: String,
    #[serde(default)]
    pub case_purpose: String,
    #[serde(default)]
    pub case_object_overview: String,
    #[serde(default)]
    pub case_priority: String,
    #[serde(default)]
    pub case_preconditions: String,
    #[serde(default)]
    pub case_environment: String,
    /// PASS | FAIL | N/A | MANUAL — empty while the case was not
    /// concluded.
    #[serde(default)]
    pub conclusion: String,
    /// The evidence of the case, as the Markdown source that was typed.
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub remarks: String,
    #[serde(default)]
    pub actual_result_summary: String,
    #[serde(default)]
    pub na_reason: String,
    #[serde(default)]
    pub na_criteria_source: String,
    #[serde(default)]
    pub manual_executor: String,
    #[serde(default)]
    pub manual_condition: String,
    #[serde(default)]
    pub steps: Vec<TestRunStep>,
    #[serde(default)]
    pub attachments: Vec<TestRunAttachment>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// The outcome of one case record of a batch, as it is written.
///
/// The conclusion is written once — the server refuses a second write
/// with `conclusion_is_final`. The fields around it stay writable while
/// the batch is open, so the evidence of a case may be added after the
/// conclusion was recorded.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestRunCaseWrite {
    pub conclusion: Option<String>,
    pub evidence: Option<String>,
    pub remarks: Option<String>,
    pub actual_result_summary: Option<String>,
    pub na_reason: Option<String>,
    pub na_criteria_source: Option<String>,
    pub manual_executor: Option<String>,
    pub manual_condition: Option<String>,
}

impl TestRunCaseWrite {
    /// True when the body carries no field at all — a write of nothing.
    pub fn is_empty(&self) -> bool {
        self.conclusion.is_none()
            && self.evidence.is_none()
            && self.remarks.is_none()
            && self.actual_result_summary.is_none()
            && self.na_reason.is_none()
            && self.na_criteria_source.is_none()
            && self.manual_executor.is_none()
            && self.manual_condition.is_none()
    }
}

/// One step record of a case record of a batch.
///
/// The action and the expected result are the **snapshot** of the step
/// of the test case: a later edit of the case never rewrites what the
/// batch recorded.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRunStep {
    pub id: String,
    /// The number of the step in the case — the endpoint of the write
    /// of a verdict.
    #[serde(default)]
    pub sequence_id: i64,
    #[serde(default)]
    pub action_snapshot: String,
    #[serde(default)]
    pub expected_result_snapshot: String,
    /// pass | fail | unset — empty while the step was not executed.
    #[serde(default)]
    pub conclusion: String,
    /// The measured value, as the Markdown source that was typed: no
    /// HTML conversion, no stripping of the markup.
    #[serde(default)]
    pub actual_value: String,
    #[serde(default)]
    pub attachments: Vec<TestRunAttachment>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// The verdict and the measured value of one step, as they are written.
///
/// The verdict is written once — the server refuses a second write with
/// `conclusion_is_final`. The measured value stays writable while the
/// batch is open.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestRunStepWrite {
    pub conclusion: Option<String>,
    pub actual_value: Option<String>,
}

impl TestRunStepWrite {
    /// True when the body carries no field at all — a write of nothing.
    pub fn is_empty(&self) -> bool {
        self.conclusion.is_none() && self.actual_value.is_none()
    }
}

/// One file attached to a case record or to a step record.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRunAttachment {
    /// The uuid of the file.
    #[serde(default)]
    pub id: String,
    /// The same value as the id, in the shape the clients key their own
    /// maps by.
    #[serde(default)]
    pub asset_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: Option<f64>,
    /// The path the file is served under.
    #[serde(default)]
    pub url: String,
}

/// One deviation of the summary of a batch: a case whose observed
/// result differs from the expected one.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRunDeviation {
    #[serde(default)]
    pub case_number: String,
    /// The one-line phenomenon of the deviation.
    #[serde(default)]
    pub summary: String,
}

/// The summary of a batch: the four counts and the deviation list.
///
/// The counts are **not** a partition of the total: `manual` counts the
/// cases that were never executed, and such a case may be counted as
/// `fail` as well (a case that failed on one step and was never
/// executed on another). `total` is the number of the case records.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRunSummary {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub pass: i64,
    #[serde(default)]
    pub fail: i64,
    #[serde(default)]
    pub na: i64,
    #[serde(default)]
    pub manual: i64,
    #[serde(default)]
    pub deviations: Vec<TestRunDeviation>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_batch_with_every_field() {
        let raw = r#"{
            "id": "r1",
            "number": "RUN-20261109-01",
            "run_date": "2026-11-09",
            "version_number": "1.2.3",
            "build_number": "42",
            "commit_hash": "abc1234",
            "environment": "bench",
            "executed_by": "u1",
            "executed_by_name": "Jicai Liu",
            "status": "in_progress",
            "retest_of": "r0",
            "retest_of_number": "RUN-20261108-02",
            "cases": [],
            "created_at": "2026-11-09T08:29:37Z",
            "updated_at": "2026-11-09T08:29:37Z",
            "created_by": {"id": "u1", "display_name": "Jicai.Liu"}
        }"#;
        let run: TestRun = serde_json::from_str(raw).unwrap();
        assert_eq!(run.number, "RUN-20261109-01");
        assert_eq!(run.run_date, "2026-11-09");
        assert_eq!(run.executed_by_name, "Jicai Liu");
        assert_eq!(run.retest_of_number.as_deref(), Some("RUN-20261108-02"));
        assert_eq!(run.created_by.unwrap().full_name(), "Jicai.Liu");
    }

    #[test]
    fn reads_a_batch_that_omits_the_optional_fields() {
        // The server omits a field it has not populated; a missing list
        // must read as an empty list, never as a parse failure.
        let run: TestRun = serde_json::from_str(r#"{"id": "r1"}"#).unwrap();
        assert!(run.cases.is_empty());
        assert_eq!(run.number, "");
        assert_eq!(run.retest_of, None);
        assert_eq!(run.retest_of_number, None);
    }

    #[test]
    fn reads_a_batch_whose_executor_the_server_answers_with_null() {
        // The shape of the live instance: a batch created without an
        // executor carries an explicit `null`, and `#[serde(default)]`
        // does not cover a `null` value — only a missing key.
        let raw = r#"{
            "id": "r1",
            "number": "RUN-20261110-08",
            "executed_by": null,
            "executed_by_name": "",
            "cases": [{"id": "rc1", "test_case": null, "case_number": "TC-PWR-001"}]
        }"#;
        let run: TestRun = serde_json::from_str(raw).expect("a null must not refuse the record");
        assert_eq!(run.executed_by, "");
        assert_eq!(run.executed_by_name, "");
        assert_eq!(run.cases[0].test_case, "");
        assert_eq!(run.cases[0].case_number, "TC-PWR-001");
    }

    #[test]
    fn reads_a_case_record_with_its_steps_and_files() {
        let raw = r#"{
            "id": "rc1",
            "test_case": "tc1",
            "case_number": "TC-CLI-007",
            "case_module_code": "CLI",
            "case_title": "Log in",
            "conclusion": "FAIL",
            "evidence": "see the screenshot",
            "steps": [{
                "id": "s1",
                "sequence_id": 3,
                "action_snapshot": "Log in",
                "expected_result_snapshot": "The dashboard is shown",
                "conclusion": "fail",
                "actual_value": "the password was refused",
                "attachments": [{"id": "a1", "asset_id": "a1", "name": "shot.png", "size": 12.0, "url": "http://x/a1"}]
            }],
            "attachments": []
        }"#;
        let case: TestRunCase = serde_json::from_str(raw).unwrap();
        assert_eq!(case.case_number, "TC-CLI-007");
        assert_eq!(case.conclusion, "FAIL");
        assert_eq!(case.steps.len(), 1);
        assert_eq!(case.steps[0].sequence_id, 3);
        assert_eq!(case.steps[0].conclusion, "fail");
        assert_eq!(case.steps[0].actual_value, "the password was refused");
        assert_eq!(case.steps[0].attachments[0].name, "shot.png");
    }

    #[test]
    fn a_write_carries_only_the_fields_that_are_set() {
        let write = TestRunCaseWrite {
            conclusion: Some("FAIL".into()),
            evidence: None,
            ..Default::default()
        };
        let value = serde_json::to_value(&write).unwrap();
        assert_eq!(value["conclusion"], "FAIL");
        // A field that was not set is null here; the client drops the
        // nulls before the body leaves, so the server reads it as
        // "leave this alone" (ADR-0007).
        assert!(value["evidence"].is_null());
        assert!(!write.is_empty());
        assert!(TestRunCaseWrite::default().is_empty());
    }

    #[test]
    fn a_step_write_carries_the_server_field_names() {
        let write = TestRunStepWrite {
            conclusion: Some("fail".into()),
            actual_value: Some("the password was refused".into()),
        };
        let value = serde_json::to_value(&write).unwrap();
        assert_eq!(value["conclusion"], "fail");
        assert_eq!(value["actual_value"], "the password was refused");
        assert!(!write.is_empty());
        assert!(TestRunStepWrite::default().is_empty());
    }

    #[test]
    fn a_batch_write_carries_the_server_field_names() {
        let write = TestRunWrite {
            run_date: Some("2026-11-09".into()),
            version_number: Some("1.2.3".into()),
            build_number: Some("42".into()),
            commit_hash: Some("abc1234".into()),
            environment: Some("bench".into()),
            executed_by: Some("u1".into()),
            status: None,
            retest_of: None,
        };
        let value = serde_json::to_value(&write).unwrap();
        assert_eq!(value["run_date"], "2026-11-09");
        assert_eq!(value["version_number"], "1.2.3");
        assert_eq!(value["build_number"], "42");
        assert_eq!(value["commit_hash"], "abc1234");
        assert_eq!(value["environment"], "bench");
        assert_eq!(value["executed_by"], "u1");
        // The number of a batch is never written: the server allocates
        // it from the day the batch carries.
        assert!(value.get("number").is_none());
        assert!(value.get("cases").is_none());
        assert!(!write.is_empty());
        assert!(TestRunWrite::default().is_empty());
    }

    #[test]
    fn reads_the_summary_with_its_counts_and_deviations() {
        let raw = r#"{"total":1,"pass":0,"fail":1,"na":0,"manual":1,
            "deviations":[{"case_number":"TC-CLI-007","summary":"the password was refused"}]}"#;
        let summary: TestRunSummary = serde_json::from_str(raw).unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.fail, 1);
        // The counts overlap: a case that failed and was never executed
        // on one of its steps is counted twice.
        assert_eq!(summary.manual, 1);
        assert_eq!(summary.deviations.len(), 1);
        assert_eq!(summary.deviations[0].case_number, "TC-CLI-007");
    }
}
