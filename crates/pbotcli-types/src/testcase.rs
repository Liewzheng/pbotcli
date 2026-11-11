//! DTOs for the test-case feature of the Plane v1 API.
//!
//! The field names are the contract: the v1 surface reuses the app
//! serializers verbatim, so the shapes below are the shapes the server
//! hands out. Every read field is optional-tolerant (`#[serde(default)]`)
//! because the server omits a field it has not populated (the recommended
//! `related_feature` / `remarks` arrive as empty strings, an unlinked case
//! has an empty `issue_links`).

use serde::{Deserialize, Serialize};

use crate::Member;

/// A registered module code of a project.
///
/// The module code is what a case number is built from: the server assigns
/// `TC-<code>-<NNN>`, and the code is unique inside its project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestCaseModule {
    pub id: String,
    /// 2-6 uppercase letters. The server uppercases the code on write.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Request body for the module registry.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestCaseModuleWrite {
    pub code: Option<String>,
    pub description: Option<String>,
}

/// One row of the step table of a case.
///
/// The step numbers are assigned by the server (`sequence_id` counts from 1),
/// so a client writes the action and the expected result and lets the server
/// number the rows.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestCaseStep {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub sequence_id: i64,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub expected_result: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// One step of a case as it is written.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestCaseStepWrite {
    pub action: String,
    pub expected_result: String,
}

/// The body of the whole-table replacement of a step table.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestCaseStepsWrite {
    pub steps: Vec<TestCaseStepWrite>,
}

/// The answer of the step-table replacement: the new table and the
/// `updated_at` of the case it belongs to (the client keeps the timestamp
/// for the next write, otherwise it locks itself out with a stale version).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestCaseStepsResult {
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub steps: Vec<TestCaseStep>,
}

/// A link between a case and a work item.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestCaseIssueLink {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub issue_id: String,
    /// `<PROJECT>-<NNN>` of the linked work item; absent when the link points
    /// at no work item.
    #[serde(default)]
    pub issue_identifier: Option<String>,
    #[serde(default)]
    pub issue_name: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

/// Request body for linking a work item to a case.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestCaseIssueWrite {
    pub issue_id: String,
}

/// A test case as it is read.
///
/// The number (`TC-<module code>-<NNN>`) is assigned by the server and is
/// read-only: the client never writes it and never generates one.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestCase {
    pub id: String,
    #[serde(default)]
    pub number: String,
    /// The uuid of the module the case belongs to.
    #[serde(default)]
    pub module: String,
    /// The code of that module — the prefix of the case number.
    #[serde(default)]
    pub module_code: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub object_overview: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub preconditions: String,
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub related_feature: String,
    #[serde(default)]
    pub remarks: String,
    /// active | void
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub steps: Vec<TestCaseStep>,
    #[serde(default)]
    pub issue_links: Vec<TestCaseIssueLink>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// The creator, as the server hands it out: the same user payload the
    /// member list carries, so the name of a creator is read the same way as
    /// the name of an assignee.
    #[serde(default)]
    pub created_by: Option<Member>,
}

/// A test case as it is written.
///
/// Every field is optional: the client strips the ones it does not set, so
/// an unset field means "leave this alone" (ADR-0007). The server refuses
/// a case without a module, a title, a purpose, an object overview, the
/// preconditions, the environment, a valid priority and at least one step
/// — so the CLI asks for all of them.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TestCaseWrite {
    pub module: Option<String>,
    pub title: Option<String>,
    pub purpose: Option<String>,
    pub object_overview: Option<String>,
    pub priority: Option<String>,
    pub preconditions: Option<String>,
    pub environment: Option<String>,
    pub related_feature: Option<String>,
    pub remarks: Option<String>,
    pub status: Option<String>,
    /// The whole step table — a write replaces it as a whole.
    pub steps: Option<Vec<TestCaseStepWrite>>,
}

impl TestCaseWrite {
    /// True when the body carries no field at all — a write of nothing.
    pub fn is_empty(&self) -> bool {
        self.module.is_none()
            && self.title.is_none()
            && self.purpose.is_none()
            && self.object_overview.is_none()
            && self.priority.is_none()
            && self.preconditions.is_none()
            && self.environment.is_none()
            && self.related_feature.is_none()
            && self.remarks.is_none()
            && self.status.is_none()
            && self.steps.is_none()
    }
}

