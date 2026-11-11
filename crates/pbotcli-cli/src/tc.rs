//! The `tc` command group: the test cases of a project.
//!
//! A case is named by its number (`TC-<module code>-<NNN>`) and the server
//! assigns that number — the CLI never builds one. The module code of a
//! case is registered first (`pbot tc module create`), because the server
//! refuses a case whose module is not registered in the project.
//!
//! Every read command answers `--json` on stdout and a table on stderr, and
//! every write command prints the number the server assigned. A refused
//! write prints the **stable code** the server answered with
//! (`invalid_test_case`, `module_code_format`, …) together with the
//! sentence of the server, so a script can tell a validation failure
//! (exit 5) from a transport or permission failure (exit 2 / 4).

use std::path::PathBuf;

use clap::{ArgGroup, Subcommand};
use pbotcli_client::PlaneClient;
use pbotcli_core::PlaneError;
use pbotcli_format::{output_json, output_table};
use pbotcli_resolve::testcase::{resolve_test_case, resolve_test_case_module};
use pbotcli_resolve::{locate_work_item_in_project, resolve_project};
use pbotcli_types::Project;
use pbotcli_types::testcase::{
    TestCase, TestCaseFilters, TestCaseModuleWrite, TestCaseStep, TestCaseStepWrite,
    TestCaseStepsWrite, TestCaseWrite,
};
use serde_json::Value;

use crate::render::{test_case_module_view, test_case_step_view, test_case_view};

