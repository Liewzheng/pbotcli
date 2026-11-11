//! Resolution of the batch resources.
//!
//! A batch is named by its number (`RUN-<YYYYMMDD>-<NN>`), and the number
//! is assigned by the server — so a reference is resolved by reading the
//! batches of the project and looking the number up, exactly like the
//! other resources of the workspace are resolved by their name.
//!
//! The resolvers live in the resolve crate (and not in a command handler)
//! because both the batch itself and the case records it holds need them.

use pbotcli_client::PlaneClient;
use pbotcli_core::PlaneError;
use pbotcli_types::testrun::{TestRun, TestRunCase, TestRunFilters};

use crate::is_uuid;

/// Resolve a batch by its number, its uuid, or its date.
///
/// The number is tried first (it is the name of a batch); a uuid is read
/// straight from the endpoint; anything else is matched against the
/// numbers of the batches of the project.
///
/// The batch is read back by its id, so the caller holds the whole
/// record — the cases it holds included.
pub async fn resolve_test_run(
    client: &PlaneClient,
    project_id: &str,
    query: &str,
) -> Result<TestRun, PlaneError> {
    let id = if is_uuid(query) {
        query.to_string()
    } else {
        let wanted = query.trim();
        let filters = TestRunFilters::default();
        let runs = client.list_test_runs(project_id, &filters).await?;
        // The number is the only name a batch has, so there is nothing
        // to match against but the number itself — the date is the
        // second half of it, and a batch of that day is the closest a
        // caller gets.
        let found = exact_number(&runs, wanted)
            .or_else(|| runs.iter().find(|run| run.run_date == wanted).cloned());
        match found {
            Some(run) => run.id,
            None => {
                return Err(PlaneError::NotFound {
                    message: format!("Test run not found: {query}"),
                    hint: Some(
                        "Run `pbot run ls -p <project>` to see the batches of the project."
                            .to_string(),
                    ),
                });
            }
        }
    };
    client
        .get_test_run(project_id, &id)
        .await
        .map_err(|e| match e {
            PlaneError::NotFound { .. } => PlaneError::NotFound {
                message: format!("Test run not found: {query}"),
                hint: None,
            },
            other => other,
        })
}

/// The case record of a batch, by its uuid or its number.
///
/// The number of the case is the name it carries inside the batch — the
/// number of the test case it was made of, frozen into the snapshot.
pub fn find_run_case(run: &TestRun, query: &str) -> Option<TestRunCase> {
    let wanted = query.trim();
    run.cases
        .iter()
        .find(|case| case.id.eq_ignore_ascii_case(wanted))
        .or_else(|| {
            run.cases
                .iter()
                .find(|case| case.case_number.eq_ignore_ascii_case(wanted))
        })
        .cloned()
}

/// The batch whose number is `wanted`, whatever the case of its letters.
fn exact_number(runs: &[TestRun], wanted: &str) -> Option<TestRun> {
    runs.iter()
        .find(|run| run.number.eq_ignore_ascii_case(wanted))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(number: &str, run_date: &str) -> TestRun {
        TestRun {
            id: format!("id-{number}"),
            number: number.to_string(),
            run_date: run_date.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_number_is_matched_whatever_the_case_of_its_letters() {
        let runs = [
            run("RUN-20261109-01", "2026-11-09"),
            run("RUN-20261109-02", "2026-11-09"),
        ];
        let found = exact_number(&runs, "run-20261109-02").expect("the number matches");
        assert_eq!(found.id, "id-RUN-20261109-02");
        assert!(exact_number(&runs, "RUN-20261109-03").is_none());
    }

    #[test]
    fn a_number_that_is_not_there_is_not_matched() {
        let runs = [run("RUN-20261109-01", "2026-11-09")];
        assert!(exact_number(&runs, "RUN-20261108-01").is_none());
        assert!(exact_number(&runs, "").is_none());
    }

    #[test]
    fn a_case_of_a_batch_is_matched_by_its_number() {
        let run = TestRun {
            cases: vec![TestRunCase {
                id: "rc1".into(),
                case_number: "TC-CLI-007".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let found = find_run_case(&run, "tc-cli-007").expect("the number matches");
        assert_eq!(found.id, "rc1");
        assert!(find_run_case(&run, "TC-CLI-008").is_none());
        // The uuid of the record names it as well.
        assert!(find_run_case(&run, "rc1").is_some());
    }
}
