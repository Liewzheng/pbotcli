//! Serde data types shared across the planebotcli crates.
//!
//! DTOs mirror the Plane v1 API responses. Some fields are polymorphic across
//! endpoints (work-item `state`/`labels`/`assignees` come back as raw UUID
//! strings in the detail endpoint and as UUID lists in list endpoints) — those
//! are kept as `serde_json::Value` and interpreted by the render layer, the
//! same way the Python CLI's `_enrich_work_item` resolves names via cached
//! state/label/member maps.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A workspace member / user, as returned by `/api/v1/users/me/`.
///
/// A service-principal credential has no person fields: the server answers with
/// `name` and `type: "service_principal"` instead. Both are kept, otherwise the
/// identity is dropped and `whoami` renders a block of blanks.
///
/// An empty string counts as absent throughout: the server sends `null` (or
/// omits the field entirely) for what it has not populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: Option<String>,
    /// The server's `type`, renamed because `type` is a Rust keyword. It is
    /// serialized and displayed as `type`, so `--json` consumers and the table
    /// see the API's own name.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub display_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
}

impl User {
    /// The name to show for this identity.
    ///
    /// A service principal has no person fields at all and is named by `name`;
    /// a person is named by `display_name`, then `first_name last_name`. A
    /// payload that only carries `name` (an unlabelled bot) still gets a label
    /// from the fallback.
    pub fn display_label(&self) -> String {
        let name = self.name.as_deref().filter(|s| !s.is_empty());
        // `Option<&str>` is Copy, so the filtered view does not consume `name`,
        // which the final fallback below still needs.
        if let Some(sp_name) = name.filter(|_| self.is_service_principal()) {
            return sp_name.to_string();
        }
        if let Some(label) = self.display_name.as_deref().filter(|s| !s.is_empty()) {
            return label.to_string();
        }
        let joined = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if joined.is_empty() {
            name.unwrap_or_default().to_string()
        } else {
            joined
        }
    }

    /// True when the credential is a service principal rather than a person.
    pub fn is_service_principal(&self) -> bool {
        self.kind.as_deref() == Some("service_principal")
    }

    /// True when the server sent at least one person field.
    pub fn has_person_fields(&self) -> bool {
        [
            self.display_name.as_deref(),
            self.first_name.as_deref(),
            self.last_name.as_deref(),
            self.email.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|s| !s.is_empty())
    }
}

/// A member row of `/api/v1/workspaces/{ws}/members/`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Member {
    pub id: String,
    #[serde(rename = "display_name")]
    pub display_name: Option<String>,
    #[serde(rename = "first_name")]
    pub first_name: Option<String>,
    #[serde(rename = "last_name")]
    pub last_name: Option<String>,
    pub email: Option<String>,
}

impl Member {
    /// Best-effort full name, matching the Python member_map builder.
    pub fn full_name(&self) -> String {
        let joined = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            joined
        } else {
            self.display_name.clone().unwrap_or_default()
        }
    }
}

/// A project as returned by the projects endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Project {
    pub id: String,
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub description: Option<String>,
    /// Whether the project's intake queue is enabled (the project's
    /// `intake_view` flag; absent in some serializers, so `None` means off).
    #[serde(default, rename = "intake_view")]
    pub intake_view: Option<bool>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}

/// Request body for project create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProjectWrite {
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub description: Option<String>,
}

/// A label as returned by the labels endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Label {
    pub id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub parent: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
}

/// Request body for label create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LabelWrite {
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// A state as returned by the states endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub id: String,
    pub name: Option<String>,
    pub group: Option<String>,
    pub color: Option<String>,
    /// Numeric sequence; the API returns integers or floats depending on the
    /// project sort config.
    pub sequence: Option<serde_json::Value>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
}

/// Request body for state create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct StateWrite {
    pub name: Option<String>,
    pub color: Option<String>,
    pub group: Option<String>,
    pub description: Option<String>,
}

/// A module as returned by the modules endpoints.
///
/// The API exposes the end date under `target_date` (not `end_date`), which is
/// the field name the write DTO and the CLI both use.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Module {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    /// backlog | planned | in-progress | paused | completed | cancelled
    pub status: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}

/// Request body for module create/update.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ModuleWrite {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
}

/// A cycle as returned by the cycles endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cycle {
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "end_date")]
    pub end_date: Option<String>,
    #[serde(rename = "owned_by")]
    pub owned_by: Option<String>,
    #[serde(rename = "total_issues")]
    pub total_issues: Option<i64>,
    #[serde(rename = "completed_issues")]
    pub completed_issues: Option<i64>,
    #[serde(rename = "started_issues")]
    pub started_issues: Option<i64>,
    #[serde(rename = "unstarted_issues")]
    pub unstarted_issues: Option<i64>,
    #[serde(rename = "backlog_issues")]
    pub backlog_issues: Option<i64>,
    #[serde(rename = "cancelled_issues")]
    pub cancelled_issues: Option<i64>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}