/// The subcommands of `pbot tc`.
#[derive(Subcommand)]
pub enum TcCmd {
    /// List the test cases of a project.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Filter by the module code (or the uuid of a module).
        #[arg(long)]
        module: Option<String>,
        /// Filter by priority: P0, P1, P2, P3.
        #[arg(long)]
        priority: Option<String>,
        /// Filter by a linked work item (identifier or UUID).
        #[arg(long)]
        issue: Option<String>,
        /// Filter by status: active (the default of the server) or all.
        #[arg(long)]
        status: Option<String>,
        /// Filter by the number or the title of a case.
        #[arg(long)]
        search: Option<String>,
        /// Maximum results to show.
        #[arg(long, short = 'l')]
        limit: Option<usize>,
    },
    /// Show one test case: its header fields, its steps and its work-item links.
    Show {
        /// Case number (TC-CLI-001), UUID, or title.
        case: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a test case.
    ///
    /// The number of the case is assigned by the server. The module code
    /// must be registered in the project (`pbot tc module create`).
    #[command(group(
        ArgGroup::new("tc_steps")
            .required(false)
            .multiple(true)
            .args(["step", "steps_file"])
    ))]
    Create {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Module code of the case (the prefix of its number).
        #[arg(long)]
        module: String,
        /// Title of the case.
        #[arg(long)]
        title: String,
        /// What the case checks.
        #[arg(long)]
        purpose: Option<String>,
        /// What is under test.
        #[arg(long, visible_alias = "object-overview")]
        object_overview: Option<String>,
        /// Priority: P0, P1, P2, P3.
        #[arg(long)]
        priority: Option<String>,
        /// What has to hold before the case runs.
        #[arg(long, visible_alias = "preconditions")]
        precondition: Option<String>,
        /// The environment and the version under test.
        #[arg(long, visible_alias = "env")]
        environment: Option<String>,
        /// The feature the case belongs to.
        #[arg(long, visible_alias = "related-feature")]
        related_feature: Option<String>,
        /// Free-form remarks.
        #[arg(long)]
        remarks: Option<String>,
        /// Status of the case: active (the default) or void.
        #[arg(long)]
        status: Option<String>,
        /// One step, written as "action | expected result" (repeatable).
        #[arg(long = "step", value_name = "ACTION|EXPECTED")]
        step: Vec<String>,
        /// A file with the whole step table (a JSON array, or one
        /// "action | expected result" per line).
        #[arg(long, value_name = "PATH")]
        steps_file: Option<PathBuf>,
    },
    /// Update a test case.
    ///
    /// The number of a case and the module it belongs to cannot be changed:
    /// the number is bound to the module, and the server keeps both.
    #[command(group(
        ArgGroup::new("tc_update_steps")
            .required(false)
            .multiple(true)
            .args(["step", "steps_file"])
    ))]
    Update {
        /// Case number (TC-CLI-001), UUID, or title.
        case: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New purpose.
        #[arg(long)]
        purpose: Option<String>,
        /// New object overview.
        #[arg(long, visible_alias = "object-overview")]
        object_overview: Option<String>,
        /// New priority: P0, P1, P2, P3.
        #[arg(long)]
        priority: Option<String>,
        /// New preconditions.
        #[arg(long, visible_alias = "preconditions")]
        precondition: Option<String>,
        /// New environment.
        #[arg(long, visible_alias = "env")]
        environment: Option<String>,
        /// New related feature.
        #[arg(long, visible_alias = "related-feature")]
        related_feature: Option<String>,
        /// New remarks.
        #[arg(long)]
        remarks: Option<String>,
        /// New status: active or void.
        #[arg(long)]
        status: Option<String>,
        /// The whole step table, one step per "action | expected result"
        /// (repeatable). The table is replaced as a whole.
        #[arg(long = "step", value_name = "ACTION|EXPECTED")]
        step: Vec<String>,
        /// A file with the whole step table (a JSON array, or one
        /// "action | expected result" per line).
        #[arg(long, value_name = "PATH")]
        steps_file: Option<PathBuf>,
        /// Refused: the module of a case cannot be changed.
        #[arg(long)]
        module: Option<String>,
        /// Refused: the number of a case is assigned by the server.
        #[arg(long)]
        number: Option<String>,
    },
    /// Read or replace the step table of a test case.
    ///
    /// Without a step the table is read; with steps the **whole** table is
    /// replaced — every step has to be passed again, the server drops the
    /// rows that are not in the list.
    #[command(group(
        ArgGroup::new("tc_steps_table")
            .required(false)
            .multiple(true)
            .args(["step", "steps_file"])
    ))]
    Steps {
        /// Case number (TC-CLI-001), UUID, or title.
        case: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// One step, written as "action | expected result" (repeatable).
        #[arg(long = "step", value_name = "ACTION|EXPECTED")]
        step: Vec<String>,
        /// A file with the whole step table (a JSON array, or one
        /// "action | expected result" per line).
        #[arg(long, value_name = "PATH")]
        steps_file: Option<PathBuf>,
    },
    /// List, add or remove the work-item links of a test case.
    #[command(group(
        ArgGroup::new("tc_issue_links")
            .required(false)
            .multiple(false)
            .args(["add", "remove"])
    ))]
    Issues {
        /// Case number (TC-CLI-001), UUID, or title.
        case: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Link a work item (identifier or UUID).
        #[arg(long, short = 'a')]
        add: Option<String>,
        /// Unlink a work item (identifier or UUID).
        #[arg(long, short = 'r')]
        remove: Option<String>,
    },
    /// Delete a test case.
    ///
    /// The server only lets a project admin remove a case; the refusal
    /// carries the server's own sentence.
    Delete {
        /// Case number (TC-CLI-001), UUID, or title.
        case: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// The registry of the module codes of a project.
    Module {
        #[command(subcommand)]
        command: TcModuleCmd,
    },
}

/// The subcommands of `pbot tc module`.
#[derive(Subcommand)]
pub enum TcModuleCmd {
    /// List the registered module codes.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Register a module code.
    ///
    /// The server owns the rules of a code: 2 to 6 uppercase letters,
    /// unique inside the project. A refused code is reported with the
    /// server's own code (`module_code_format`, `module_code_taken`) and
    /// its sentence.
    Create {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// The module code, for example CLI.
        #[arg(long)]
        code: String,
        /// What the module covers.
        #[arg(long, short = 'd')]
        description: Option<String>,
    },
}

