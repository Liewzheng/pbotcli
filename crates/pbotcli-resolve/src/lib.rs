//! Fuzzy resolution of resource references: UUID, identifier, or fuzzy name.
//!
//! Mirrors the removed Python line's `utils/fuzzy.py` (rapidfuzz
//! token_sort_ratio, threshold 60) and `utils/resolve.py`.

use pbotcli_client::PlaneClient;
use pbotcli_core::PlaneError;
use pbotcli_types::Project;

/// Minimum similarity (0-100) for a fuzzy match to be accepted.
pub const MATCH_THRESHOLD: f64 = 60.0;

/// token_sort_ratio similarity in [0, 100].
///
/// rapidfuzz (Rust) dropped `token_sort_ratio` after 0.4; it is reimplemented
/// here on top of `fuzz::ratio`, which is exactly what the algorithm does:
/// lowercase, sort the whitespace-separated tokens of each side, join, ratio.
pub fn token_sort_ratio(a: &str, b: &str) -> f64 {
    let sort_tokens = |s: &str| -> String {
        let lower = s.to_lowercase();
        let mut tokens: Vec<&str> = lower.split_whitespace().collect();
        tokens.sort_unstable();
        tokens.join(" ")
    };
    let sa = sort_tokens(a);
    let sb = sort_tokens(b);
    // rapidfuzz (Rust) scores on a 0-1 scale; the CLI contract uses 0-100.
    rapidfuzz::fuzz::ratio(sa.chars(), sb.chars()) * 100.0
}

pub struct BestMatch<T> {
    pub item: T,
    pub score: f64,
}

/// Return the item (and score) closest to `query` with score >= threshold.
/// A blank query never matches — an empty string would otherwise score 100
/// against a resource with an empty name.
pub fn find_best_match<'a, T, F>(query: &str, items: &'a [T], key: F) -> Option<BestMatch<&'a T>>
where
    F: Fn(&T) -> &str,
{
    if query.trim().is_empty() {
        return None;
    }
    let mut best: Option<(f64, &'a T)> = None;
    for item in items {
        let score = token_sort_ratio(query, key(item));
        if score >= MATCH_THRESHOLD && best.is_none_or(|(bs, _)| score > bs) {
            best = Some((score, item));
        }
    }
    best.map(|(score, item)| BestMatch { item, score })
}

/// True when `s` looks like a UUID (8-4-4-4-12 hex).
///
/// Delegates to the canonical check in `pbotcli-types` (which the identity
/// helpers there also use) so the shape is defined in exactly one place.
pub fn is_uuid(s: &str) -> bool {
    pbotcli_types::is_uuid(s)
}

/// How a project query was matched: exact identifier, exact name, or fuzzy name.
pub enum ProjectMatch {
    ExactIdentifier(Project),
    ExactName(Project),
    Fuzzy(Project),
}

/// Match a project reference: exact identifier first (case-insensitive), then exact
/// name (case-insensitive), then fuzzy name. Exact hits win over a closer fuzzy match
/// — an identifier like `RENG` must never be resolved onto a different project by name.
pub fn match_project(query: &str, projects: &[Project]) -> Option<ProjectMatch> {
    if query.trim().is_empty() {
        return None;
    }
    projects
        .iter()
        .find(|p| {
            p.identifier
                .as_deref()
                .is_some_and(|i| i.eq_ignore_ascii_case(query))
        })
        .map(|p| ProjectMatch::ExactIdentifier(p.clone()))
        .or_else(|| {
            projects
                .iter()
                .find(|p| {
                    p.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(query))
                })
                .map(|p| ProjectMatch::ExactName(p.clone()))
        })
        .or_else(|| {
            find_best_match(query, projects, |p| p.name.as_deref().unwrap_or(""))
                .map(|m| ProjectMatch::Fuzzy(m.item.clone()))
        })
}

