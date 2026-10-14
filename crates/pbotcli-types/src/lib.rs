//! Serde data types shared across the pbotcli crates.
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
    /// The human a service principal belongs to, if the payload names one.
    ///
    /// The v1 `/users/me/` payload does **not** carry this today: the server
    /// deliberately never serializes the owner's profile. It is read
    /// forward-compatibly so `whoami` prints `name (SP owned by <owner>)` the
    /// day the server adds it. Polymorphic like the work-item fields — a name
    /// string (also accepted as `owner_name`) or an owner object — and omitted
    /// from `--json` when absent, so a human payload serializes as before.
    #[serde(default, alias = "owner_name", skip_serializing_if = "Option::is_none")]
    pub owner: Option<Value>,
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

    /// The identity as `whoami` shows it and as error messages name it.
    ///
    /// A person renders as their name, unchanged. A service principal carries an
    /// explicit marker, because the bare name looks like a credential that
    /// failed to load: `name (SP owned by <owner>)` when the payload supplies an
    /// owner name, `name (service principal)` when it does not.
    pub fn identity_label(&self) -> String {
        if !self.is_service_principal() {
            return self.display_label();
        }
        let name = self.display_label();
        match self.owner_label() {
            Some(owner) => format!("{name} (SP owned by {owner})"),
            None => format!("{name} (service principal)"),
        }
    }

    /// The owner's name, when the payload names one.
    ///
    /// Accepts an owner object (`display_name`, then `name`, then
    /// `first_name last_name`) or a plain string. An id and an address are both
    /// rejected: a UUID-shaped string is an *id*, not a name, and an
    /// `@`-shaped string is an email, which `whoami` does not print. Either way
    /// this returns `None` and `identity_label` degrades to the plain marker
    /// rather than claiming "owned by 17af336a-…" or "owned by j@x", and the raw
    /// payload stays available in `--json`.
    pub fn owner_label(&self) -> Option<String> {
        let owner = self.owner.as_ref()?;
        if let Some(raw) = owner.as_str() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || is_uuid(trimmed) || is_email(trimmed) {
                return None;
            }
            return Some(trimmed.to_string());
        }
        let obj = owner.as_object()?;
        let keyed = ["display_name", "name"].iter().find_map(|key| {
            obj.get(*key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty() && !is_uuid(s) && !is_email(s))
        });
        if let Some(name) = keyed {
            return Some(name.to_string());
        }
        // A pair names the human only if neither half is an id or an address:
        // half a name invented from a payload that mixes an address into a name
        // field would be worse than the plain marker.
        let parts: Vec<&str> = ["first_name", "last_name"]
            .iter()
            .filter_map(|key| obj.get(*key).and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() || parts.iter().any(|s| is_uuid(s) || is_email(s)) {
            return None;
        }
        Some(parts.join(" "))
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

/// The canonical `8-4-4-4-12` hex UUID shape check, shared by the query parser
/// in `pbotcli-resolve` (which re-exports it) and by the identity checks here.
pub fn is_uuid(s: &str) -> bool {
    const LENGTHS: [usize; 5] = [8, 4, 4, 4, 12];
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == LENGTHS.len()
        && parts
            .iter()
            .zip(LENGTHS)
            .all(|(part, len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Whether a string is an email address rather than a name: an `@` with
/// something either side of it. Deliberately coarse — the check only has to
/// keep an address out of a displayed name, not validate one.
fn is_email(s: &str) -> bool {
    match s.split_once('@') {
        Some((local, domain)) => !local.is_empty() && !domain.is_empty(),
        None => false,
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

    /// True when this member row is the shadow bot user of the service
    /// principal named `sp_name`.
    ///
    /// Plane credits every service-principal write to a *shadow bot* `User`
    /// (`apps/api/plane/service_principals/attribution.py`): `bot_display_name`
    /// is `svc-<name> (bot)`, `bot_first_name` is `svc-<name>`, `last_name` is
    /// `Bot`. That user's id — not the `ServicePrincipal.id` `whoami` prints —
    /// is the one an assignment has to carry, which is why `--assignee me`
    /// under an SP used to match nothing.
    ///
    /// A row matches only when a name field both carries the bot composition
    /// (`svc-` prefix or `(bot)` marker) *and* normalises to the SP's name, so
    /// the server's rule is matched tolerantly (a deployment that sends the
    /// bare SP name in `first_name` still matches through `display_name`)
    /// without a bare name being enough. A human member who happens to share
    /// the principal's name must not be resolved as its bot: the miss is loud
    /// (the caller reports "no shadow bot member" plus the command that lists
    /// members) while a wrong match would silently write to the wrong person.
    pub fn matches_service_principal(&self, sp_name: &str) -> bool {
        // The principal's own name is compared as it is (trimmed, lowercased):
        // only the member's name carries a composition to strip, and stripping
        // it off the principal too would lose a principal whose name itself
        // starts with `svc-`.
        let target = sp_name.trim().to_lowercase();
        if target.is_empty() {
            return false;
        }
        [self.display_name.as_deref(), self.first_name.as_deref()]
            .into_iter()
            .flatten()
            .any(|candidate| has_bot_marker(candidate) && normalize_bot_name(candidate) == target)
    }
}

/// Lowercase, trim, and strip the bot composition (`svc-` prefix, `(bot)`
/// suffix) so the server's composed names and a bare SP name compare equal.
///
/// Only the parenthesised marker is stripped: a bare `bot` suffix would eat
/// into the SP's own name (`kimicode-bot` → `kimicode`), which is exactly the
/// name this has to match.
fn normalize_bot_name(s: &str) -> String {
    let mut out = s.trim().to_lowercase();
    if let Some(rest) = out.strip_prefix("svc-") {
        out = rest.trim().to_string();
    }
    if let Some(rest) = out.strip_suffix("(bot)") {
        out = rest.trim_end_matches([' ', '.', '-']).trim().to_string();
    }
    out
}

/// Whether a raw name carries the bot composition the server stamps onto a
/// shadow bot user. Only a *match* is strengthened by this; it never decides
/// one on its own.
fn has_bot_marker(s: &str) -> bool {
    let lower = s.trim().to_lowercase();
    lower.starts_with("svc-") || lower.contains("(bot)")
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

    #[test]
    fn service_principal_identity_label_marks_the_kind() {
        // No owner in the payload (what the server sends today) → explicit marker.
        let sp = user(r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal"}"#);
        assert_eq!(sp.identity_label(), "kimicode-bot (service principal)");
    }

    #[test]
    fn service_principal_identity_label_names_the_owner_when_the_payload_has_one() {
        // Forward-compatible: an owner object naming the human.
        let sp = user(
            r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal",
                "owner":{"id":"u1","display_name":"Jicai Liu","first_name":"Jicai"}}"#,
        );
        assert_eq!(sp.identity_label(), "kimicode-bot (SP owned by Jicai Liu)");
        // ... and the `owner_name` string alias.
        let sp = user(
            r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner_name":"Jicai Liu"}"#,
        );
        assert_eq!(sp.identity_label(), "kimicode-bot (SP owned by Jicai Liu)");
        // An owner object with only a first/last pair still names the human.
        let sp = user(
            r#"{"id":"sp1","name":"b","type":"service_principal","owner":{"first_name":"Jicai","last_name":"Liu"}}"#,
        );
        assert_eq!(sp.identity_label(), "b (SP owned by Jicai Liu)");
    }

    #[test]
    fn owner_that_is_only_a_uuid_is_not_a_name() {
        // The SP-list endpoint answers with the owner's UUID. A UUID is not a
        // name, so the label degrades instead of claiming "owned by 17af…".
        let uuid = "17af336a-126d-4f0f-a067-25e7d90a2d78";
        let sp = user(&format!(
            r#"{{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":"{uuid}"}}"#
        ));
        assert_eq!(sp.owner_label(), None);
        assert_eq!(sp.identity_label(), "kimicode-bot (service principal)");
        let sp = user(&format!(
            r#"{{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":{{"id":"{uuid}"}}}}"#
        ));
        assert_eq!(sp.identity_label(), "kimicode-bot (service principal)");
    }

    #[test]
    fn owner_email_is_not_taken_as_a_name() {
        // The owner's address is not a name, and `whoami` is not the place to
        // print it: the raw payload stays available in `--json`.
        let sp = user(
            r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":{"id":"u1","email":"j@x"}}"#,
        );
        assert_eq!(sp.owner_label(), None);
        assert_eq!(sp.identity_label(), "kimicode-bot (service principal)");
        // ... in the object's name fields too.
        for owner in [
            r#"{"name":"j@x"}"#,
            r#"{"display_name":"j@x"}"#,
            r#"{"first_name":"j@x","last_name":"y@x"}"#,
        ] {
            let sp = user(&format!(
                r#"{{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":{owner}}}"#
            ));
            assert_eq!(sp.owner_label(), None, "owner {owner} must not name anyone");
            assert_eq!(sp.identity_label(), "kimicode-bot (service principal)");
        }
        // An address among the name parts invalidates the pair rather than
        // contributing half a name: the marker degrades, nothing is invented.
        let sp = user(
            r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":{"first_name":"j@x","last_name":"Liu"}}"#,
        );
        assert_eq!(sp.owner_label(), None);
        // ... and in the string form, `owner` and its `owner_name` alias alike.
        for key in ["owner", "owner_name"] {
            let sp = user(&format!(
                r#"{{"id":"sp1","name":"kimicode-bot","type":"service_principal","{key}":"j@x"}}"#
            ));
            assert_eq!(sp.owner_label(), None, "{key} must not name anyone");
            assert_eq!(sp.identity_label(), "kimicode-bot (service principal)");
        }
        // Any `@`-bearing string is treated as an address and rejected: the
        // degradation is safe (the plain marker), and a name is never invented
        // from something that looks like an address.
        let sp = user(
            r#"{"id":"sp1","name":"kimicode-bot","type":"service_principal","owner":"Jicai @ Islet"}"#,
        );
        assert_eq!(sp.owner_label(), None);
    }

    #[test]
    fn human_identity_label_and_json_are_unchanged() {
        let raw = r#"{"id":"u1","display_name":"Jicai Liu","first_name":"Jicai","last_name":"Liu","email":"j@x"}"#;
        let me = user(raw);
        assert_eq!(me.identity_label(), me.display_label());
        assert_eq!(me.identity_label(), "Jicai Liu");
        // The added field is skipped when absent, so `whoami --json` for a
        // human credential keeps its exact shape.
        let value = serde_json::to_value(&me).expect("serializes");
        assert!(value.get("owner").is_none(), "{value}");
        assert_eq!(value["display_name"], "Jicai Liu");
    }

    fn member(json: &str) -> Member {
        serde_json::from_str(json).expect("valid member json")
    }

    #[test]
    fn shadow_bot_matching_follows_the_server_composition() {
        // server composition: display_name = "svc-<name> (bot)", first_name = "svc-<name>"
        let bot = member(
            r#"{"id":"e25ef580","display_name":"svc-deploy-bot (bot)","first_name":"svc-deploy-bot","last_name":"Bot"}"#,
        );
        assert!(bot.matches_service_principal("deploy-bot"));
        // The live shape: the display name carries the composition while
        // `first_name` is the bare SP name.
        let live = member(
            r#"{"id":"e25ef580","display_name":"svc-Kimicode IsletspaceWorkstation (bot)","first_name":"Kimicode IsletspaceWorkstation"}"#,
        );
        assert!(live.matches_service_principal("Kimicode IsletspaceWorkstation"));
        // The composition may also be carried by `first_name` alone.
        let by_first_name =
            member(r#"{"id":"e2","display_name":"Deploy bot","first_name":"svc-deploy-bot"}"#);
        assert!(by_first_name.matches_service_principal("deploy-bot"));
        // Unrelated members never match.
        let human = member(r#"{"id":"17af336a","display_name":"Jicai.Liu","first_name":"Jicai"}"#);
        assert!(!human.matches_service_principal("deploy-bot"));
    }

    #[test]
    fn a_bare_name_equal_to_the_principal_is_not_a_shadow_bot_match() {
        // A human member who happens to share the SP's name must not be
        // resolved as its bot: a wrong match would silently assign work to the
        // wrong person, while a miss reports "no shadow bot member" loudly.
        let human = member(r#"{"id":"u2","display_name":"Deploy Bot","first_name":"deploy-bot"}"#);
        assert!(!human.matches_service_principal("deploy-bot"));
    }

    #[test]
    fn shadow_bot_matching_keeps_a_name_that_ends_in_bot() {
        // Stripping a bare trailing "bot" would have turned the target into
        // "kimicode" and matched nothing.
        let bot = member(
            r#"{"id":"e1","display_name":"svc-kimicode-bot (bot)","first_name":"svc-kimicode-bot"}"#,
        );
        assert!(bot.matches_service_principal("kimicode-bot"));
    }

    #[test]
    fn shadow_bot_matching_rejects_an_empty_name() {
        // A blank SP name must not match every member whose name normalises to "".
        let blank = member(r#"{"id":"u3","display_name":"Bot","first_name":""}"#);
        assert!(!blank.matches_service_principal(""));
        assert!(!blank.matches_service_principal("   "));
    }

    #[test]
    fn shadow_bot_matching_keeps_a_principal_name_that_starts_with_svc() {
        // Only the member's name carries a composition to strip: stripping it
        // off the principal too would turn "svc-thing" into "thing" and match
        // nothing.
        let bot = member(
            r#"{"id":"e1","display_name":"svc-svc-thing (bot)","first_name":"svc-svc-thing"}"#,
        );
        assert!(bot.matches_service_principal("svc-thing"));
    }
}
