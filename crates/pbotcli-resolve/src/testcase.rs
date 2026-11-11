//! Resolution of the test-case resources.
//!
//! A case is named by its number (`TC-<module code>-<NNN>`), and the number
//! is assigned by the server — so a reference is resolved by reading the
//! cases of the project and looking the number up, exactly like the other
//! resources of the workspace are resolved by their name.
//!
//! The resolvers live in the resolve crate (and not in a command handler)
//! because both halves of the feature need them: the cases of a project
//! and the batches that hold them.

use pbotcli_client::PlaneClient;
use pbotcli_core::PlaneError;
use pbotcli_types::testcase::{TestCase, TestCaseFilters, TestCaseModule};

use crate::{find_best_match, is_uuid};

/// The cases of a project, with the voided ones among them.
///
/// A lookup has to find a case that was voided as well — the number of a
/// voided case is kept, so the number still names it.
async fn all_cases(client: &PlaneClient, project_id: &str) -> Result<Vec<TestCase>, PlaneError> {
    let filters = TestCaseFilters {
        status: Some("all".to_string()),
        ..Default::default()
    };
    client.list_test_cases(project_id, &filters).await
}

/// Resolve a test case by its number, its uuid, or its title.
///
/// The number is tried first (it is the name of a case); a uuid is read
/// straight from the endpoint; anything else is matched against the titles
/// of the cases of the project.
pub async fn resolve_test_case(
    client: &PlaneClient,
    project_id: &str,
    query: &str,
) -> Result<TestCase, PlaneError> {
    if is_uuid(query) {
        return client
            .get_test_case(project_id, query)
            .await
            .map_err(|e| match e {
                PlaneError::NotFound { .. } => PlaneError::NotFound {
                    message: format!("Test case not found: {query}"),
                    hint: None,
                },
                other => other,
            });
    }

    let wanted = query.trim();
    // The server can narrow the list down by the number and the title of a
    // case, so the common case (a number) costs one small request instead of
    // the whole list of the project.
    let filters = TestCaseFilters {
        status: Some("all".to_string()),
        search: Some(wanted.to_string()),
        ..Default::default()
    };
    let candidates = client.list_test_cases(project_id, &filters).await?;
    if let Some(case) = exact_number(&candidates, wanted) {
        return Ok(case);
    }
    if let Some(found) = find_best_match(wanted, &candidates, |case| case.title.as_str()) {
        return Ok(found.item.clone());
    }
    // The search of the server matches the number and the title, but a
    // fuzzy match may still find a case the search did not: the whole list
    // is the last resort.
    let cases = all_cases(client, project_id).await?;
    if let Some(case) = exact_number(&cases, wanted) {
        return Ok(case);
    }
    match find_best_match(wanted, &cases, |case| case.title.as_str()) {
        Some(found) => Ok(found.item.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("Test case not found: {query}"),
            hint: Some(
                "Run `pbot tc ls -p <project>` to see the cases of the project.".to_string(),
            ),
        }),
    }
}

/// The case whose number is `wanted`, whatever the case of its letters.
fn exact_number(cases: &[TestCase], wanted: &str) -> Option<TestCase> {
    cases
        .iter()
        .find(|case| case.number.eq_ignore_ascii_case(wanted))
        .cloned()
}

/// Resolve a module code of a project by its code or its uuid.
///
/// The code is what the caller types (`--module CLI`); the server stores
/// the module under a uuid, and the uuid is what the write of a case
/// carries.
pub async fn resolve_test_case_module(
    client: &PlaneClient,
    project_id: &str,
    query: &str,
) -> Result<TestCaseModule, PlaneError> {
    let modules = client.list_test_case_modules(project_id).await?;
    if is_uuid(query) {
        // A uuid of a module of this project is read out of the registry, so
        // the caller gets its code and its description as well. A uuid the
        // registry does not know is passed on as it is: the server is the
        // authority on which modules belong to the project, and it answers
        // `module_not_in_project`.
        if let Some(module) = modules.iter().find(|module| module.id == query) {
            return Ok(module.clone());
        }
        return Ok(TestCaseModule {
            id: query.to_string(),
            ..Default::default()
        });
    }
    let wanted = query.trim();
    if let Some(module) = modules
        .iter()
        .find(|module| module.code.eq_ignore_ascii_case(wanted))
    {
        return Ok(module.clone());
    }
    let found = find_best_match(wanted, &modules, |module| module.description.as_str());
    match found {
        Some(found) => Ok(found.item.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("Test case module not found: {query}"),
            hint: Some(
                "Run `pbot tc module ls -p <project>` to see the registered codes, or register one with `pbot tc module create`.".to_string(),
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(number: &str, title: &str) -> TestCase {
        TestCase {
            id: format!("id-{number}"),
            number: number.to_string(),
            title: title.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_module_code_is_matched_case_insensitively() {
        let modules = [TestCaseModule {
            id: "m1".into(),
            code: "CLI".into(),
            description: "The command line of pbot".into(),
            ..Default::default()
        }];
        let found = modules
            .iter()
            .find(|module| module.code.eq_ignore_ascii_case("cli"))
            .expect("the code matches whatever its case");
        assert_eq!(found.id, "m1");
    }

    #[test]
    fn a_case_carries_its_number_and_its_title() {
        let cases = [case("TC-CLI-001", "Log in with a valid account")];
        let found = cases
            .iter()
            .find(|c| c.number.eq_ignore_ascii_case("tc-cli-001"))
            .expect("the number matches whatever its case");
        assert_eq!(found.id, "id-TC-CLI-001");
    }
}