/// Resolve a project by UUID, exact identifier, exact name, or fuzzy name.
/// A fuzzy (name-only) fallback prints a warning instead of silently landing on
/// a different project.
pub async fn resolve_project(query: &str, client: &PlaneClient) -> Result<Project, PlaneError> {
    if is_uuid(query) {
        return client.get_project(query).await;
    }
    let projects = client.list_projects().await?;
    match match_project(query, &projects) {
        Some(ProjectMatch::ExactIdentifier(p)) | Some(ProjectMatch::ExactName(p)) => Ok(p),
        Some(ProjectMatch::Fuzzy(p)) => {
            let shown = p
                .name
                .as_deref()
                .or(p.identifier.as_deref())
                .unwrap_or_default();
            eprintln!(
                "warning: no exact project named or identified '{query}'; matched '{shown}' — pass the exact name or identifier to target it precisely",
            );
            Ok(p)
        }
        None => Err(PlaneError::NotFound {
            message: format!("Project not found: {query}"),
            hint: None,
        }),
    }
}

/// Resolve a user reference to an (id, display name) pair: `me` resolves to
/// the authenticated user, a UUID is accepted as-is, otherwise the query is
/// fuzzy-matched against workspace member names.
///
/// # `me` under a service principal
///
/// A service-principal credential has **two** identities, and they are not
/// interchangeable:
///
/// * `ServicePrincipal.id` — what `/api/v1/users/me/` answers with, what
///   `whoami` prints, and what an API key authenticates as. It is *not* a
///   member: no `WorkspaceMember` row carries it, so putting it in an
///   assignment payload silently assigns nobody and filtering on it returns an
///   empty list.
/// * the **shadow bot** `User` — the row Plane credits every SP write to
///   (`service_principals/attribution.py`; `display_name` `svc-<name> (bot)`,
///   `first_name` `svc-<name>`). It is the member id an assignment must carry.
///
/// `me` therefore resolves to the shadow bot member for an SP, and to the
/// credential's own id for a human (unchanged). When the shadow bot cannot be
/// found the call fails with a hint naming the next command instead of
/// returning an id that would do nothing.
pub async fn resolve_user_query(
    query: &str,
    client: &PlaneClient,
) -> Result<ResolvedUser, PlaneError> {
    if query.eq_ignore_ascii_case("me") {
        let me = client.get_me().await?;
        if me.is_service_principal() {
            return resolve_service_principal_self(&me, client).await;
        }
        let name = me
            .display_name
            .clone()
            .or_else(|| me.first_name.clone())
            .unwrap_or_default();
        return Ok(ResolvedUser {
            id: me.id,
            name,
            service_principal: None,
        });
    }
    let members = match client.list_members().await {
        Ok(members) => members,
        // A UUID reference does not need the member list to be usable — the list
        // only supplies the display name — so an unreadable list must not turn
        // an explicit id into a failure. A name query, and `me` for a service
        // principal, do need it and fail loudly (with `me`'s own hint, below).
        Err(err) if is_uuid(query) && pbotcli_client::is_resource_access_denied(&err) => {
            eprintln!(
                "warning: could not read the workspace member list ({err}); using the id as the name"
            );
            return Ok(ResolvedUser::explicit(query, query));
        }
        Err(err) => return Err(err),
    };
    if is_uuid(query) {
        if let Some(m) = members.iter().find(|m| m.id == query) {
            return Ok(ResolvedUser::explicit(&m.id, &m.full_name()));
        }
        // Not present in the member list: accept the UUID as-is.
        return Ok(ResolvedUser::explicit(query, query));
    }
    let with_names: Vec<(pbotcli_types::Member, String)> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m, name)
        })
        .collect();
    if let Some(m) = find_best_match(query, &with_names, |(_, name)| name.as_str()) {
        return Ok(ResolvedUser::explicit(&m.item.0.id, &m.item.1));
    }
    Err(PlaneError::NotFound {
        message: format!("User not found: {query}"),
        hint: None,
    })
}

/// A resolved user reference.
#[derive(Debug, Clone)]
pub struct ResolvedUser {
    /// The id to put in an API payload (a workspace member id).
    pub id: String,
    /// The name to show for the resolved member.
    pub name: String,
    /// Set only when `me` resolved through a **service principal**: what that
    /// principal is and which shadow bot member it mapped to. `None` for a human
    /// credential and for an explicit reference, whose messages are unchanged.
    pub service_principal: Option<ServicePrincipalSelf>,
}

/// A service-principal credential's own identity, as `me` resolved it.
#[derive(Debug, Clone)]
pub struct ServicePrincipalSelf {
    /// The principal's label — `kimicode-bot (service principal)`, or
    /// `… (SP owned by <owner>)` when `/users/me/` names the owner.
    pub label: String,
    /// The shadow bot workspace member every `me` payload carries.
    pub member_id: String,
    /// That member's server-composed name (`svc-<principal name> (bot)`).
    pub member_name: String,
}

