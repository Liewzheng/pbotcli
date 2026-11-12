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
    TestCase, TestCaseFilters, TestCaseIssueLink, TestCaseModuleWrite, TestCaseStep,
    TestCaseStepWrite, TestCaseStepsWrite, TestCaseWrite,
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
        /// Maximum results to show (default 50).
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
    ///
    /// A case needs at least one step: pass --step "action | expected
    /// result" (or a --steps-file that holds one). The server refuses a
    /// case without a title, a purpose, an object overview, the
    /// preconditions, the environment or a priority.
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
        /// What the case checks. The server refuses a case without it.
        #[arg(long)]
        purpose: String,
        /// What is under test. The server refuses a case without it.
        #[arg(long)]
        object_overview: String,
        /// Priority: P0, P1, P2, P3. The server refuses a case without one.
        #[arg(long)]
        priority: String,
        /// What has to hold before the case runs. The server refuses a case
        /// without it.
        #[arg(long, visible_alias = "preconditions")]
        precondition: String,
        /// The environment and the version under test. The server refuses a
        /// case without it.
        #[arg(long, visible_alias = "env")]
        environment: String,
        /// The feature the case belongs to.
        #[arg(long)]
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
        #[arg(long)]
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
        #[arg(long)]
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
        /// Refused: the module of a case cannot be changed (the number is
        /// bound to it). The flag exists so the refusal names the field the
        /// caller tried to change.
        #[arg(long)]
        module: Option<String>,
        /// Refused: the number of a case is assigned by the server and is
        /// part of the record of every run.
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
            let opts = TcCaseCreate {
                module: &module,
                title: &title,
                purpose: &purpose,
                object_overview: &object_overview,
                priority: &priority,
                precondition: &precondition,
                environment: &environment,
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
            let opts = TcCaseUpdate {
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
        TcCmd::Delete { case, project } => {
            cmd_tc_delete(client, &case, project.as_deref(), json).await
        }
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

/// The fields of a case create, gathered so the handler signatures stay
/// small.
///
/// Every one of the header fields is mandatory: the server refuses a case
/// without a title, a purpose, an object overview, the preconditions, the
/// environment and a priority, so the CLI asks for them too and its
/// `--help` tells the truth.
struct TcCaseCreate<'a> {
    module: &'a str,
    title: &'a str,
    purpose: &'a str,
    object_overview: &'a str,
    priority: &'a str,
    precondition: &'a str,
    environment: &'a str,
    related_feature: Option<&'a str>,
    remarks: Option<&'a str>,
    status: Option<&'a str>,
    step: &'a [String],
    steps_file: Option<&'a std::path::Path>,
}

/// The fields of a case update, gathered so the handler signatures stay
/// small.
///
/// Every one of them is optional: the server keeps the value the case
/// carries for the fields that are not sent.
struct TcCaseUpdate<'a> {
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
    if trimmed.starts_with('{') {
        return Err(PlaneError::validation_with_hint(
            "A JSON steps file must be an array of steps.",
            "Write it as [{\"action\": \"...\", \"expected\": \"...\"}].",
        ));
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
            if action.is_empty() {
                return Err(PlaneError::validation_with_hint(
                    "The \"action\" of a step of the JSON steps file is missing or not a string.",
                    "Write the step as {\"action\": \"...\", \"expected\": \"...\"}.",
                ));
            }
            if expected.is_empty() {
                return Err(PlaneError::validation_with_hint(
                    "The \"expected\" of a step of the JSON steps file is missing or not a string.",
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

/// The write body of a create, built from the flags the user gave.
fn create_write(
    opts: &TcCaseCreate<'_>,
    steps: Vec<TestCaseStepWrite>,
) -> Result<TestCaseWrite, PlaneError> {
    Ok(TestCaseWrite {
        module: Some(opts.module.to_string()),
        title: Some(opts.title.to_string()),
        purpose: Some(opts.purpose.to_string()),
        object_overview: Some(opts.object_overview.to_string()),
        priority: Some(normalize_case_priority(opts.priority)?),
        preconditions: Some(opts.precondition.to_string()),
        environment: Some(opts.environment.to_string()),
        related_feature: opts.related_feature.map(str::to_string),
        remarks: opts.remarks.map(str::to_string),
        status: normalize_case_status(opts.status)?,
        steps: Some(steps),
    })
}

/// The write body of an update, built from the flags the user gave.
fn update_write(
    opts: &TcCaseUpdate<'_>,
    steps: Option<Vec<TestCaseStepWrite>>,
) -> Result<TestCaseWrite, PlaneError> {
    Ok(TestCaseWrite {
        module: None,
        title: opts.title.map(str::to_string),
        purpose: opts.purpose.map(str::to_string),
        object_overview: opts.object_overview.map(str::to_string),
        priority: match opts.priority {
            Some(priority) => Some(normalize_case_priority(priority)?),
            None => None,
        },
        preconditions: opts.precondition.map(str::to_string),
        environment: opts.environment.map(str::to_string),
        related_feature: opts.related_feature.map(str::to_string),
        remarks: opts.remarks.map(str::to_string),
        status: normalize_case_status(opts.status)?,
        steps,
    })
}

/// The priorities the server accepts (P0, P1, P2, P3).
const CASE_PRIORITIES: [&str; 4] = ["P0", "P1", "P2", "P3"];

/// The statuses the server accepts (active, void).
const CASE_STATUSES: [&str; 2] = ["active", "void"];

/// Normalize a `--priority` flag. The server accepts P0 to P3 and stores
/// them uppercased.
fn normalize_case_priority(value: &str) -> Result<String, PlaneError> {
    let upper = value.trim().to_uppercase();
    if CASE_PRIORITIES.contains(&upper.as_str()) {
        return Ok(upper);
    }
    Err(PlaneError::validation_with_hint(
        format!("Invalid priority '{value}'."),
        format!("Valid values: {}.", CASE_PRIORITIES.join(", ")),
    ))
}

/// Normalize a `--status` flag. The server accepts `active` and `void` and
/// stores them lowercased.
fn normalize_case_status(value: Option<&str>) -> Result<Option<String>, PlaneError> {
    let Some(value) = value else { return Ok(None) };
    let lower = value.trim().to_lowercase();
    if CASE_STATUSES.contains(&lower.as_str()) {
        return Ok(Some(lower));
    }
    Err(PlaneError::validation_with_hint(
        format!("Invalid status '{value}'."),
        format!("Valid values: {}.", CASE_STATUSES.join(", ")),
    ))
}

/// The step table of a create: the server refuses a case without a step, so
/// the CLI refuses it before the request.
fn require_steps(
    steps: Option<Vec<TestCaseStepWrite>>,
) -> Result<Vec<TestCaseStepWrite>, PlaneError> {
    let hint = "Pass --step \"action | expected result\" (or a --steps-file that holds one).";
    let Some(steps) = steps else {
        return Err(PlaneError::validation_with_hint(
            "A test case needs at least one step.",
            hint,
        ));
    };
    if steps.is_empty() {
        return Err(PlaneError::validation_with_hint(
            "A test case needs at least one step.",
            hint,
        ));
    }
    Ok(steps)
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
    if limit == 0 {
        return Err(PlaneError::validation_with_hint(
            "A limit of zero shows nothing.",
            "Pass -l/--limit 1 or more (the default is 50).",
        ));
    }
    let project = require_project(client, project).await?;
    let issue_id = match issue {
        Some(query) => Some(resolve_issue_id(client, &project, query).await?),
        None => None,
    };
    // The server stores the priority uppercased and the status lowercased,
    // and it compares the status filter literally — `--status ALL` would
    // match no case at all. The filters are normalized here, so a filter
    // the caller wrote in another case still finds its rows.
    let filters = TestCaseFilters {
        module: module.map(str::to_string),
        priority: priority.map(|value| value.trim().to_uppercase()),
        issue: issue_id,
        status: status.map(|value| value.trim().to_lowercase()),
        search: search.map(str::to_string),
    };
    let mut cases = client.list_test_cases(&project.id, &filters).await?;
    let total = cases.len();
    cases.truncate(limit);
    // A list that hides rows without saying so is the one place where the
    // output and the reality diverge.
    if cases.len() < total {
        eprintln!("Showing {} of {} test cases.", cases.len(), total);
    }
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
                .map(|user| user.full_name())
                .unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);

    output_table(
        &["Step", "Action", "Expected result"],
        &step_rows(&found.steps),
    );
    output_table(
        &["Work item", "Title", "Issue ID"],
        &issue_rows(&found.issue_links),
    );
    Ok(())
}

async fn cmd_tc_create(
    client: &PlaneClient,
    project: Option<&str>,
    opts: &TcCaseCreate<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let steps = require_steps(collect_steps(opts.step, opts.steps_file)?)?;
    // The body is built before the project is resolved, so a priority the
    // server would refuse is refused here as well.
    let mut write = create_write(opts, steps)?;
    let project = require_project(client, project).await?;
    // The write carries the uuid of the module; the code is what the user
    // types, so it is resolved first.
    let module = resolve_test_case_module(client, &project.id, opts.module).await?;
    write.module = Some(module.id);
    let created = client.create_test_case(&project.id, &write).await?;
    output_case_written(&created, "Created", json);
    Ok(())
}

async fn cmd_tc_update(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
    opts: &TcCaseUpdate<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let steps = collect_steps(opts.step, opts.steps_file)?;
    // A step table without a row is refused, exactly as `tc steps` refuses
    // it: the server would refuse it too (`step_count_zero`), and a
    // replacement that empties the table is never what the caller meant.
    if let Some(steps) = &steps
        && steps.is_empty()
    {
        return Err(PlaneError::validation_with_hint(
            "A step table needs at least one step.",
            "Pass --step \"action | expected result\" (or a --steps-file that holds one).",
        ));
    }
    let write = update_write(opts, steps)?;
    // An update without a field would be a write of nothing: the server
    // would answer 200 and the record would stay as it is. The refusal
    // happens before any request.
    if write.is_empty() {
        return Err(PlaneError::validation_with_hint(
            "Nothing to update.",
            "Pass at least one field, for example --title or --step.",
        ));
    }
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    // A step table is replaced as a whole, so the rows that are not in the
    // new table are dropped — the same notice the `tc steps` command
    // gives, so no path replaces a table silently.
    if let Some(steps) = &write.steps {
        announce_step_replacement(client, &project.id, &found, steps.len()).await?;
    }
    let updated = client
        .update_test_case(&project.id, &found.id, &write)
        .await?;
    output_case_written(&updated, "Updated", json);
    Ok(())
}

/// Say how many rows a replacement of the step table drops.
///
/// The table is replaced as a whole, so a shorter table drops rows. The
/// replacement is documented, not prompted (ADR-0008) — but it is never
/// silent: the row count of the table is said out loud before the write.
async fn announce_step_replacement(
    client: &PlaneClient,
    project_id: &str,
    case: &TestCase,
    new_count: usize,
) -> Result<(), PlaneError> {
    let current = client.list_test_case_steps(project_id, &case.id).await?;
    // The operation always replaces the whole table — even when the new
    // table has as many rows as the old one, the rows are new. So the
    // replacement is always said out loud.
    eprintln!(
        "Replacing the {} step(s) of {} with {} step(s).",
        current.len(),
        case.number,
        new_count
    );
    Ok(())
}

/// The rows of a step table: the number of the step, its action and its
/// expected result.
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

/// The rows of the work-item links of a case: the identifier of the work
/// item, its title and its uuid.
fn issue_rows(links: &[TestCaseIssueLink]) -> Vec<Vec<String>> {
    links
        .iter()
        .map(|link| {
            vec![
                link.issue_identifier.clone().unwrap_or_default(),
                link.issue_name.clone().unwrap_or_default(),
                link.issue_id.clone(),
            ]
        })
        .collect()
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
            output_table(&["Step", "Action", "Expected result"], &step_rows(&table));
        }
        return Ok(());
    };
    // A step was given: the whole table is replaced.
    if steps.is_empty() {
        return Err(PlaneError::validation_with_hint(
            "A step table needs at least one step.",
            "Pass --step \"action | expected result\" (or a --steps-file that holds one).",
        ));
    }
    // The table is replaced as a whole, so a shorter table drops rows.
    announce_step_replacement(client, &project.id, &found, steps.len()).await?;
    let body = TestCaseStepsWrite { steps };
    let result = client
        .replace_test_case_steps(&project.id, &found.id, &body)
        .await?;
    if json {
        let views: Vec<Value> = result.steps.iter().map(test_case_step_view).collect();
        output_json(&views);
    } else {
        output_table(
            &["Step", "Action", "Expected result"],
            &step_rows(&result.steps),
        );
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
                "issue": query,
                "issue_id": issue_id,
                "removed": true,
            }));
        } else {
            eprintln!("Unlinked {query} from {}.", found.number);
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
        output_table(&["Work item", "Title", "Issue ID"], &issue_rows(&links));
    }
    Ok(())
}

async fn cmd_tc_delete(
    client: &PlaneClient,
    case: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_case(client, &project.id, case).await?;
    client.delete_test_case(&project.id, &found.id).await?;
    if json {
        output_json(&serde_json::json!({
            "number": found.number,
            "id": found.id,
            "deleted": true,
        }));
    } else {
        eprintln!("Deleted {}.", found.number);
    }
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
    fn a_json_step_with_a_field_that_is_not_a_string_is_refused() {
        let err = parse_steps_document(r#"[{"action": 7, "expected": "The list"}]"#).unwrap_err();
        assert!(err.to_string().contains("action"), "{err}");
        let err = parse_steps_document(r#"[{"action": "Open", "expected": 7}]"#).unwrap_err();
        assert!(err.to_string().contains("expected"), "{err}");
    }

    #[test]
    fn a_json_object_is_not_a_step_table() {
        // The step table is a list; a lone object is a mistake that the
        // error has to name, not a line of text to be read as a step.
        let err =
            parse_steps_document(r#"{"action": "Open", "expected": "The list"}"#).unwrap_err();
        assert!(err.to_string().contains("array of steps"), "{err}");
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