/// Request body for cycle create/update.
///
/// `owned_by` and `project_id` are required on create (the API fills in the
/// owner from the authenticated user when omitted from PATCH bodies).
#[derive(Debug, Clone, Serialize, Default)]
pub struct CycleWrite {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "end_date")]
    pub end_date: Option<String>,
    #[serde(rename = "owned_by")]
    pub owned_by: Option<String>,
    #[serde(rename = "project_id")]
    pub project_id: Option<String>,
}

/// Project summary embedded in cross-project work-item listings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectDetail {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub name: Option<String>,
}

/// A work item as returned by the list/detail endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkItem {
    pub id: String,
    pub name: Option<String>,
    pub sequence_id: Option<Value>,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    pub priority: Option<String>,
    /// UUID string, or an object when expanded.
    #[serde(default)]
    pub state: Option<Value>,
    /// List of UUIDs, or objects when expanded.
    #[serde(default)]
    pub labels: Option<Value>,
    /// List of UUIDs, or objects when expanded.
    #[serde(default)]
    pub assignees: Option<Value>,
    pub parent: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
    pub project: Option<String>,
    /// Flat project identifier from the search endpoint (`project__identifier`,
    /// double underscore) — list/detail responses nest it in `project_detail`
    /// instead, so this stays None there.
    #[serde(rename = "project__identifier")]
    pub project_identifier: Option<String>,
    #[serde(rename = "project_detail")]
    pub project_detail: Option<ProjectDetail>,
    #[serde(rename = "estimate_point")]
    pub estimate_point: Option<Value>,
    pub point: Option<Value>,
    #[serde(rename = "is_draft")]
    pub is_draft: Option<bool>,
}

/// Request body for work-item create.
#[derive(Debug, Clone, Serialize, Default)]
pub struct WorkItemWrite {
    pub name: Option<String>,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    pub priority: Option<String>,
    pub state: Option<String>,
    pub assignees: Option<Vec<String>>,
    pub labels: Option<Vec<String>>,
    pub parent: Option<String>,
    #[serde(rename = "start_date")]
    pub start_date: Option<String>,
    #[serde(rename = "target_date")]
    pub target_date: Option<String>,
    #[serde(rename = "estimate_point")]
    pub estimate_point: Option<String>,
}

/// A comment as returned by the comments endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Comment {
    pub id: String,
    #[serde(rename = "comment_html")]
    pub comment_html: Option<String>,
    #[serde(rename = "comment_stripped")]
    pub comment_stripped: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
    pub actor: Option<String>,
    #[serde(rename = "actor_detail")]
    pub actor_detail: Option<Value>,
    #[serde(rename = "created_by")]
    pub created_by: Option<String>,
    pub parent: Option<String>,
}

/// Request body for comment create/update.
#[derive(Debug, Clone, Serialize)]
pub struct CommentWrite {
    #[serde(rename = "comment_html")]
    pub comment_html: String,
}

/// An intake queue row (the `IntakeIssue` record), as returned by the
/// `/intake-issues/` endpoints.
///
/// The wrapper has its own `id`; the underlying work item's UUID is the
/// `issue` field (shown as "Issue ID" by the CLI and used as the target of
/// accept/decline/delete — never the wrapper `id`). `issue_detail` carries an
/// expanded copy of the work item (name, priority, state, ...) that the API
/// includes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntakeItem {
    /// Intake wrapper id — NOT the work-item uuid.
    pub id: String,
    /// Work-item UUID — the triage target / "Issue ID" column.
    #[serde(default, alias = "issue_id")]
    pub issue: Option<String>,
    /// Expanded work item (id/name/priority/state/...), when the API expands it.
    #[serde(default, rename = "issue_detail")]
    pub issue_detail: Option<Value>,
    /// Intake status code: -2 pending, -1 rejected, 0 snoozed, 1 accepted,
    /// 2 duplicate.
    #[serde(default)]
    pub status: Option<i64>,
    #[serde(default, rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(default, rename = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
}

/// Request body for `intake create`: the work item data nested under `issue`.
#[derive(Debug, Clone, Serialize, Default)]
pub struct IntakeWrite {
    #[serde(rename = "issue")]
    pub issue: IntakeIssueWrite,
}

/// The embedded work item of an intake create request.
#[derive(Debug, Clone, Serialize, Default)]
pub struct IntakeIssueWrite {
    pub name: String,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    pub priority: Option<String>,
}