impl ServicePrincipalSelf {
    /// One sentence naming what `me` resolved to, so an unattended caller can
    /// see the mapping — principal, shadow bot member, member id — from the
    /// output alone, without reading the source.
    pub fn resolved_to(&self) -> String {
        format!(
            "{label} — a ServicePrincipal id is not a workspace member row, so 'me' maps to its \
             shadow bot member '{member}' (member id {id})",
            label = self.label,
            member = self.member_name,
            id = self.member_id,
        )
    }

    /// How a write made through `me` names its attribution: the principal, the
    /// shadow bot member the payload carried, and that member's id. The single
    /// source of this sentence, so `wi assign`, `wi create` and `wi update`
    /// cannot drift apart.
    pub fn attributed_write(&self) -> String {
        format!(
            "{label} as its shadow bot member '{member}' (member id {id})",
            label = self.label,
            member = self.member_name,
            id = self.member_id,
        )
    }
}

impl ResolvedUser {
    fn explicit(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            service_principal: None,
        }
    }

    /// What to show for this resolution: the service principal's label when `me`
    /// went through one, the member's name otherwise.
    pub fn display(&self) -> &str {
        match &self.service_principal {
            Some(sp) => &sp.label,
            None => &self.name,
        }
    }
}

/// Resolve `me` for a service-principal credential to its shadow bot member.
async fn resolve_service_principal_self(
    me: &pbotcli_types::User,
    client: &PlaneClient,
) -> Result<ResolvedUser, PlaneError> {
    let sp_name = me.display_label();
    // A payload with no name cannot identify a shadow bot member, and "expected
    // a member named 'svc- (bot)'" would be a riddle: say what is missing.
    if sp_name.trim().is_empty() {
        return Err(PlaneError::not_found_with_hint(
            format!(
                "Cannot resolve 'me': the service principal {id} answered /users/me/ without a \
                 name, so its shadow bot workspace member cannot be identified.",
                id = me.id,
            ),
            "Run 'pbot user ls' to list the workspace members, then pass --assignee with the \
             member's name or UUID.",
        ));
    }
    let members = match client.list_members().await {
        Ok(members) => members,
        // `me` under an SP is resolved through the member list, so a denial
        // there cannot be answered by the SP's own id (it is not a member) and
        // must not surface as a bare API error: say what was denied and what to
        // run instead.
        Err(err) if pbotcli_client::is_resource_access_denied(&err) => {
            return Err(PlaneError::not_found_with_hint(
                format!(
                    "Cannot resolve 'me': {identity} is resolved through the workspace member \
                     list, which this credential cannot read ({err}).",
                    identity = me.identity_label(),
                ),
                "Run 'pbot user ls' to check what this credential can read, then pass --assignee \
                 with an explicit member name or UUID.",
            ));
        }
        Err(err) => return Err(err),
    };
    match find_shadow_bot(&members, &sp_name) {
        Some(member) => Ok(ResolvedUser {
            id: member.id.clone(),
            name: member.full_name(),
            service_principal: Some(ServicePrincipalSelf {
                label: me.identity_label(),
                member_id: member.id.clone(),
                // The matched row's own name, so the caller can find it in
                // `pbot user ls`; the server's composition rule is only the
                // fallback for a row with no usable name.
                member_name: member_display_name(member)
                    .unwrap_or_else(|| format!("svc-{sp_name} (bot)")),
            }),
        }),
        None => Err(PlaneError::not_found_with_hint(
            format!(
                "Cannot resolve 'me': {identity} has no shadow bot workspace member \
                 (expected a member named 'svc-{sp_name} (bot)'). A ServicePrincipal id is not a \
                 member row, so assigning it would silently do nothing.",
                identity = me.identity_label(),
            ),
            "Run 'pbot user ls' to list the workspace members, then pass --assignee with the \
             member's name or UUID.",
        )),
    }
}

/// The name to show for a resolved member: its `display_name` when the row has
/// one, else its full name. `None` when the row names itself nowhere.
fn member_display_name(member: &pbotcli_types::Member) -> Option<String> {
    member
        .display_name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            let full = member.full_name();
            (!full.trim().is_empty()).then_some(full)
        })
}