/// Run a `tc` subcommand.
pub async fn dispatch(client: &PlaneClient, command: TcCmd, json: bool) -> Result<(), PlaneError> {
    match command {
        TcCmd::List {
            project,
            module,
            priority,
            issue,
            status,
            search,
            limit,
        } => {
            cmd_tc_list(
                client,
                project.as_deref(),
                module.as_deref(),
                priority.as_deref(),
                issue.as_deref(),
                status.as_deref(),
                search.as_deref(),
                limit.unwrap_or(50),
                json,
            )
            .await
        }
        TcCmd::Show { case, project } => cmd_tc_show(client, &case, project.as_deref(), json).await,
        TcCmd::Create {
            project,
            module,
            title,
            purpose,
            object_overview,
            priority,
            precondition,
            environment,
            related_feature,
            remarks,
            status,
            step,
            steps_file,
        } => {
            let opts = TcCaseWrite {
                module: Some(module.as_str()),
                title: Some(title.as_str()),
                purpose: purpose.as_deref(),
                object_overview: object_overview.as_deref(),
                priority: priority.as_deref(),
                precondition: precondition.as_deref(),
                environment: environment.as_deref(),
                related_feature: related_feature.as_deref(),
                remarks: remarks.as_deref(),
                status: status.as_deref(),
                step: &step,
                steps_file: steps_file.as_deref(),
            };
            cmd_tc_create(client, project.as_deref(), &opts, json).await
        }
        TcCmd::Update {
            case,
            project,
            title,
            purpose,
            object_overview,
            priority,
            precondition,
            environment,
            related_feature,
            remarks,
            status,
            step,
            steps_file,
            module,
            number,
        } => {
            // The number of a case and its module are fixed: the number is
            // bound to the module, and the server keeps both. The refusal
            // happens before any request, so nothing has to be undone.
            if let Some(module) = module.as_deref() {
                return Err(PlaneError::validation_with_hint(
                    format!("The module of a test case cannot be changed (got '{module}')."),
                    "The number of a case is bound to its module; create a case in the other module instead.",
                ));
            }
            if let Some(number) = number.as_deref() {
                return Err(PlaneError::validation_with_hint(
                    format!("The number of a test case cannot be changed (got '{number}')."),
                    "The server assigns the number of a case; it is part of the record of every run.",
                ));
            }
            let opts = TcCaseWrite {
                module: None,
                title: title.as_deref(),
                purpose: purpose.as_deref(),
                object_overview: object_overview.as_deref(),
                priority: priority.as_deref(),
                precondition: precondition.as_deref(),
                environment: environment.as_deref(),
                related_feature: related_feature.as_deref(),
                remarks: remarks.as_deref(),
                status: status.as_deref(),
                step: &step,
                steps_file: steps_file.as_deref(),
            };
            cmd_tc_update(client, &case, project.as_deref(), &opts, json).await
        }
        TcCmd::Steps {
            case,
            project,
            step,
            steps_file,
        } => {
            cmd_tc_steps(
                client,
                &case,
                project.as_deref(),
                &step,
                steps_file.as_deref(),
                json,
            )
            .await
        }
        TcCmd::Issues {
            case,
            project,
            add,
            remove,
        } => {
            cmd_tc_issues(
                client,
                &case,
                project.as_deref(),
                add.as_deref(),
                remove.as_deref(),
                json,
            )
            .await
        }
        TcCmd::Delete { case, project } => cmd_tc_delete(client, &case, project.as_deref()).await,
        TcCmd::Module { command } => match command {
            TcModuleCmd::List { project } => {
                cmd_tc_module_list(client, project.as_deref(), json).await
            }
            TcModuleCmd::Create {
                project,
                code,
                description,
            } => {
                cmd_tc_module_create(
                    client,
                    project.as_deref(),
                    &code,
                    description.as_deref(),
                    json,
                )
                .await
            }
        },
    }
}

/// The fields of a case write, gathered so the handler signatures stay
/// small.
struct TcCaseWrite<'a> {
    module: Option<&'a str>,
    title: Option<&'a str>,
    purpose: Option<&'a str>,
    object_overview: Option<&'a str>,
    priority: Option<&'a str>,
    precondition: Option<&'a str>,
    environment: Option<&'a str>,
    related_feature: Option<&'a str>,
    remarks: Option<&'a str>,
    status: Option<&'a str>,
    step: &'a [String],
    steps_file: Option<&'a std::path::Path>,
}

/// Resolve the project of a command — every command of the group needs one.
async fn require_project(
    client: &PlaneClient,
    project: Option<&str>,
) -> Result<Project, PlaneError> {
    let project = project.ok_or_else(|| {
        PlaneError::validation_with_hint(
            "Project is required for this command.",
            "Use -p/--project <name-or-id> to specify the project.",
        )
    })?;
    resolve_project(project, client).await
}