/// A page (document) as returned by the pages endpoints.
///
/// Pages live in two scopes: workspace pages and per-project pages. Workspace
/// pages carry `project: null`; both scopes expose `created_by` and, once
/// archived, `archived_at` (the API refuses to DELETE a page that has not been
/// archived — see ADR-0007).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Page {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
    #[serde(rename = "description_stripped")]
    pub description_stripped: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
    #[serde(rename = "created_by")]
    pub created_by: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(rename = "archived_at")]
    pub archived_at: Option<String>,
}

/// Request body for page create/update. `description_html` is required on
/// create (send an empty `<p></p>` when there is no content); `None` fields
/// are dropped from the serialized body so a PATCH only carries what changed.
#[derive(Debug, Clone, Serialize, Default)]
pub struct PageWrite {
    pub name: Option<String>,
    #[serde(rename = "description_html")]
    pub description_html: Option<String>,
}

/// A work-item attachment as returned by the attachment endpoints.
///
/// The original filename/MIME live in the nested `attributes` object, and the
/// byte count may appear at the top level or only under `attributes`;
/// [`Attachment::name`]/[`Attachment::content_type`]/[`Attachment::size_bytes`]
/// flatten both shapes, mirroring the Python `_enrich_attachment`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Attachment {
    pub id: String,
    /// Underlying asset UUID; equals `id` on work-item-scoped records.
    #[serde(default, rename = "asset_id")]
    pub asset_id: Option<String>,
    /// Nested `{name, type, size}` metadata captured at register time.
    #[serde(default)]
    pub attributes: Option<Value>,
    /// Top-level byte count (some serializers only expose it in `attributes`).
    #[serde(default)]
    pub size: Option<Value>,
    #[serde(default, rename = "is_uploaded")]
    pub is_uploaded: Option<bool>,
    #[serde(default, rename = "is_deleted")]
    pub is_deleted: Option<bool>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "updated_at")]
    pub updated_at: Option<String>,
}

impl Attachment {
    fn attribute(&self, key: &str) -> String {
        self.attributes
            .as_ref()
            .and_then(|a| a.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// Original filename, stored under the nested `attributes` object.
    pub fn name(&self) -> String {
        self.attribute("name")
    }

    /// MIME type captured at register time (the nested `type` field).
    pub fn content_type(&self) -> String {
        self.attribute("type")
    }

    /// Byte count: the top-level `size` when present, else `attributes.size`.
    pub fn size_bytes(&self) -> Option<Value> {
        self.size
            .as_ref()
            .filter(|v| !v.is_null())
            .or_else(|| {
                self.attributes
                    .as_ref()
                    .and_then(|a| a.get("size"))
                    .filter(|v| !v.is_null())
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(json: &str) -> User {
        serde_json::from_str(json).expect("valid user json")
    }

    #[test]
    fn service_principal_identity_is_kept() {
        let me = user(r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal"}"#);
        assert_eq!(me.display_label(), "kimicode-bot");
        assert_eq!(me.kind.as_deref(), Some("service_principal"));
        assert!(!me.has_person_fields());
    }

    #[test]
    fn display_label_prefers_the_person_fields_over_nothing() {
        let me = user(
            r#"{"id":"u1","display_name":"Jicai Liu","first_name":"Jicai","last_name":"Liu","email":"j@x"}"#,
        );
        assert_eq!(me.display_label(), "Jicai Liu");
        assert!(me.has_person_fields());
    }

    #[test]
    fn display_label_falls_back_to_first_and_last() {
        let me = user(r#"{"id":"u2","first_name":"Jicai","last_name":"Liu"}"#);
        assert_eq!(me.display_label(), "Jicai Liu");
        let nameless = user(r#"{"id":"u3","first_name":"","last_name":""}"#);
        assert_eq!(nameless.display_label(), "");
    }

    #[test]
    fn display_label_ignores_name_for_a_person() {
        // A person whose payload also carries `name` is still labelled by
        // `display_name`: only a service principal is named by `name`.
        let me = user(
            r#"{"id":"u7","name":"jicai-bot","display_name":"Jicai Liu","first_name":"Jicai","last_name":"Liu","email":"j@x"}"#,
        );
        assert_eq!(me.display_label(), "Jicai Liu");
        let sp = user(
            r#"{"id":"sp2","name":"bot","type":"service_principal","display_name":"ignored"}"#,
        );
        assert!(sp.is_service_principal());
        assert_eq!(sp.display_label(), "bot");
    }

    #[test]
    fn display_label_handles_a_one_sided_or_untyped_name() {
        assert_eq!(
            user(r#"{"id":"u4","first_name":"Jicai"}"#).display_label(),
            "Jicai"
        );
        assert_eq!(
            user(r#"{"id":"u5","last_name":"Liu"}"#).display_label(),
            "Liu"
        );
        // Only `name` and no `type`: the fallback still labels the identity.
        assert_eq!(
            user(r#"{"id":"u6","name":"lonely-bot"}"#).display_label(),
            "lonely-bot"
        );
    }
}