/// Read one project's work items, answering `Ok(None)` when the credential may
/// not read *that project* (HTTP 403 — what a service principal gets on every
/// project it has no grant for — or 404).
///
/// The single skip decision shared by both cross-project scans
/// ([`locate_work_item_across`] and the `wi ls` project loop), so they cannot
/// drift apart. Only this call decides a skip: a denial on a project's secondary
/// reads (states, labels, members) is a different failure and must not be
/// reported as "no read access to this project". Anything that is not a
/// per-resource denial propagates.
pub async fn list_work_items_or_skip(
    client: &PlaneClient,
    project_id: &str,
) -> Result<Option<Vec<pbotcli_types::WorkItem>>, PlaneError> {
    match client.list_work_items(project_id).await {
        Ok(items) => Ok(Some(items)),
        Err(err) if pbotcli_client::is_resource_access_denied(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

/// The workspace member that is the shadow bot user of the service principal
/// named `sp_name`.
///
/// Matching is [`pbotcli_types::Member::matches_service_principal`]: the server
/// composes the row as `display_name` `svc-<name> (bot)` / `first_name`
/// `svc-<name>` (`service_principals/attribution.py`), and only a name carrying
/// that composition counts — a human member who happens to share the SP's name
/// is never resolved as its bot.
pub fn find_shadow_bot<'a>(
    members: &'a [pbotcli_types::Member],
    sp_name: &str,
) -> Option<&'a pbotcli_types::Member> {
    members
        .iter()
        .find(|member| member.matches_service_principal(sp_name))
}

/// The body of the one-line warning a cross-project scan prints after skipping
/// projects the credential cannot read; callers print it as
/// `warning: {note}`. Built here so the work-item lookup and the `wi ls` listing
/// cannot drift apart.
///
/// `verb` is the caller's own word for what it was doing (`searched` /
/// `listed`). The wording adapts to what actually happened, so the advice is
/// never nonsense: a scan that covered some projects suggests `-p` to target a
/// readable one, a single explicit project says plainly that it is unreadable
/// (advising `-p` to someone who already passed `-p` helps nobody), and a scan
/// where *every* project was skipped says so instead of "0 of N".
pub fn skipped_projects_note(verb: &str, total: usize, skipped: &[String]) -> String {
    let denied = skipped.join(", ");
    let scanned = total.saturating_sub(skipped.len());
    if total == 1 {
        return format!("no read access to {denied}");
    }
    if scanned == 0 {
        return format!("{verb} none of {total} projects — no read access to {denied}");
    }
    format!(
        "{verb} {scanned} of {total} projects; no read access to {denied} — pass -p <project> to \
         target one directly"
    )
}

/// A located work item: the raw item plus the project context it lives in.
#[derive(Debug, Clone)]
pub struct LocatedWorkItem {
    pub item: pbotcli_types::WorkItem,
    pub project_id: String,
    pub project_identifier: String,
}

/// Match one query (UUID, full identifier `PROJ-123`, or name) against a list
/// of `(work_item, project_identifier)` candidates.
///
/// `project_identifier` is the project's string identifier (e.g. `PLANE`),
/// the prefix composed with a work item's `sequence_id` to form `PROJ-N`.
///
/// Two passes, with the first one looking only at exact id and identifier-plus-
/// sequence matches. `sequence_id` is unique within a project, so any exact hit
/// in pass 1 is unambiguous and wins over later passes — this prevents a
/// substring-of-name match on an unrelated item from hijacking a short
/// identifier query (e.g. `PLANE-3` being silently returned when some task's
/// title contains the literal `PLANE-3`). Pass 2 keeps the existing
/// substring-then-fuzzy behaviour so callers relying on partial-name matching
/// see no change.
///
/// An identifier query that misses in pass 1 is a miss, not a name to guess at:
/// pass 2 is skipped entirely, so `PLANECLI-99` (an item that does not exist)
/// reports NotFound instead of landing on whichever item happens to contain
/// that string in its title or scores closest to it. "Identifier query" means
/// `<known project identifier>-<digits>` — the prefix has to be one of the
/// project identifiers being searched, so an ordinary hyphenated name query
/// (`hotfix-2` in a workspace with no `hotfix` project) still resolves by name.
fn match_work_item(
    query: &str,
    candidates: &[(pbotcli_types::WorkItem, &str)],
) -> Option<LocatedWorkItem> {
    let lower = query.to_lowercase();
    // Pass 1: exact UUID or `PROJ-N` match, scanning the whole list. Returning
    // the first hit is safe because these identifiers are unique per project.
    // A UUID-shaped query cannot match any `PROJ-N` composed sequence, so we
    // skip the per-item sequence-id parse in that case.
    let query_is_uuid = is_uuid(query);
    for (item, identifier) in candidates {
        if item.id.eq_ignore_ascii_case(query) {
            return Some(make_located(item, identifier));
        }
        if !query_is_uuid {
            let seq = item.sequence_id.as_ref().map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            });
            let composed = seq
                .filter(|s| !s.is_empty() && !identifier.is_empty())
                .map(|s| format!("{identifier}-{s}"));
            if composed
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(query))
            {
                return Some(make_located(item, identifier));
            }
        }
    }
    // Pass 2: substring-on-name (preserved from before the fix so existing
    // callers' UX is unchanged) and fuzzy fallback. An identifier-shaped query
    // that reached this point already missed on the exact pass, so it gets
    // neither: guessing a target for a reference the user spelled precisely is
    // how a non-existent short number silently resolved to a different item.
    if is_identifier_query(query, candidates) {
        return None;
    }
    for (item, identifier) in candidates {
        let name = item.name.as_deref().unwrap_or("");
        if !is_uuid(query) && name.to_lowercase().contains(&lower) {
            return Some(make_located(item, identifier));
        }
    }
    let with_names: Vec<(&pbotcli_types::WorkItem, &str, String)> = candidates
        .iter()
        .map(|(item, identifier)| (item, *identifier, item.name.clone().unwrap_or_default()))
        .collect();
    if let Some(m) = find_best_match(query, &with_names, |(_, _, name)| name.as_str()) {
        let (item, identifier, _) = m.item;
        return Some(make_located(item, identifier));
    }
    None
}