/// Parse one `--step "action | expected result"` value.
fn parse_step(raw: &str) -> Result<TestCaseStepWrite, PlaneError> {
    let Some((action, expected)) = raw.split_once('|') else {
        return Err(PlaneError::validation_with_hint(
            format!("A step must be written as \"action | expected result\" (got '{raw}')."),
            "Use --step \"Open the page | The list is shown\".",
        ));
    };
    let action = action.trim();
    let expected = expected.trim();
    if action.is_empty() || expected.is_empty() {
        return Err(PlaneError::validation_with_hint(
            "A step needs both an action and an expected result.",
            "Use --step \"Open the page | The list is shown\".",
        ));
    }
    Ok(TestCaseStepWrite {
        action: action.to_string(),
        expected_result: expected.to_string(),
    })
}

/// Read the step table out of a file.
///
/// The file is either a JSON array of steps (`{"action": …, "expected": …}`)
/// or one `action | expected result` per line; a line that starts with `#`
/// is a comment and an empty line is skipped.
fn read_steps_file(path: &std::path::Path) -> Result<Vec<TestCaseStepWrite>, PlaneError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        PlaneError::validation_with_hint(
            format!("Cannot read the steps file {}: {e}", path.display()),
            "Check the path of --steps-file.",
        )
    })?;
    parse_steps_document(&text)
}

/// Parse a step table out of a document.
fn parse_steps_document(text: &str) -> Result<Vec<TestCaseStepWrite>, PlaneError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        return parse_steps_json(trimmed);
    }
    parse_steps_lines(text)
}

/// Parse a step table written as a JSON array.
fn parse_steps_json(text: &str) -> Result<Vec<TestCaseStepWrite>, PlaneError> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        PlaneError::validation_with_hint(
            format!("The steps file is not valid JSON: {e}"),
            "A JSON steps file is an array of {\"action\": \"...\", \"expected\": \"...\"}.",
        )
    })?;
    let items = value.as_array().ok_or_else(|| {
        PlaneError::validation_with_hint(
            "A JSON steps file must be an array of steps.",
            "A JSON steps file is an array of {\"action\": \"...\", \"expected\": \"...\"}.",
        )
    })?;
    items
        .iter()
        .map(|item| {
            let action = item
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            let expected = item
                .get("expected")
                .or_else(|| item.get("expected_result"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if action.is_empty() || expected.is_empty() {
                return Err(PlaneError::validation_with_hint(
                    "A step of the JSON steps file needs an \"action\" and an \"expected\".",
                    "Write the step as {\"action\": \"...\", \"expected\": \"...\"}.",
                ));
            }
            Ok(TestCaseStepWrite {
                action,
                expected_result: expected,
            })
        })
        .collect()
}

/// Parse a step table written as one step per line.
fn parse_steps_lines(text: &str) -> Result<Vec<TestCaseStepWrite>, PlaneError> {
    let mut steps = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let step = parse_step(line).map_err(|e| {
            PlaneError::validation_with_hint(
                format!("Line {} of the steps file: {}", index + 1, e),
                "Write one step per line as \"action | expected result\".",
            )
        })?;
        steps.push(step);
    }
    Ok(steps)
}

/// The step table of a write: the inline steps and the steps file, if any.
///
/// The two sources may not be mixed — a table that is half from the command
/// line and half from a file is a table nobody can predict.
fn collect_steps(
    inline: &[String],
    steps_file: Option<&std::path::Path>,
) -> Result<Option<Vec<TestCaseStepWrite>>, PlaneError> {
    if !inline.is_empty() && steps_file.is_some() {
        return Err(PlaneError::validation_with_hint(
            "--step and --steps-file cannot be used together.",
            "Pass the steps inline or read them from a file, not both.",
        ));
    }
    if let Some(path) = steps_file {
        return Ok(Some(read_steps_file(path)?));
    }
    if inline.is_empty() {
        return Ok(None);
    }
    let steps = inline
        .iter()
        .map(|raw| parse_step(raw))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(steps))
}