/// The filters the list endpoint of the cases understands.
#[derive(Debug, Clone, Default)]
pub struct TestCaseFilters {
    /// A module uuid or a module code.
    pub module: Option<String>,
    /// P0 | P1 | P2 | P3
    pub priority: Option<String>,
    /// The uuid of a linked work item.
    pub issue: Option<String>,
    /// active (the server default) | all
    pub status: Option<String>,
    /// Matches the number and the title of a case.
    pub search: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_case_with_every_field() {
        let raw = r#"{
            "id": "tc1",
            "number": "TC-PWR-001",
            "module": "m1",
            "module_code": "PWR",
            "title": "Wake source",
            "purpose": "Check the constraint",
            "object_overview": "The config page",
            "priority": "P1",
            "preconditions": "Device flashed",
            "environment": "RV1106 EVB",
            "related_feature": "",
            "remarks": "",
            "status": "active",
            "steps": [{"id": "s1", "sequence_id": 1, "action": "Open the page", "expected_result": "Only GPIO0"}],
            "issue_links": [{"id": "l1", "issue_id": "wi1", "issue_identifier": "DEMO-1", "issue_name": "A task"}],
            "created_at": "2026-11-10T08:29:37Z",
            "updated_at": "2026-11-10T08:29:37Z",
            "created_by": {"id": "u1", "display_name": "Jicai.Liu", "first_name": "Jicai", "last_name": "Liu"}
        }"#;
        let case: TestCase = serde_json::from_str(raw).unwrap();
        assert_eq!(case.number, "TC-PWR-001");
        assert_eq!(case.module_code, "PWR");
        assert_eq!(case.steps.len(), 1);
        assert_eq!(case.steps[0].sequence_id, 1);
        assert_eq!(
            case.issue_links[0].issue_identifier.as_deref(),
            Some("DEMO-1")
        );
        assert_eq!(case.created_by.unwrap().full_name(), "Jicai Liu");
    }

    #[test]
    fn reads_a_case_that_omits_the_optional_fields() {
        // The server omits a field it has not populated; a missing list must
        // read as an empty list, never as a parse failure.
        let case: TestCase = serde_json::from_str(r#"{"id": "tc1"}"#).unwrap();
        assert!(case.steps.is_empty());
        assert!(case.issue_links.is_empty());
        assert_eq!(case.number, "");
    }

    #[test]
    fn writes_only_the_fields_that_are_set() {
        let write = TestCaseWrite {
            title: Some("A title".into()),
            module: Some("m1".into()),
            ..Default::default()
        };
        let value = serde_json::to_value(&write).unwrap();
        assert_eq!(value["title"], "A title");
        assert_eq!(value["module"], "m1");
        // A field that was not set is null here; the client drops the nulls
        // before the body leaves, so the server reads it as "leave this
        // alone" (ADR-0007).
        assert!(value["purpose"].is_null());
    }

    #[test]
    fn a_write_without_a_field_is_empty() {
        assert!(TestCaseWrite::default().is_empty());
        assert!(
            !TestCaseWrite {
                title: Some("A title".into()),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !TestCaseWrite {
                steps: Some(Vec::new()),
                ..Default::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn the_step_write_carries_the_server_field_names() {
        let write = TestCaseStepWrite {
            action: "Open the page".into(),
            expected_result: "The list is shown".into(),
        };
        let value = serde_json::to_value(&write).unwrap();
        assert_eq!(value["action"], "Open the page");
        assert_eq!(value["expected_result"], "The list is shown");
    }

    #[test]
    fn the_creator_is_read_as_a_member() {
        // The creator is the same payload the member list carries, so its
        // name is read the same way as the name of an assignee.
        let case: TestCase = serde_json::from_str(
            r#"{"id":"tc1","created_by":{"id":"u1","display_name":"Jicai.Liu","first_name":"Jicai","last_name":"Liu"}}"#,
        )
        .unwrap();
        assert_eq!(case.created_by.unwrap().full_name(), "Jicai Liu");
    }
}