/// True when `query` references a work item the way a work-item identifier does
/// — `<project identifier>-<sequence>` — and that prefix is one of the project
/// identifiers in `candidates`. Purely numeric suffixes only: `ABC-1` is an
/// identifier, `ABC-123abc` and `hotfix-2 rollout` are names.
fn is_identifier_query(query: &str, candidates: &[(pbotcli_types::WorkItem, &str)]) -> bool {
    if is_uuid(query) {
        return false;
    }
    let Some((prefix, seq)) = query.rsplit_once('-') else {
        return false;
    };
    if prefix.is_empty() || seq.is_empty() || !seq.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    candidates
        .iter()
        .any(|(_, identifier)| !identifier.is_empty() && identifier.eq_ignore_ascii_case(prefix))
}

/// Wrap a candidate `(item, project_identifier)` pair in a `LocatedWorkItem`,
/// copying the project's UUID and identifier onto the result. Centralised so
/// the two match passes and the fuzzy fallback all produce the same shape.
fn make_located(item: &pbotcli_types::WorkItem, identifier: &str) -> LocatedWorkItem {
    LocatedWorkItem {
        item: item.clone(),
        project_id: item.project.clone().unwrap_or_default(),
        project_identifier: identifier.to_string(),
    }
}

/// Locate a work item within a single project (from its work-item list).
pub async fn locate_work_item_in_project(
    query: &str,
    project: &pbotcli_types::Project,
    client: &PlaneClient,
) -> Result<LocatedWorkItem, PlaneError> {
    let items = client.list_work_items(&project.id).await?;
    let identifier = project.identifier.clone().unwrap_or_default();
    let candidates: Vec<(pbotcli_types::WorkItem, &str)> = items
        .iter()
        .map(|i| (i.clone(), identifier.as_str()))
        .collect();
    match_work_item(query, &candidates).ok_or_else(|| PlaneError::NotFound {
        message: format!("Work item not found: {query}"),
        hint: None,
    })
}