/// The write body of a case, built from the flags the user gave.
fn case_write(opts: &TcCaseWrite<'_>, steps: Option<Vec<TestCaseStepWrite>>) -> TestCaseWrite {
    TestCaseWrite {
        module: opts.module.map(str::to_string),
        title: opts.title.map(str::to_string),
        purpose: opts.purpose.map(str::to_string),
        object_overview: opts.object_overview.map(str::to_string),
        priority: opts.priority.map(str::to_string),
        preconditions: opts.precondition.map(str::to_string),
        environment: opts.environment.map(str::to_string),
        related_feature: opts.related_feature.map(str::to_string),
        remarks: opts.remarks.map(str::to_string),
        status: opts.status.map(str::to_string),
        steps,
    }
}

/// Resolve a work item of the project to its uuid — the endpoint of the
/// links takes the uuid, not the identifier.
async fn resolve_issue_id(
    client: &PlaneClient,
    project: &Project,
    query: &str,
) -> Result<String, PlaneError> {
    let located = locate_work_item_in_project(query, project, client).await?;
    Ok(located.item.id)
}

#[allow(clippy::too_many_arguments)]
async fn cmd_tc_list(
    client: &PlaneClient,
    project: Option<&str>,
    module: Option<&str>,
    priority: Option<&str>,
    issue: Option<&str>,
    status: Option<&str>,
    search: Option<&str>,
    limit: usize,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let issue_id = match issue {
        Some(query) => Some(resolve_issue_id(client, &project, query).await?),
        None => None,
    };
    let filters = TestCaseFilters {
        module: module.map(str::to_string),
        priority: priority.map(str::to_string),
        issue: issue_id,
        status: status.map(str::to_string),
        search: search.map(str::to_string),
    };
    let mut cases = client.list_test_cases(&project.id, &filters).await?;
    cases.truncate(limit);
    if json {
        let views: Vec<Value> = cases.iter().map(test_case_view).collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = cases
            .iter()
            .map(|case| {
                vec![
                    case.number.clone(),
                    case.title.clone(),
                    case.priority.clone(),
                    case.module_code.clone(),
                    case.status.clone(),
                    case.steps.len().to_string(),
                ]
            })
            .collect();
        output_table(
            &["Number", "Title", "Priority", "Module", "Status", "Steps"],
            &rows,
        );
    }
    Ok(())
}