/// Locate a work item across all projects, matching by UUID, full identifier,
/// or name. Cross-project searches fire many requests, so projects are paced.
///
/// A project the credential cannot read is skipped, not fatal. A service
/// principal is granted per project, so it answers 403 on every project it has
/// no grant for (a project it cannot see at all answers 404): aborting the scan
/// on the first one made `pbot wi show <query>` without `-p` fail even when a
/// readable project held the item. Failures that are not a per-resource access
/// denial — a rejected credential, a transport error, a 5xx — still propagate.
pub async fn locate_work_item_across(
    query: &str,
    client: &PlaneClient,
) -> Result<LocatedWorkItem, PlaneError> {
    let projects = client.list_projects().await?;
    // Identifiers are borrowed from `projects`, which outlives the scan, so the
    // candidate list is built without a per-item identifier clone and without a
    // second cloned copy for matching.
    let mut candidates: Vec<(pbotcli_types::WorkItem, &str)> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for (i, project) in projects.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let Some(items) = list_work_items_or_skip(client, &project.id).await? else {
            skipped.push(project_label(project));
            continue;
        };
        let identifier = project.identifier.as_deref().unwrap_or("");
        candidates.extend(items.into_iter().map(|item| (item, identifier)));
    }
    if !skipped.is_empty() {
        eprintln!(
            "warning: {}",
            skipped_projects_note("searched", projects.len(), &skipped)
        );
    }
    match match_work_item(query, &candidates) {
        Some(found) => Ok(found),
        None if !skipped.is_empty() => Err(PlaneError::not_found_with_hint(
            format!("Work item not found: {query}"),
            format!(
                "The search skipped {} project(s) this credential cannot read: {}. \
                 Pass -p <project> to search a readable project directly.",
                skipped.len(),
                skipped.join(", ")
            ),
        )),
        None => Err(PlaneError::NotFound {
            message: format!("Work item not found: {query}"),
            hint: None,
        }),
    }
}