async fn cmd_tc_show(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    if json {
        output_json(&test_case_view(&found));
        return Ok(());
    }
    let rows = vec![
        vec!["number".to_string(), found.number.clone()],
        vec!["id".to_string(), found.id.clone()],
        vec!["module".to_string(), found.module_code.clone()],
        vec!["title".to_string(), found.title.clone()],
        vec!["purpose".to_string(), found.purpose.clone()],
        vec!["object_overview".to_string(), found.object_overview.clone()],
        vec!["priority".to_string(), found.priority.clone()],
        vec!["preconditions".to_string(), found.preconditions.clone()],
        vec!["environment".to_string(), found.environment.clone()],
        vec!["related_feature".to_string(), found.related_feature.clone()],
        vec!["remarks".to_string(), found.remarks.clone()],
        vec!["status".to_string(), found.status.clone()],
        vec!["created_at".to_string(), found.created_at.clone()],
        vec!["updated_at".to_string(), found.updated_at.clone()],
        vec![
            "created_by".to_string(),
            found
                .created_by
                .as_ref()
                .map(|user| user.label())
                .unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);

    let step_rows: Vec<Vec<String>> = found
        .steps
        .iter()
        .map(|step| {
            vec![
                step.sequence_id.to_string(),
                step.action.clone(),
                step.expected_result.clone(),
            ]
        })
        .collect();
    output_table(&["Step", "Action", "Expected result"], &step_rows);

    let issue_rows: Vec<Vec<String>> = found
        .issue_links
        .iter()
        .map(|link| {
            vec![
                link.issue_identifier.clone().unwrap_or_default(),
                link.issue_name.clone().unwrap_or_default(),
                link.issue_id.clone(),
            ]
        })
        .collect();
    output_table(&["Work item", "Title", "Issue ID"], &issue_rows);
    Ok(())
}

async fn cmd_tc_create(
    client: &PlaneClient,
    project: Option<&str>,
    opts: &TcCaseWrite<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let steps = collect_steps(opts.step, opts.steps_file)?;
    let project = require_project(client, project).await?;
    // The write carries the uuid of the module; the code is what the user
    // types, so it is resolved first.
    let module =
        resolve_test_case_module(client, &project.id, opts.module.unwrap_or_default()).await?;
    let mut write = case_write(opts, steps);
    write.module = Some(module.id);
    let created = client.create_test_case(&project.id, &write).await?;
    output_case_written(&created, "Created", json);
    Ok(())
}

async fn cmd_tc_update(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
    opts: &TcCaseWrite<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let steps = collect_steps(opts.step, opts.steps_file)?;
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    let write = case_write(opts, steps);
    let updated = client
        .update_test_case(&project.id, &found.id, &write)
        .await?;
    output_case_written(&updated, "Updated", json);
    Ok(())
}

/// Print the outcome of a write: the number the server assigned.
fn output_case_written(case: &TestCase, verb: &str, json: bool) {
    if json {
        output_json(&test_case_view(case));
        return;
    }
    eprintln!("{verb} {}.", case.number);
}

async fn cmd_tc_steps(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
    inline: &[String],
    steps_file: Option<&std::path::Path>,
    json: bool,
) -> Result<(), PlaneError> {
    let steps = collect_steps(inline, steps_file)?;
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    let Some(steps) = steps else {
        // No step given: the table is read.
        let table = client.list_test_case_steps(&project.id, &found.id).await?;
        if json {
            let views: Vec<Value> = table.iter().map(test_case_step_view).collect();
            output_json(&views);
        } else {
            let rows: Vec<Vec<String>> = table
                .iter()
                .map(|step| {
                    vec![
                        step.sequence_id.to_string(),
                        step.action.clone(),
                        step.expected_result.clone(),
                    ]
                })
                .collect();
            output_table(&["Step", "Action", "Expected result"], &rows);
        }
        return Ok(());
    };
    // A step was given: the whole table is replaced.
    let body = TestCaseStepsWrite { steps };
    let result = client
        .replace_test_case_steps(&project.id, &found.id, &body)
        .await?;
    if json {
        let views: Vec<Value> = result.steps.iter().map(test_case_step_view).collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = result
            .steps
            .iter()
            .map(|step| {
                vec![
                    step.sequence_id.to_string(),
                    step.action.clone(),
                    step.expected_result.clone(),
                ]
            })
            .collect();
        output_table(&["Step", "Action", "Expected result"], &rows);
    }
    Ok(())
}

async fn cmd_tc_issues(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
    add: Option<&str>,
    remove: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    if let Some(query) = add {
        let issue_id = resolve_issue_id(client, &project, query).await?;
        let link = client
            .add_test_case_issue(&project.id, &found.id, &issue_id)
            .await?;
        if json {
            output_json(&crate::render::test_case_issue_view(&link));
        } else {
            eprintln!(
                "Linked {} to {}.",
                link.issue_identifier.clone().unwrap_or(issue_id),
                found.number
            );
        }
        return Ok(());
    }
    if let Some(query) = remove {
        let issue_id = resolve_issue_id(client, &project, query).await?;
        client
            .remove_test_case_issue(&project.id, &found.id, &issue_id)
            .await?;
        if json {
            output_json(&serde_json::json!({
                "number": found.number,
                "issue_id": issue_id,
                "removed": true,
            }));
        } else {
            eprintln!("Unlinked {issue_id} from {}.", found.number);
        }
        return Ok(());
    }
    let links = client.list_test_case_issues(&project.id, &found.id).await?;
    if json {
        let views: Vec<Value> = links
            .iter()
            .map(crate::render::test_case_issue_view)
            .collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = links
            .iter()
            .map(|link| {
                vec![
                    link.issue_identifier.clone().unwrap_or_default(),
                    link.issue_name.clone().unwrap_or_default(),
                    link.issue_id.clone(),
                ]
            })
            .collect();
        output_table(&["Work item", "Title", "Issue ID"], &rows);
    }
    Ok(())
}

async fn cmd_tc_delete(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    client.delete_test_case(&project.id, &found.id).await?;
    eprintln!("Deleted {}.", found.number);
    Ok(())
}

async fn cmd_tc_module_list(
    client: &PlaneClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let modules = client.list_test_case_modules(&project.id).await?;
    if json {
        let views: Vec<Value> = modules.iter().map(test_case_module_view).collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = modules
            .iter()
            .map(|module| {
                vec![
                    module.code.clone(),
                    module.description.clone(),
                    module.id.clone(),
                ]
            })
            .collect();
        output_table(&["Code", "Description", "ID"], &rows);
    }
    Ok(())
}

async fn cmd_tc_module_create(
    client: &PlaneClient,
    project: Option<&str>,
    code: &str,
    description: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    // The code goes to the server as it was typed: the server owns the rules
    // of a code (its format, its uniqueness, its case) and answers a
    // refused code with its own code and its own sentence.
    let body = TestCaseModuleWrite {
        code: Some(code.to_string()),
        description: description.map(str::to_string),
    };
    let created = client.create_test_case_module(&project.id, &body).await?;
    if json {
        output_json(&test_case_module_view(&created));
    } else {
        eprintln!("Registered module code {}.", created.code);
    }
    Ok(())
}

/// The step table of a case, as the read command answers it.
#[allow(dead_code)]
fn step_rows(steps: &[TestCaseStep]) -> Vec<Vec<String>> {
    steps
        .iter()
        .map(|step| {
            vec![
                step.sequence_id.to_string(),
                step.action.clone(),
                step.expected_result.clone(),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_split_on_the_first_pipe() {
        let step = parse_step("Open the page | The list is shown").unwrap();
        assert_eq!(step.action, "Open the page");
        assert_eq!(step.expected_result, "The list is shown");
    }

    #[test]
    fn a_step_keeps_a_pipe_inside_the_expected_result() {
        let step = parse_step("Run the build | The output says OK | done").unwrap();
        assert_eq!(step.action, "Run the build");
        assert_eq!(step.expected_result, "The output says OK | done");
    }

    #[test]
    fn a_step_without_a_pipe_is_refused() {
        let err = parse_step("Open the page").unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("action | expected result"));
    }

    #[test]
    fn a_step_without_an_expected_result_is_refused() {
        assert!(parse_step("Open the page |").is_err());
        assert!(parse_step(" | The list is shown").is_err());
    }

    #[test]
    fn a_json_steps_file_is_read() {
        let steps = parse_steps_document(
            r#"[{"action": "Open the page", "expected": "The list is shown"},
                {"action": "Click it", "expected_result": "The row opens"}]"#,
        )
        .unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].action, "Open the page");
        assert_eq!(steps[0].expected_result, "The list is shown");
        assert_eq!(steps[1].expected_result, "The row opens");
    }

    #[test]
    fn a_text_steps_file_is_read_and_comments_are_skipped() {
        let steps = parse_steps_document(
            "# the login path\nOpen the page | The list is shown\n\n  Click it | The row opens  \n",
        )
        .unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].action, "Open the page");
        assert_eq!(steps[1].action, "Click it");
        assert_eq!(steps[1].expected_result, "The row opens");
    }

    #[test]
    fn an_empty_steps_file_is_an_empty_table() {
        assert!(parse_steps_document("   \n\n").unwrap().is_empty());
    }

    #[test]
    fn a_broken_json_steps_file_names_the_problem() {
        let err = parse_steps_document("[{\"action\": ").unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("not valid JSON"));
    }

    #[test]
    fn a_json_step_without_an_expected_result_is_refused() {
        let err = parse_steps_document(r#"[{"action": "Open the page"}]"#).unwrap_err();
        assert!(err.to_string().contains("expected"));
    }

    #[test]
    fn the_inline_steps_and_the_steps_file_are_not_mixed() {
        // The file exists and holds a valid step table, so the only thing
        // that can refuse the call is the rule that the two sources are not
        // mixed.
        let dir = std::env::temp_dir().join(format!("pbotcli_tc_mix_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("steps.json");
        std::fs::write(&path, "[{\"action\": \"A\", \"expected\": \"B\"}]").expect("write");
        let inline = vec!["Open the page | The list is shown".to_string()];
        let err = collect_steps(&inline, Some(&path)).unwrap_err();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(
            err.to_string().contains("--step and --steps-file"),
            "the refusal must name the two flags: {err}"
        );
    }
}