/// How a skipped project is named in a warning or hint: its name, else its
/// identifier, else its UUID. Public so both cross-project scans (the work-item
/// lookup and the `wi ls` listing) name a skipped project the same way.
pub fn project_label(project: &pbotcli_types::Project) -> String {
    project
        .name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| project.identifier.clone().filter(|i| !i.trim().is_empty()))
        .unwrap_or_else(|| project.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_close_names() {
        let items = vec!["Frontend".to_string(), "Backend API".to_string()];
        let m = find_best_match("frontend", &items, |s| s).unwrap();
        assert_eq!(*m.item, "Frontend");
    }

    #[test]
    fn rejects_distant_names() {
        let items = vec!["Alpha".to_string()];
        assert!(find_best_match("zzz nonexistent", &items, |s| s).is_none());
    }

    #[test]
    fn detects_uuids() {
        assert!(is_uuid("5963a34f-1038-43b2-90aa-cc2b183a30e2"));
        assert!(!is_uuid("PLANECLI-9"));
        assert!(!is_uuid("not-a-uuid"));
    }

    fn proj(id: &str, name: &str, identifier: &str) -> Project {
        Project {
            id: id.to_string(),
            name: Some(name.to_string()),
            identifier: Some(identifier.to_string()),
            description: None,
            intake_view: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn project_identifier_wins_over_fuzzy_name() {
        // RENG and SIRENA both fuzzy-match "reng"; the exact identifier must win.
        let projects = vec![
            proj("p-reng", "ReviewEngine", "RENG"),
            proj("p-sirena", "Sirena", "SIRENA"),
        ];
        let m = match_project("RENG", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-reng"));
        let m = match_project("reng", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-reng"));
    }

    #[test]
    fn project_exact_name_beats_fuzzy_name() {
        let projects = vec![
            proj("p-plane", "PlaneCommunity", "PLANE"),
            proj("p-planec", "PlaneCLI", "PLANECLI"),
        ];
        // "PLANE" is p-plane's identifier → exact identifier wins.
        let m = match_project("PLANE", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-plane"));
        // "PlaneCLI" is also p-planec's identifier → identifier beats name.
        let m = match_project("PlaneCLI", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-planec"));
        // "planecommunity" equals p-plane's name (case-insensitive) → exact name.
        let m = match_project("planecommunity", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactName(p) if p.id == "p-plane"));
    }

    #[test]
    fn project_fuzzy_only_when_no_exact_hit() {
        let projects = vec![
            proj("p-reng", "ReviewEngine", "RENG"),
            proj("p-sirena", "Sirena", "SIRENA"),
        ];
        // No exact identifier/name for "Siren"; falls back to fuzzy → Sirena.
        let m = match_project("Siren", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::Fuzzy(p) if p.id == "p-sirena"));
        // A query that matches nothing exactly and is too distant fuzzy-wise → None.
        assert!(match_project("zzz-nothing", &projects).is_none());
    }

    #[test]
    fn project_empty_query_has_no_match() {
        let projects = vec![proj("p-reng", "ReviewEngine", "RENG")];
        assert!(match_project("", &projects).is_none());
        assert!(match_project("   ", &projects).is_none());
    }

    fn wi(id: &str, name: &str, seq: u64) -> pbotcli_types::WorkItem {
        pbotcli_types::WorkItem {
            id: id.to_string(),
            name: Some(name.to_string()),
            sequence_id: Some(serde_json::Value::Number(seq.into())),
            description_html: None,
            priority: None,
            state: None,
            labels: None,
            assignees: None,
            parent: None,
            start_date: None,
            target_date: None,
            created_at: None,
            updated_at: None,
            project: Some("p-plane".to_string()),
            project_identifier: Some("PLANE".to_string()),
            ..Default::default()
        }
    }

    fn cands<'a>(items: &[&'a pbotcli_types::WorkItem]) -> Vec<(pbotcli_types::WorkItem, &'a str)> {
        items.iter().map(|w| ((*w).clone(), "PLANE")).collect()
    }

    #[test]
    fn seq_match_wins_over_name_substring_hijack() {
        // PLANE-56's title contains the literal substring "PLANE-3" (from a
        // sentence referencing "PLANE-33"); the old code returned PLANE-56 for
        // query "PLANE-3" whenever the API returned it earlier in the list.
        // The new pass-1 sweep finds PLANE-3 by exact sequence match first.
        let wi3 = wi("i-3", "WebUI 配置 allowed_rate_limit", 3);
        let wi56 = wi(
            "i-56",
            "根因定位：Pages 内容整篇复制膨胀为何复发（PLANE-33 修复后的漏网路径）",
            56,
        );
        // Whichever order the API returns them, the query must land on wi3.
        for order in [&[&wi3, &wi56] as &[&_], &[&wi56, &wi3]] {
            let candidates = cands(order);
            let m = match_work_item("PLANE-3", &candidates).unwrap();
            assert_eq!(m.item.id, "i-3", "PLANE-3 must win regardless of order");
        }
    }

    #[test]
    fn seq_match_wins_for_short_prefix_of_existing_seq() {
        // Querying a short identifier that's a prefix of another item's seq
        // (e.g. PLANE-3 vs PLANE-33) must still resolve to the exact item
        // when present, even when the prefix-larger item appears earlier in
        // the list.
        let wi3 = wi("i-3", "Plain task", 3);
        let wi33 = wi("i-33", "PLANE-3 hijack target", 33);
        for order in [&[&wi33, &wi3] as &[&_], &[&wi3, &wi33]] {
            let candidates = cands(order);
            let m = match_work_item("PLANE-3", &candidates).unwrap();
            assert_eq!(m.item.id, "i-3", "PLANE-3 must win regardless of order");
        }
    }

    #[test]
    fn uuid_match_wins_over_name_substring() {
        // If the user passes a UUID-shaped query that happens to appear as a
        // substring in another item's name, the exact UUID match still wins.
        let wi_uuid = wi("11111111-2222-3333-4444-555555555555", "Plain task", 5);
        let wi_other = wi(
            "i-other",
            "see 11111111-2222-3333-4444-555555555555 in the docs",
            7,
        );
        let candidates = cands(&[&wi_other, &wi_uuid]);
        let m = match_work_item("11111111-2222-3333-4444-555555555555", &candidates).unwrap();
        assert_eq!(m.item.id, "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn substring_falls_through_when_no_exact_match() {
        // No exact id/seq match for "login", but a task name does.
        let wi = wi("i-login", "Login bug fix", 1);
        let candidates = cands(&[&wi]);
        let m = match_work_item("login", &candidates).unwrap();
        assert_eq!(m.item.id, "i-login");
    }

    #[test]
    fn missing_short_id_does_not_hijack_via_name() {
        // PLANE-999 is not in the project, but PLANE-56's title contains the
        // substring "PLANE-9" via "PLANE-99". The old code would return
        // PLANE-56; the new code returns None.
        let wi56 = wi("i-56", "see PLANE-99 plans", 56);
        let candidates = cands(&[&wi56]);
        assert!(match_work_item("PLANE-999", &candidates).is_none());
    }

    fn cands_with<'a>(
        items: &[&'a pbotcli_types::WorkItem],
        identifier: &'a str,
    ) -> Vec<(pbotcli_types::WorkItem, &'a str)> {
        items.iter().map(|w| ((*w).clone(), identifier)).collect()
    }

    #[test]
    fn absent_identifier_query_is_not_found_instead_of_hijacked_by_title() {
        // The live PLANECLI-84 shape: PLANECLI-99 does not exist, and the item
        // that documents the bug carries "PLANECLI-99" in its own title, so the
        // substring pass handed back that unrelated item.
        let wi84 = wi(
            "i-84",
            "fix: 不存在的短号被静默解析成别的条目（PLANECLI-99 → PLANECLI-10）",
            84,
        );
        let candidates = cands_with(&[&wi84], "PLANECLI");
        assert!(match_work_item("PLANECLI-99", &candidates).is_none());
    }

    #[test]
    fn absent_identifier_query_is_not_found_instead_of_fuzzy_matched() {
        // No substring either — the title spells the number without the dash, so
        // only the rapidfuzz fallback could serve this query. It must not.
        let wi = wi("i-84", "PLANECLI99 rollout", 84);
        let candidates = cands_with(&[&wi], "PLANECLI");
        assert!(match_work_item("PLANECLI-99", &candidates).is_none());
    }

    #[test]
    fn name_query_with_a_non_identifier_prefix_still_resolves() {
        // "hotfix" is not a project identifier, so a hyphenated name query keeps
        // the substring/fuzzy fallback: only `<known identifier>-<digits>` is strict.
        let wi = wi("i-hf", "hotfix-2 redeploy", 4);
        let candidates = cands_with(&[&wi], "PLANE");
        let m = match_work_item("hotfix-2", &candidates).expect("name fallback must still run");
        assert_eq!(m.item.id, "i-hf");
    }

    fn member(id: &str, display: &str, first: &str) -> pbotcli_types::Member {
        pbotcli_types::Member {
            id: id.to_string(),
            display_name: Some(display.to_string()),
            first_name: Some(first.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn find_shadow_bot_matches_the_server_composition() {
        // Live shape: display_name carries the composition, first_name the bare
        // SP name (a deployment predating `bot_first_name`).
        let members = vec![
            member("u-human", "Jicai.Liu", "Jicai"),
            member(
                "e-shadow",
                "svc-Kimicode IsletspaceWorkstation (bot)",
                "Kimicode IsletspaceWorkstation",
            ),
        ];
        let found = find_shadow_bot(&members, "Kimicode IsletspaceWorkstation")
            .expect("the shadow bot row must be found");
        assert_eq!(found.id, "e-shadow");
    }

    #[test]
    fn find_shadow_bot_prefers_the_bot_row_over_a_same_named_human() {
        // The human shares the SP's name but carries no bot composition, so the
        // row that does is the only match.
        let members = vec![
            member("u-human", "Deploy Bot", "deploy-bot"),
            member("e-shadow", "svc-deploy-bot (bot)", "svc-deploy-bot"),
        ];
        let found = find_shadow_bot(&members, "deploy-bot").expect("found");
        assert_eq!(found.id, "e-shadow");
    }

    #[test]
    fn find_shadow_bot_returns_none_when_no_row_matches() {
        let members = vec![member("u-human", "Jicai.Liu", "Jicai")];
        assert!(find_shadow_bot(&members, "deploy-bot").is_none());
        // A blank SP name matches nothing, not everything.
        assert!(find_shadow_bot(&members, "").is_none());
        // A bare name equal to the SP's is a human, not the bot: resolving it
        // would assign the bot's work to that person.
        let same_name_human = vec![member("u-human", "Deploy Bot", "deploy-bot")];
        assert!(find_shadow_bot(&same_name_human, "deploy-bot").is_none());
    }

    #[test]
    fn project_label_prefers_name_then_identifier_then_id() {
        assert_eq!(project_label(&proj("p-1", "Demo", "DEMO")), "Demo");
        let mut no_name = proj("p-2", "", "DEMO2");
        no_name.name = None;
        assert_eq!(project_label(&no_name), "DEMO2");
        let mut bare = proj("p-3", "", "");
        bare.name = None;
        bare.identifier = None;
        assert_eq!(project_label(&bare), "p-3");
    }
}
