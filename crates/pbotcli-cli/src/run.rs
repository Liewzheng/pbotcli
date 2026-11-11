//! The `run` command group: the batches of a project.
//!
//! A batch is one continuous execution of test cases. It is named by its
//! number (`RUN-<YYYYMMDD>-<NN>`), and the server assigns that number —
//! the CLI never builds one, and never writes one.
//!
//! Two rules of the feature shape this group:
//!
//! * a verdict (the ✔ / ✘ of a step) and a conclusion (the verdict of a
//!   case) are written **once**: the server refuses a second write with
//!   `conclusion_is_final`, and a wrong one is corrected by voiding the
//!   record and running the case again in a new batch. So both writes ask
//!   before they write, and no flag of this group overwrites a verdict;
//! * a **completed** batch is frozen: every write to it is refused with
//!   `run_completed`, and the CLI never retries such a refusal on its
//!   own.
//!
//! Every read command answers `--json` on stdout and a table on stderr,
//! and a refused write prints the **stable code** the server answered
//! with (`conclusion_is_final`, `duplicate_case_number`, …) together with
//! the sentence of the server.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use clap::Subcommand;
use pbotcli_client::PlaneClient;
use pbotcli_client::asset::{ENTITY_TYPE_CASE_EVIDENCE, ENTITY_TYPE_STEP_RECORD};
use pbotcli_client::tcerror::carries_code;
use pbotcli_core::PlaneError;
use pbotcli_format::{output_json, output_table};
use pbotcli_resolve::testcase::resolve_test_case;
use pbotcli_resolve::testrun::{find_run_case, resolve_test_run};
use pbotcli_resolve::{resolve_project, resolve_user_query};
use pbotcli_types::Project;
use pbotcli_types::testrun::{
    RUN_CASE_CONCLUSIONS, STEP_VERDICTS, TestRun, TestRunCase, TestRunCaseWrite, TestRunFilters,
    TestRunStepWrite, TestRunWrite,
};
use regex::Regex;
use serde_json::Value;

use crate::render::{
    test_run_case_view, test_run_summary_unavailable_view, test_run_summary_view, test_run_view,
};

/// The subcommands of `pbot run`.
#[derive(Subcommand)]
pub enum RunCmd {
    /// List the batches of a project.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Filter by status: draft, in_progress, completed, or all.
        #[arg(long)]
        status: Option<String>,
        /// Filter by the day of the execution (YYYY-MM-DD).
        #[arg(long = "run-date")]
        run_date: Option<String>,
        /// Maximum results to show (default 50).
        #[arg(long, short = 'l')]
        limit: Option<usize>,
    },
    /// Show one batch: its header, its cases, their steps and their files.
    Show {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a batch.
    ///
    /// The number of the batch is assigned by the server. The tested
    /// version and the environment are asked for, because the server
    /// refuses to close a batch that does not carry them — and this
    /// group has no command that fills them in later.
    Create {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// The version under test (frozen once the batch is created).
        #[arg(long)]
        version: String,
        /// The build under test (frozen once the batch is created).
        #[arg(long)]
        build: String,
        /// The commit under test (frozen once the batch is created).
        #[arg(long)]
        commit: String,
        /// The environment the batch ran in.
        #[arg(long, visible_alias = "env")]
        environment: String,
        /// The day of the execution (default: today, in UTC).
        #[arg(long = "run-date", value_name = "YYYY-MM-DD")]
        run_date: Option<String>,
        /// The batch this one re-tests (number or UUID). A re-test runs
        /// in the project of its original and never in a ring.
        #[arg(long = "retest-of", value_name = "RUN")]
        retest_of: Option<String>,
        /// Who executed the batch: me (the default), a name, or a UUID.
        #[arg(long = "executed-by", value_name = "WHO")]
        executed_by: Option<String>,
    },
    /// Add test cases to a batch.
    ///
    /// The server snapshots every case that is added, so a later edit of
    /// the case never rewrites what the batch recorded. A case that is
    /// already in the batch is refused with `duplicate_case_number`.
    Add {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// The cases to add (numbers or UUIDs).
        #[arg(required = true, value_name = "CASE")]
        case: Vec<String>,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Write the verdict and the measured value of one step.
    ///
    /// The verdict of a step is written once. The command asks before it
    /// writes, and a script confirms with --yes.
    ///
    /// A screenshot of a step does not satisfy the finalize: the
    /// evidence of a case hangs on the case itself.
    Step {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// The case of the batch (TC-CLI-007).
        #[arg(long)]
        case: String,
        /// The number of the step in the case (1, 2, 3 …).
        #[arg(long)]
        step: i64,
        /// The verdict of the step: pass, fail or unset.
        #[arg(long)]
        verdict: String,
        /// The measured value, as Markdown. It reaches the server as it
        /// was typed — no conversion, no rewriting.
        #[arg(long)]
        actual: Option<String>,
        /// A file with the measured value. Use it for a multi-line
        /// value: the file reaches the server byte for byte.
        #[arg(long = "actual-file", value_name = "PATH")]
        actual_file: Option<PathBuf>,
        /// A screenshot of the step.
        #[arg(long = "image", value_name = "PATH")]
        image: Option<PathBuf>,
        /// Confirm the write (needed when the command is not on a
        /// terminal).
        #[arg(long, short = 'y')]
        yes: bool,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Write the conclusion of one case of a batch.
    ///
    /// The conclusion of a case is written once. The command asks before
    /// it writes, and a script confirms with --yes.
    ///
    /// The evidence of the case hangs on the case: --evidence (or
    /// --evidence-file) and --image are the two ways to write it, and
    /// either one is enough.
    Conclude {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// The case of the batch (TC-CLI-007).
        #[arg(long)]
        case: String,
        /// The conclusion of the case: PASS, FAIL, N/A or MANUAL.
        #[arg(long)]
        conclusion: String,
        /// The evidence of the case, as Markdown.
        #[arg(long)]
        evidence: Option<String>,
        /// A file with the evidence of the case. Use it for a multi-line
        /// value: the file reaches the server byte for byte.
        #[arg(long = "evidence-file", value_name = "PATH")]
        evidence_file: Option<PathBuf>,
        /// A screenshot of the case — the evidence of a case on its own
        /// is enough.
        #[arg(long = "image", value_name = "PATH")]
        image: Option<PathBuf>,
        /// Free-form remarks.
        #[arg(long)]
        remarks: Option<String>,
        /// The one-line phenomenon of the case (the summary shows it).
        #[arg(long = "actual-result-summary")]
        actual_result_summary: Option<String>,
        /// Why the case does not apply (a N/A case needs it).
        #[arg(long = "na-reason")]
        na_reason: Option<String>,
        /// Where the criterion of a N/A case comes from.
        #[arg(long = "na-criteria-source")]
        na_criteria_source: Option<String>,
        /// Who ran the case by hand (a MANUAL case needs it).
        #[arg(long = "manual-executor")]
        manual_executor: Option<String>,
        /// Under which condition the case was run by hand.
        #[arg(long = "manual-condition")]
        manual_condition: Option<String>,
        /// Confirm the write (needed when the command is not on a
        /// terminal).
        #[arg(long, short = 'y')]
        yes: bool,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Take a case out of a batch.
    ///
    /// A record that already carries a verdict or a conclusion stays in
    /// the batch: the server refuses the removal with
    /// `run_case_removal_after_execution`.
    Remove {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// The case of the batch (TC-CLI-007).
        #[arg(long)]
        case: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Close a batch: the server runs the validity rules over it.
    ///
    /// A refused finalize lists every problem it found, one per line,
    /// each with the case and the step it belongs to. The batch stays
    /// open until every problem is fixed.
    Finalize {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// The summary of a batch: the four counts and the deviations.
    ///
    /// The counts are not a partition of the total — a case that failed
    /// on one step and was never executed on another is counted twice.
    Summary {
        /// Batch number (RUN-20261109-01) or UUID.
        run: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

/// The fields of a batch create, gathered so the handler signature stays
/// small.
struct RunCreate<'a> {
    version: &'a str,
    build: &'a str,
    commit: &'a str,
    environment: &'a str,
    run_date: Option<&'a str>,
    retest_of: Option<&'a str>,
    executed_by: Option<&'a str>,
}

/// Run a `run` subcommand.
pub async fn dispatch(client: &PlaneClient, command: RunCmd, json: bool) -> Result<(), PlaneError> {
    match command {
        RunCmd::List {
            project,
            status,
            run_date,
            limit,
        } => {
            cmd_run_list(
                client,
                project.as_deref(),
                status.as_deref(),
                run_date.as_deref(),
                limit.unwrap_or(50),
                json,
            )
            .await
        }
        RunCmd::Show { run, project } => cmd_run_show(client, &run, project.as_deref(), json).await,
        RunCmd::Create {
            project,
            version,
            build,
            commit,
            environment,
            run_date,
            retest_of,
            executed_by,
        } => {
            let opts = RunCreate {
                version: &version,
                build: &build,
                commit: &commit,
                environment: &environment,
                run_date: run_date.as_deref(),
                retest_of: retest_of.as_deref(),
                executed_by: executed_by.as_deref(),
            };
            cmd_run_create(client, project.as_deref(), &opts, json).await
        }
        RunCmd::Add { run, case, project } => {
            cmd_run_add(client, &run, &case, project.as_deref(), json).await
        }
        RunCmd::Step {
            run,
            case,
            step,
            verdict,
            actual,
            actual_file,
            image,
            yes,
            project,
        } => {
            cmd_run_step(
                client,
                &run,
                &case,
                step,
                &verdict,
                actual.as_deref(),
                actual_file.as_deref(),
                image.as_deref(),
                yes,
                project.as_deref(),
                json,
            )
            .await
        }
        RunCmd::Conclude {
            run,
            case,
            conclusion,
            evidence,
            evidence_file,
            image,
            remarks,
            actual_result_summary,
            na_reason,
            na_criteria_source,
            manual_executor,
            manual_condition,
            yes,
            project,
        } => {
            cmd_run_conclude(
                client,
                &run,
                &case,
                &conclusion,
                evidence.as_deref(),
                evidence_file.as_deref(),
                image.as_deref(),
                remarks.as_deref(),
                actual_result_summary.as_deref(),
                na_reason.as_deref(),
                na_criteria_source.as_deref(),
                manual_executor.as_deref(),
                manual_condition.as_deref(),
                yes,
                project.as_deref(),
                json,
            )
            .await
        }
        RunCmd::Remove { run, case, project } => {
            cmd_run_remove(client, &run, &case, project.as_deref(), json).await
        }
        RunCmd::Finalize { run, project } => {
            cmd_run_finalize(client, &run, project.as_deref(), json).await
        }
        RunCmd::Summary { run, project } => {
            cmd_run_summary(client, &run, project.as_deref(), json).await
        }
    }
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

/// The batch and the case of a write, with the confirmation of the write.
///
/// The refusal of a write that cannot be confirmed happens before any
/// request — so nothing is written. The question on a terminal is asked
/// after the read, so it can name the batch and the case as the server
/// knows them.
async fn resolve_and_confirm(
    client: &PlaneClient,
    project: Option<&str>,
    run: &str,
    case: &str,
    yes: bool,
    subject: &str,
) -> Result<(Project, TestRun, TestRunCase), PlaneError> {
    refuse_unconfirmable(yes, &format!("{subject} of {case} in {run}"))?;
    let project = require_project(client, project).await?;
    let found = resolve_test_run(client, &project.id, run).await?;
    let record = require_run_case(&found, case)?;
    if !yes {
        confirm_on_terminal(&format!(
            "{subject} of {} in {}",
            record.case_number, found.number
        ))?;
    }
    Ok((project, found, record))
}

/// The re-test marker of a batch.
///
/// The server keeps the number of the original batch next to the link,
/// so a caller can show it without a second call. The CLI speaks
/// English everywhere, so the marker is English as well — the label of
/// the web UI (「补测：RUN-…」) is not copied into the output.
fn retest_marker(run: &TestRun) -> String {
    match run
        .retest_of_number
        .as_deref()
        .filter(|number| !number.is_empty())
    {
        Some(number) => format!("retest of {number}"),
        None => String::new(),
    }
}

/// The case record of a batch, by its uuid or its number.
///
/// The batch holds the records of its cases, so the batch itself answers
/// which one the caller means.
fn require_run_case(found: &TestRun, query: &str) -> Result<TestRunCase, PlaneError> {
    find_run_case(found, query).ok_or_else(|| {
        PlaneError::not_found_with_hint(
            format!("Test run case not found: {query}"),
            format!(
                "Run `pbot run show {}` to see the cases of the batch.",
                found.number
            ),
        )
    })
}

/// The verdict of a step, in the vocabulary of the server.
///
/// The server accepts `pass`, `fail` and `unset` — the placeholder of a
/// step that was never executed. `N/A` is a conclusion of a case and
/// not a verdict of a step, so it is refused here with the sentence that
/// says so.
fn normalize_step_verdict(value: &str) -> Result<String, PlaneError> {
    let lower = value.trim().to_lowercase();
    if STEP_VERDICTS.contains(&lower.as_str()) {
        return Ok(lower);
    }
    Err(PlaneError::validation_with_hint(
        format!("Invalid step verdict '{value}'."),
        format!(
            "A step carries pass, fail or unset — a step that was not executed is unset. N/A is a conclusion of a case (`pbot run conclude --conclusion N/A`). Valid values: {}.",
            STEP_VERDICTS.join(", ")
        ),
    ))
}

/// The conclusion of a case, in the vocabulary of the server.
///
/// The server stores the conclusion uppercased, and it knows `N/A` —
/// the short form `NA` is accepted here and normalized to it.
fn normalize_conclusion(value: &str) -> Result<String, PlaneError> {
    let upper = value.trim().to_uppercase();
    let normalized = match upper.as_str() {
        "PASS" => "PASS",
        "FAIL" => "FAIL",
        "N/A" | "NA" => "N/A",
        "MANUAL" => "MANUAL",
        _ => {
            return Err(PlaneError::validation_with_hint(
                format!("Invalid conclusion '{value}'."),
                format!("Valid values: {}.", RUN_CASE_CONCLUSIONS.join(", ")),
            ));
        }
    };
    Ok(normalized.to_string())
}

/// The evidence of a record: the Markdown text and the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Evidence {
    /// Nothing was written.
    None,
    /// A text was written.
    Text,
    /// A file was attached.
    Attachment,
    /// Both were written.
    Both,
}

impl Evidence {
    /// True when the evidence satisfies the rule of the server.
    ///
    /// The rule is an **or**: the evidence of a record is a non-empty
    /// text **or** at least one attachment. A screenshot of the report
    /// is an evidence on its own.
    fn satisfies_finalize(self) -> bool {
        !matches!(self, Evidence::None)
    }
}

/// The shape of the evidence of a record.
///
/// The text that is already on the record counts as well — the evidence
/// may be written before the conclusion.
///
/// The rule is the rule of the server (`is_filled_markdown` of
/// `testcase_validity.py`): the value has to carry a text **of its own**.
/// A placeholder (`—`, `待补充`, `TBD`, …) carries none, and a value that
/// holds nothing but an image (`![](shot.png)`) carries none either — a
/// picture is a file, not a sentence. The value is read the way the
/// server reads it, and the table of
/// `the_evidence_rule_is_the_rule_of_the_server` pins the answer of the
/// server for the shapes that matter.
///
/// The residue: a shape of Markdown that neither of the two reads the
/// same would leave the notice wrong. The notice only ever warns, and
/// the server still refuses the batch and names the case.
fn evidence_shape(text: Option<&str>, has_file: bool) -> Evidence {
    let has_text = text.is_some_and(carries_text);
    match (has_text, has_file) {
        (true, true) => Evidence::Both,
        (true, false) => Evidence::Text,
        (false, true) => Evidence::Attachment,
        (false, false) => Evidence::None,
    }
}

/// The values the server reads as no text at all.
///
/// The comparison is on the **whole** value: `—` is a placeholder, while
/// `— (the device stayed silent)` is an observation.
const PLACEHOLDERS: [&str; 25] = [
    "-",
    "--",
    "---",
    "—",
    "–",
    ".",
    "..",
    "...",
    "…",
    "?",
    "??",
    "???",
    "*",
    "**",
    "_",
    "__",
    "tbd",
    "todo",
    "xxx",
    "待补充",
    "待填写",
    "待填",
    "待定",
    "暂填",
    "待确认",
];

/// True when the text of a value is nothing but a placeholder.
fn is_placeholder(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    !normalized.is_empty() && PLACEHOLDERS.contains(&normalized.as_str())
}

/// The text a Markdown value carries of its own.
///
/// The value is read the way the server reads it (the `markdown_text` of
/// `db/testcase_validity.py`): the content of a code span and of a code
/// block is a quotation — the server reads it as text and never looks
/// for a picture inside it — and the rest is markup: the pictures are
/// taken out, the inline HTML is dropped, the destination of a link is
/// dropped, and the emphasis of a text is not what the text says.
fn markdown_text(value: &str) -> String {
    let mut text = String::new();
    let mut rest = value;
    // The literal parts go first: a code span and a code block are
    // quotations, and a picture inside one is part of the quotation.
    while let Some(start) = rest.find('`') {
        text.push_str(&read_markup(&rest[..start]));
        let literal = &rest[start..];
        let ticks = literal.chars().take_while(|c| *c == '`').count();
        match closing_fence(&literal[ticks..], ticks) {
            Some(end) => {
                text.push_str(&literal[ticks..ticks + end]);
                rest = &literal[ticks + end + ticks..];
            }
            None => {
                // An unterminated code span is text of the parse, so the
                // pictures inside it are pictures of the text.
                text.push_str(&read_markup(literal));
                rest = "";
                break;
            }
        }
    }
    text.push_str(&read_markup(rest));
    text
}

/// The place where a code span ends.
///
/// A code span opened with `n` backticks ends at the next run of
/// exactly `n` backticks — a longer run is not its end (CommonMark).
fn closing_fence(text: &str, ticks: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'`' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index] == b'`' {
            index += 1;
        }
        if index - start == ticks {
            return Some(start);
        }
    }
    None
}

/// The text of a part that carries markup.
fn read_markup(part: &str) -> String {
    let without_pictures = strip_pictures(part);
    let without_html = strip_html(&without_pictures);
    let without_destinations = strip_destinations(&without_html);
    strip_marks(&without_destinations)
}

/// Take the pictures out of a part of markup.
///
/// The shapes are the ones of the server (`_LOOSE_IMAGE_RE`): an inline
/// picture that is closed — its destination may hold spaces and balanced
/// parentheses —, an inline picture that was never closed (the token
/// ends at the first blank, so the words behind it stay text) and a
/// reference-style picture. The shortcut form `![alt]` is literal text,
/// like on the server.
fn strip_pictures(part: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // The three shapes of `_LOOSE_IMAGE_RE` of the server, in one
        // pattern: a closed inline picture (the destination may hold
        // spaces and balanced parentheses), an inline picture that was
        // never closed (the token ends at the first blank) and a
        // reference-style picture.
        Regex::new(r"!\[[^\]\n]*\]\((?:[^()\n]|\([^()\n]*\))*\)|!\[[^\]\n]*\]\([^)\s\n]*|!\[[^\]\n]*\]\[[^\]\n]*\]").expect("the picture pattern of the server is valid")
    })
    .replace_all(part, " ")
    .into_owned()
}

/// Take the inline HTML out of a part of markup.
///
/// The server drops the HTML nodes of its parse: `<b>` and `</b>` are
/// markup, not text.
fn strip_html(part: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"</?[A-Za-z][^>\n]*>|<!--[^\n]*-->|<\?[^>\n]*\?>|<![^>\n]*>")
            .expect("the tag pattern is valid")
    })
    .replace_all(part, "")
    .into_owned()
}

/// Remove the destination of a link: `[text](url)` says `text`.
fn strip_destinations(value: &str) -> String {
    let mut text = String::new();
    let mut rest = value;
    while let Some(start) = rest.find("](") {
        text.push_str(&rest[..start + 1]);
        match rest[start + 1..].find(')') {
            Some(end) => rest = &rest[start + 2 + end..],
            None => {
                // An unterminated link is text, not a link.
                text.push_str(&rest[start + 1..]);
                rest = "";
                break;
            }
        }
    }
    text.push_str(rest);
    text
}

/// Remove the markup marks of a value — but only the ones that are
/// markup.
///
/// A mark that stands alone is text: `TBD*` says `TBD*`, while `**TBD**`
/// says `TBD`. So a mark is taken away only when the value holds a
/// second one of it.
fn strip_marks(text: &str) -> String {
    let stars = text.matches('*').count();
    let underscores = text.matches('_').count();
    let backticks = text.matches('`').count();
    text.chars()
        .filter(|c| match c {
            '*' => stars < 2,
            '_' => underscores < 2,
            '`' => backticks < 2,
            _ => true,
        })
        .collect()
}

/// True when the value carries a text of its own.
fn carries_text(value: &str) -> bool {
    let text = markdown_text(value);
    !text.trim().is_empty() && !is_placeholder(&text)
}

/// What the finalize rules still want for a conclusion.
///
/// The server runs the validity rules when the batch is closed: every
/// case needs the evidence, a N/A case needs the reason and the source
/// of its criterion, a MANUAL case needs the executor and the condition.
/// The CLI says what is missing while the tester can still write it.
fn missing_for_finalize(
    conclusion: &str,
    write: &TestRunCaseWrite,
    case: &TestRunCase,
    has_file: bool,
) -> Vec<&'static str> {
    let mut missing = Vec::new();
    let evidence = evidence_shape(
        write.evidence.as_deref().or(Some(case.evidence.as_str())),
        has_file || !case.attachments.is_empty(),
    );
    if !evidence.satisfies_finalize() {
        missing.push("evidence");
    }
    let filled = |written: Option<&String>, stored: &str| {
        written
            .map(|value| !value.trim().is_empty())
            .unwrap_or(!stored.trim().is_empty())
    };
    if conclusion == "N/A" {
        if !filled(write.na_reason.as_ref(), &case.na_reason) {
            missing.push("na_reason");
        }
        if !filled(write.na_criteria_source.as_ref(), &case.na_criteria_source) {
            missing.push("na_criteria_source");
        }
    }
    if conclusion == "MANUAL" {
        if !filled(write.manual_executor.as_ref(), &case.manual_executor) {
            missing.push("manual_executor");
        }
        if !filled(write.manual_condition.as_ref(), &case.manual_condition) {
            missing.push("manual_condition");
        }
    }
    missing
}

/// The Markdown value of a flag or of a file.
///
/// The content reaches the server as it was typed: no HTML conversion,
/// no rewriting of the markup, and no trimming — a file is read as it
/// is on the disk. A multi-line value belongs into the file.
fn collect_markdown(
    inline: Option<&str>,
    file: Option<&Path>,
    inline_flag: &str,
    file_flag: &str,
) -> Result<Option<String>, PlaneError> {
    if inline.is_some() && file.is_some() {
        return Err(PlaneError::validation_with_hint(
            format!("{inline_flag} and {file_flag} cannot be used together."),
            "Pass the text inline or read it from a file, not both.",
        ));
    }
    if let Some(path) = file {
        return Ok(Some(read_markdown_file(path, file_flag)?));
    }
    Ok(inline.map(str::to_string))
}

/// Read a Markdown value out of a file, byte for byte.
fn read_markdown_file(path: &Path, flag: &str) -> Result<String, PlaneError> {
    let bytes = std::fs::read(path).map_err(|e| {
        PlaneError::validation_with_hint(
            format!("Cannot read the {flag} file {}: {e}", path.display()),
            format!("Check the path of {flag}."),
        )
    })?;
    String::from_utf8(bytes).map_err(|_| {
        PlaneError::validation_with_hint(
            format!("The {flag} file {} is not valid UTF-8.", path.display()),
            format!("A Markdown value is text; pass {flag} with the text itself."),
        )
    })
}

/// The mime type of an image file, by its extension.
///
/// The server accepts images only as the evidence of a test record, so
/// the type is read from the name of the file.
fn image_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

/// Upload an image as the evidence of a record.
///
/// The file is bound to the record through the asset endpoint: the
/// upload *is* the bind, so the record has to exist before the file is
/// sent. The server accepts images only — a log belongs into the
/// Markdown field of the record.
async fn upload_image(
    client: &PlaneClient,
    project_id: &str,
    path: &Path,
    entity_type: &str,
    entity_identifier: &str,
) -> Result<pbotcli_client::asset::TestEvidence, PlaneError> {
    let mime = image_mime(path).ok_or_else(|| {
        PlaneError::validation_with_hint(
            format!(
                "'{}' is not a file type the server accepts as evidence.",
                path.display()
            ),
            "Pass an image (png, jpg, webp or gif); a log belongs into --evidence/--actual-file.",
        )
    })?;
    let bytes = std::fs::read(path).map_err(|e| {
        PlaneError::validation_with_hint(
            format!("Cannot read the image {}: {e}", path.display()),
            "Check the path of --image.",
        )
    })?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "evidence".to_string());
    client
        .upload_test_evidence(
            project_id,
            &name,
            mime,
            bytes,
            entity_type,
            entity_identifier,
        )
        .await
}

/// Refuse a write that cannot be confirmed.
///
/// The verdict of a step and the conclusion of a case are written once,
/// so the CLI asks before it writes — and it can only ask on a terminal.
/// A non-interactive caller has to pass `--yes`. The refusal happens
/// before any request, so nothing is written.
fn refuse_unconfirmable(yes: bool, subject: &str) -> Result<(), PlaneError> {
    if yes || std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(PlaneError::validation_with_hint(
        format!("{subject} needs a confirmation, and this command is not running on a terminal."),
        "Pass --yes to confirm the write, or run the command on a terminal to be asked.",
    ))
}

/// Ask the confirmation of a write on the terminal.
///
/// The prompt goes to stderr: stdout carries the JSON of `--json`.
fn confirm_on_terminal(subject: &str) -> Result<(), PlaneError> {
    confirm_with(subject, &mut std::io::stdin().lock())
}

/// The confirmation of a write, read from a reader.
///
/// The answer is one line: `y` (or `yes`) confirms the write, every
/// other answer — an empty one included — refuses it. The write is
/// never made when the answer is refused.
fn confirm_with(subject: &str, input: &mut impl std::io::BufRead) -> Result<(), PlaneError> {
    use std::io::Write;

    eprint!("{subject} — write it? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let _ = input.read_line(&mut line);
    if is_confirmation(&line) {
        return Ok(());
    }
    Err(PlaneError::validation_with_hint(
        "Aborted: nothing was written.",
        "Run the command again and answer y to confirm the write.",
    ))
}

/// True when the answer confirms the write.
fn is_confirmation(answer: &str) -> bool {
    matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Today, as the batch number counts it.
///
/// The server stamps the number of a batch with the day the batch
/// carries, and the day is the one the CLI sends — so the default is the
/// current date. The clock of the system is read in UTC: the workspace
/// has no timezone-aware clock, and a day that is off by one is
/// corrected with `--run-date`.
fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// The calendar date of a day since the epoch.
///
/// The days are counted from 1970-01-01; the date is computed with the
/// civil-from-days algorithm of Howard Hinnant.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    (year + i64::from(month <= 2), month, day)
}

/// The error of a case that is already in the batch.
///
/// The server refuses a duplicate with the stable code
/// `duplicate_case_number`, but its refusal names the field and not the
/// row. The CLI knows which case it tried to add — so the refusal names
/// it, and the code of the server stays visible.
fn duplicate_case_error(case_number: &str, run_number: &str, server: Option<&str>) -> PlaneError {
    let mut message = format!(
        "duplicate_case_number: the case {case_number} is already in the batch {run_number}."
    );
    if let Some(server) = server {
        message.push(' ');
        message.push_str(server);
    }
    PlaneError::validation_with_hint(
        message,
        "The case is already in this batch. Read the batch (`pbot run show <RUN>`) before adding it again.",
    )
}

/// The sentences of the server in a refusal.
///
/// The mapper prints the code of the refusal first and one line per
/// problem; the sentence of a problem is what follows its code.
fn server_sentences(err: &PlaneError) -> String {
    err.to_string()
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(": ").map(|(_, text)| text.trim()))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

async fn cmd_run_list(
    client: &PlaneClient,
    project: Option<&str>,
    status: Option<&str>,
    run_date: Option<&str>,
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
    // The server compares the status literally, so a filter written in
    // another case would match no batch at all.
    let filters = TestRunFilters {
        status: status.map(|value| value.trim().to_lowercase()),
        run_date: run_date.map(|value| value.trim().to_string()),
    };
    let mut runs = client.list_test_runs(&project.id, &filters).await?;
    let total = runs.len();
    runs.truncate(limit);
    if runs.len() < total {
        eprintln!("Showing {} of {} test runs.", runs.len(), total);
    }
    if json {
        let views: Vec<Value> = runs.iter().map(test_run_view).collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = runs
            .iter()
            .map(|run| {
                vec![
                    run.number.clone(),
                    run.run_date.clone(),
                    run.version_number.clone(),
                    run.build_number.clone(),
                    run.status.clone(),
                    run.executed_by_name.clone(),
                    run.cases.len().to_string(),
                    // A re-test names the batch it re-tests.
                    retest_marker(run),
                ]
            })
            .collect();
        output_table(
            &[
                "Number",
                "Date",
                "Version",
                "Build",
                "Status",
                "Executor",
                "Cases",
                "Retest of",
            ],
            &rows,
        );
    }
    Ok(())
}

async fn cmd_run_show(
    client: &PlaneClient,
    run: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_run(client, &project.id, run).await?;
    if json {
        output_json(&test_run_view(&found));
        return Ok(());
    }
    let rows = vec![
        vec!["number".to_string(), found.number.clone()],
        vec!["id".to_string(), found.id.clone()],
        vec!["run_date".to_string(), found.run_date.clone()],
        vec!["version_number".to_string(), found.version_number.clone()],
        vec!["build_number".to_string(), found.build_number.clone()],
        vec!["commit_hash".to_string(), found.commit_hash.clone()],
        vec!["environment".to_string(), found.environment.clone()],
        vec!["executed_by".to_string(), found.executed_by_name.clone()],
        vec!["status".to_string(), found.status.clone()],
        // The number of the batch this one re-tests — the original
        // keeps its number even when it is deleted — marked as the
        // re-test it is.
        vec!["retest_of".to_string(), retest_marker(&found)],
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

    let case_rows: Vec<Vec<String>> = found
        .cases
        .iter()
        .map(|case| {
            vec![
                case.case_number.clone(),
                case.conclusion.clone(),
                case.case_title.clone(),
                case.case_priority.clone(),
                case.evidence.clone(),
            ]
        })
        .collect();
    output_table(
        &["Case", "Conclusion", "Title", "Priority", "Evidence"],
        &case_rows,
    );

    for case in &found.cases {
        let step_rows: Vec<Vec<String>> = case
            .steps
            .iter()
            .map(|step| {
                vec![
                    case.case_number.clone(),
                    step.sequence_id.to_string(),
                    step.action_snapshot.clone(),
                    step.conclusion.clone(),
                    step.actual_value.clone(),
                ]
            })
            .collect();
        output_table(
            &["Case", "Step", "Action", "Verdict", "Actual value"],
            &step_rows,
        );
        let file_rows: Vec<Vec<String>> = case
            .attachments
            .iter()
            .map(|file| {
                vec![
                    case.case_number.clone(),
                    file.name.clone(),
                    file.size.map(|size| size.to_string()).unwrap_or_default(),
                    file.url.clone(),
                ]
            })
            .collect();
        output_table(&["Case", "File", "Size", "Url"], &file_rows);
    }
    Ok(())
}

async fn cmd_run_create(
    client: &PlaneClient,
    project: Option<&str>,
    opts: &RunCreate<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    // The executor is the identity the credential acts under: `me` is
    // the credential itself (the shadow bot member of a service
    // principal).
    let executor = resolve_user_query(opts.executed_by.unwrap_or("me"), client).await?;
    // The re-test link points at the batch this one re-tests; the server
    // refuses a link into another project, into itself, or into a ring.
    let retest_of = match opts.retest_of {
        Some(query) => Some(resolve_test_run(client, &project.id, query).await?.id),
        None => None,
    };
    let write = TestRunWrite {
        run_date: Some(opts.run_date.map(str::to_string).unwrap_or_else(today)),
        version_number: Some(opts.version.to_string()),
        build_number: Some(opts.build.to_string()),
        commit_hash: Some(opts.commit.to_string()),
        environment: Some(opts.environment.to_string()),
        executed_by: Some(executor.id.clone()),
        // The status is the server's: a batch is completed through the
        // finalize endpoint, never by writing the status.
        status: None,
        retest_of,
    };
    let created = client.create_test_run(&project.id, &write).await?;
    if json {
        output_json(&test_run_view(&created));
    } else {
        eprintln!("Created {}.", created.number);
    }
    Ok(())
}

async fn cmd_run_add(
    client: &PlaneClient,
    run: &str,
    cases: &[String],
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_run(client, &project.id, run).await?;
    // Every case is resolved first: the endpoint takes the uuids, and a
    // case that is not a case of this project is refused by the server.
    let mut ids = Vec::new();
    let mut numbers = Vec::new();
    for query in cases {
        let case = resolve_test_case(client, &project.id, query).await?;
        ids.push(case.id.clone());
        numbers.push(case.number.clone());
    }
    // The batch knows which of the cases is already in it — so the
    // refusal names the case before the request is made.
    if let Some(duplicate) = duplicate_in(&found.cases, &ids) {
        return Err(duplicate_case_error(&duplicate, &found.number, None));
    }
    let created = match client
        .add_test_run_cases(&project.id, &found.id, &ids)
        .await
    {
        Ok(created) => created,
        // A concurrent add won the race: the server refused the same
        // reason, and the batch knows which case it was.
        Err(err) if carries_code(&err, "duplicate_case_number") => {
            let cases = client.list_test_run_cases(&project.id, &found.id).await?;
            let duplicate = duplicate_in(&cases, &ids).unwrap_or_else(|| numbers.join(", "));
            return Err(duplicate_case_error(
                &duplicate,
                &found.number,
                Some(&server_sentences(&err)),
            ));
        }
        Err(err) => return Err(err),
    };
    if json {
        let views: Vec<Value> = created.iter().map(test_run_case_view).collect();
        output_json(&views);
    } else {
        for case in &created {
            eprintln!("Added {} to {}.", case.case_number, found.number);
        }
    }
    Ok(())
}

/// The case of the batch that is already in it, if there is one.
///
/// The batch holds a snapshot of every case it took in, and the snapshot
/// names the case it was made of — so the batch itself answers which of
/// the cases the caller tried to add is already there.
fn duplicate_in(cases: &[TestRunCase], ids: &[String]) -> Option<String> {
    cases
        .iter()
        .find(|case| ids.iter().any(|id| id == &case.test_case))
        .map(|case| case.case_number.clone())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_run_step(
    client: &PlaneClient,
    run: &str,
    case: &str,
    step_number: i64,
    verdict: &str,
    actual: Option<&str>,
    actual_file: Option<&Path>,
    image: Option<&Path>,
    yes: bool,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    // The flags are checked before anything is resolved, so a wrong
    // verdict never reaches the server.
    let verdict = normalize_step_verdict(verdict)?;
    if step_number < 1 {
        return Err(PlaneError::validation_with_hint(
            format!("Invalid step number '{step_number}'."),
            "The steps of a case are numbered from 1 (`pbot run show <RUN>` lists them).",
        ));
    }
    let actual_value = collect_markdown(actual, actual_file, "--actual", "--actual-file")?;
    // The confirmation is asked for before the write — and a
    // non-interactive caller has to pass --yes.
    let (project, found, record) = resolve_and_confirm(
        client,
        project,
        run,
        case,
        yes,
        &format!("The verdict {verdict} of step {step_number}"),
    )
    .await?;

    // The file is bound to the step record, so the record is read
    // first — its uuid is what the upload names.
    let mut evidence = None;
    if let Some(path) = image {
        let step_record = client
            .get_test_run_step(&project.id, &found.id, &record.id, step_number)
            .await?;
        evidence = Some(
            upload_image(
                client,
                &project.id,
                path,
                ENTITY_TYPE_STEP_RECORD,
                &step_record.id,
            )
            .await?,
        );
    }
    // A failed step needs a measured value; the server refuses the
    // batch without one at finalize, so the tester is told now. The
    // value the step already carries counts — the measured value stays
    // writable while the batch is open.
    let current = record
        .steps
        .iter()
        .find(|step| step.sequence_id == step_number);
    let measured = actual_value
        .as_deref()
        .or_else(|| current.map(|step| step.actual_value.as_str()));
    let has_file = evidence.is_some()
        || current
            .map(|step| !step.attachments.is_empty())
            .unwrap_or(false);
    let missing_value = verdict == "fail" && evidence_shape(measured, has_file) == Evidence::None;

    let write = TestRunStepWrite {
        conclusion: Some(verdict.clone()),
        actual_value,
    };
    let written = client
        .update_test_run_step(&project.id, &found.id, &record.id, step_number, &write)
        .await?;
    // The notice waits for the write, so it is only said when the step
    // really carries what was written.
    if missing_value {
        eprintln!(
            "warning: a failed step needs a measured value before the batch can be closed (the server refuses it with fail_without_actual_value). Pass --actual/--actual-file, or --image."
        );
    }
    if json {
        output_json(&crate::render::test_run_step_view(&written));
    } else {
        eprintln!(
            "Wrote the verdict {verdict} of step {step_number} of {} in {}.",
            record.case_number, found.number
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_run_conclude(
    client: &PlaneClient,
    run: &str,
    case: &str,
    conclusion: &str,
    evidence: Option<&str>,
    evidence_file: Option<&Path>,
    image: Option<&Path>,
    remarks: Option<&str>,
    actual_result_summary: Option<&str>,
    na_reason: Option<&str>,
    na_criteria_source: Option<&str>,
    manual_executor: Option<&str>,
    manual_condition: Option<&str>,
    yes: bool,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let conclusion = normalize_conclusion(conclusion)?;
    let evidence_text = collect_markdown(evidence, evidence_file, "--evidence", "--evidence-file")?;
    let (project, found, record) = resolve_and_confirm(
        client,
        project,
        run,
        case,
        yes,
        &format!("The conclusion {conclusion}"),
    )
    .await?;

    let write = TestRunCaseWrite {
        conclusion: Some(conclusion.clone()),
        evidence: evidence_text,
        remarks: remarks.map(str::to_string),
        actual_result_summary: actual_result_summary.map(str::to_string),
        na_reason: na_reason.map(str::to_string),
        na_criteria_source: na_criteria_source.map(str::to_string),
        manual_executor: manual_executor.map(str::to_string),
        manual_condition: manual_condition.map(str::to_string),
    };
    // The file is bound to the case record, so it is sent before the
    // conclusion: a refused conclusion leaves the file behind, and the
    // file can be written again — the conclusion cannot.
    let mut uploaded = None;
    if let Some(path) = image {
        uploaded = Some(
            upload_image(
                client,
                &project.id,
                path,
                ENTITY_TYPE_CASE_EVIDENCE,
                &record.id,
            )
            .await?,
        );
    }
    // The server runs the validity rules when the batch is closed; the
    // CLI says what is still missing while the tester can write it. The
    // notice waits for the write, so it is only said when the record
    // really carries what was written.
    let missing = missing_for_finalize(&conclusion, &write, &record, uploaded.is_some());

    let written = client
        .update_test_run_case(&project.id, &found.id, &record.id, &write)
        .await?;
    if !missing.is_empty() {
        eprintln!(
            "warning: the batch cannot be closed until the case carries: {}.",
            missing.join(", ")
        );
    }
    if json {
        output_json(&test_run_case_view(&written));
    } else {
        eprintln!(
            "Wrote the conclusion {conclusion} of {} in {}.",
            record.case_number, found.number
        );
    }
    Ok(())
}

async fn cmd_run_remove(
    client: &PlaneClient,
    run: &str,
    case: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_run(client, &project.id, run).await?;
    let record = require_run_case(&found, case)?;
    client
        .remove_test_run_case(&project.id, &found.id, &record.id)
        .await?;
    if json {
        output_json(&serde_json::json!({
            "number": found.number,
            "case_number": record.case_number,
            "id": record.id,
            "removed": true,
        }));
    } else {
        eprintln!("Removed {} from {}.", record.case_number, found.number);
    }
    Ok(())
}

async fn cmd_run_finalize(
    client: &PlaneClient,
    run: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_run(client, &project.id, run).await?;
    // The executor is asked for at the creation of the batch; a batch
    // without one is refused by the server, so the CLI says it before
    // the request instead of surfacing the raw refusal.
    if found.executed_by.trim().is_empty() && found.executed_by_name.trim().is_empty() {
        return Err(PlaneError::validation_with_hint(
            format!(
                "The batch {} carries no executor, and the server refuses to close a batch without one.",
                found.number
            ),
            "Open a new batch with `pbot run create --executed-by me`.",
        ));
    }
    let closed = client.finalize_test_run(&project.id, &found.id).await?;
    if json {
        output_json(&test_run_view(&closed));
    } else {
        eprintln!("Finalized {}.", closed.number);
    }
    Ok(())
}

async fn cmd_run_summary(
    client: &PlaneClient,
    run: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = require_project(client, project).await?;
    let found = resolve_test_run(client, &project.id, run).await?;
    match client.get_test_run_summary(&project.id, &found.id).await? {
        Some(summary) => {
            if json {
                output_json(&test_run_summary_view(&found.number, &summary));
            } else {
                let rows = vec![
                    vec!["total".to_string(), summary.total.to_string()],
                    vec!["pass".to_string(), summary.pass.to_string()],
                    vec!["fail".to_string(), summary.fail.to_string()],
                    vec!["na".to_string(), summary.na.to_string()],
                    vec!["manual".to_string(), summary.manual.to_string()],
                ];
                output_table(&["Count", "Cases"], &rows);
                let deviations: Vec<Vec<String>> = summary
                    .deviations
                    .iter()
                    .map(|deviation| vec![deviation.case_number.clone(), deviation.summary.clone()])
                    .collect();
                output_table(&["Case", "Deviation"], &deviations);
            }
        }
        // The endpoint is optional on the server side: an instance that
        // does not serve it says so instead of failing the command.
        None => {
            if json {
                output_json(&test_run_summary_unavailable_view(&found.number));
            } else {
                eprintln!(
                    "The summary endpoint is not available on this instance yet ({}).",
                    found.number
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_verdict_is_the_vocabulary_of_the_server() {
        assert_eq!(normalize_step_verdict("PASS").unwrap(), "pass");
        assert_eq!(normalize_step_verdict(" fail ").unwrap(), "fail");
        assert_eq!(normalize_step_verdict("unset").unwrap(), "unset");
    }

    #[test]
    fn a_step_verdict_the_server_would_refuse_is_refused_here() {
        // N/A is a conclusion of a case, not a verdict of a step.
        let err = normalize_step_verdict("na").unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        let message = err.to_string();
        assert!(message.contains("na"), "{message}");
        let hint = err.hint().expect("a hint");
        assert!(hint.contains("pass, fail or unset"), "{hint}");
    }

    #[test]
    fn a_conclusion_is_the_vocabulary_of_the_server() {
        assert_eq!(normalize_conclusion("pass").unwrap(), "PASS");
        assert_eq!(normalize_conclusion("fail").unwrap(), "FAIL");
        assert_eq!(normalize_conclusion("n/a").unwrap(), "N/A");
        assert_eq!(normalize_conclusion("NA").unwrap(), "N/A");
        assert_eq!(normalize_conclusion("manual").unwrap(), "MANUAL");
    }

    #[test]
    fn a_conclusion_the_server_would_refuse_is_refused_here() {
        let err = normalize_conclusion("blocked").unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("blocked"));
        let hint = err.hint().expect("a hint");
        assert!(hint.contains("PASS"), "{hint}");
    }

    #[test]
    fn a_text_and_a_file_are_not_mixed() {
        let err = collect_markdown(
            Some("a"),
            Some(Path::new("/tmp/x")),
            "--actual",
            "--actual-file",
        )
        .unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("--actual"));
    }

    #[test]
    fn the_evidence_of_a_record_is_a_text_or_a_file() {
        // The rule of the server is an or: a text alone is an evidence,
        // a file alone is an evidence, and both are an evidence.
        assert_eq!(evidence_shape(Some("a log"), false), Evidence::Text);
        assert_eq!(evidence_shape(None, true), Evidence::Attachment);
        assert_eq!(evidence_shape(Some("a log"), true), Evidence::Both);
        assert!(evidence_shape(Some("a log"), false).satisfies_finalize());
        assert!(evidence_shape(None, true).satisfies_finalize());
    }

    #[test]
    fn a_placeholder_is_no_evidence() {
        // The server reads a placeholder as no text at all
        // (`is_placeholder` of `testcase_validity.py`): the CLI has to
        // read it the same way, or its notice is skipped exactly when
        // the server refuses the batch.
        for placeholder in ["—", "待补充", "TBD", "tbd", "...", "???", "xxx", "  —  "] {
            assert_eq!(
                evidence_shape(Some(placeholder), false),
                Evidence::None,
                "{placeholder} carries no text"
            );
        }
    }

    #[test]
    fn a_value_that_holds_only_a_picture_is_no_evidence() {
        // A picture is a file, not a sentence: the inline image carries
        // no text — not even its alt text.
        assert_eq!(evidence_shape(Some("![](shot.png)"), false), Evidence::None);
        assert_eq!(
            evidence_shape(Some("![a screenshot](shot.png)"), false),
            Evidence::None
        );
        // With a file attached, the attachment is the evidence.
        assert_eq!(
            evidence_shape(Some("![](shot.png)"), true),
            Evidence::Attachment
        );
    }

    #[test]
    fn the_evidence_rule_is_the_rule_of_the_server() {
        // The values and the answers of the server: `is_filled_markdown`
        // of `db/testcase_validity.py` (mistune 3.4.0, the version the
        // server pins) over the same values. The CLI has to answer the
        // same, so a tester learns at write time what the server will
        // say at finalize.
        let cases: [(&str, bool); 83] = [
            ("—", false),
            ("待补充", false),
            ("TBD", false),
            ("tbd", false),
            ("...", false),
            ("???", false),
            ("xxx", false),
            ("  —  ", false),
            ("![](shot.png)", false),
            ("**TBD**", false),
            ("_TBD_", false),
            ("TBD*", true),
            ("![](a.png error 42", true),
            ("[the report](url)", true),
            ("— (the device stayed silent)", true),
            ("**不一致**", true),
            ("`TBD` is not a text", true),
            ("a_b_c", true),
            ("   ", false),
            ("", false),
            ("the password was refused", true),
            ("see the report", true),
            ("![](Screenshot (1).png)", false),
            ("![shot](shot.png", false),
            ("![alt][ref]", false),
            ("![](a(b).png)", false),
            ("<b>**</b>", false),
            ("`![](x.png)`", true),
            ("![alt](shot.png)", false),
            ("![alt]()", false),
            ("![alt]", true),
            ("![alt] [ref]", true),
            ("*emphasis* text", true),
            ("# heading", true),
            ("> quote", true),
            ("text with <b>bold</b> markup", true),
            ("```\ncode\n```", true),
            ("``code with ` tick``", true),
            ("5 < 6 and 7 > 8", true),
            ("![alt](url) text", true),
            ("text ![alt](url) more", true),
            ("- item", true),
            ("1. item", true),
            ("TBD.", true),
            (".", false),
            ("TODO", false),
            ("n/a", true),
            ("N/A", true),
            ("待定", false),
            ("![alt][ref] more text", true),
            ("![](a b.png) and text", true),
            ("![](a.png", false),
            ("![](a.png) trailing", true),
            ("text with ![img](x.png) inline", true),
            ("| a | b |", true),
            ("**", false),
            ("__", false),
            ("***", false),
            ("![a](b(c)d.png)", false),
            ("![](x.png)![](y.png)", false),
            ("text\n\n---\n\nmore", true),
            ("<!-- comment -->", false),
            ("<!DOCTYPE html>", false),
            ("```code```", true),
            ("```code``", true),
            ("`a `` b`", true),
            ("`![](x.png)", true),
            ("`TBD", true),
            ("<b>TBD</b>", false),
            ("<TBD>", false),
            ("<?php echo 1; ?>", false),
            ("<a href='x'>TBD</a>", false),
            ("text <!-- c --> more", true),
            ("5 < 6 and 7 > 8", true),
            ("<not a tag", true),
            ("a<b>c", true),
            ("![alt](url)TBD", false),
            ("TBD![alt](url)", false),
            ("![](x.png)TBD", false),
            ("``a ` b``", true),
            ("`` TBD ``", false),
            ("text `code` more", true),
            ("```\nTBD\n```", false),
        ];
        for (value, filled) in cases {
            let shape = evidence_shape(Some(value), false);
            assert_eq!(
                shape != Evidence::None,
                filled,
                "the server reads {value:?} as {}",
                if filled { "evidence" } else { "no evidence" }
            );
        }
    }

    #[test]
    fn the_placeholders_are_the_placeholders_of_the_server() {
        // The list is a copy of `PLACEHOLDER_VALUES` of
        // `db/testcase_validity.py` on the server. It is pinned here, so
        // a change of it is a deliberate one — and the rule it feeds
        // only ever *warns*: a write is never refused because of it.
        assert_eq!(PLACEHOLDERS.len(), 25);
        for placeholder in [
            "-",
            "--",
            "---",
            "—",
            "–",
            ".",
            "..",
            "...",
            "…",
            "?",
            "??",
            "???",
            "*",
            "**",
            "_",
            "__",
            "tbd",
            "todo",
            "xxx",
            "待补充",
            "待填写",
            "待填",
            "待定",
            "暂填",
            "待确认",
        ] {
            assert!(
                PLACEHOLDERS.contains(&placeholder),
                "{placeholder} is a placeholder of the server"
            );
        }
    }

    #[test]
    fn a_value_the_server_cannot_read_is_read_as_text() {
        // An unterminated picture and an unterminated link are text,
        // and a mark that stands alone is text too.
        assert_eq!(
            evidence_shape(Some("the log ![a screenshot"), false),
            Evidence::Text
        );
        assert_eq!(
            evidence_shape(Some("the log [a report"), false),
            Evidence::Text
        );
        // `TBD*` is not the placeholder `TBD`: the mark is text.
        assert_eq!(evidence_shape(Some("TBD*"), false), Evidence::Text);
        // `**TBD**` says `TBD`, and `TBD` is a placeholder.
        assert_eq!(evidence_shape(Some("**TBD**"), false), Evidence::None);
    }

    #[test]
    fn a_real_text_is_evidence() {
        // The markup is not what the value says: `**不一致**` carries the
        // text `不一致`.
        assert_eq!(
            evidence_shape(Some("the password was refused"), false),
            Evidence::Text
        );
        assert_eq!(evidence_shape(Some("**不一致**"), false), Evidence::Text);
        assert_eq!(
            evidence_shape(Some("`TBD` is not a text"), false),
            Evidence::Text
        );
        // A placeholder with something else in it is an observation.
        assert_eq!(
            evidence_shape(Some("— (the device stayed silent)"), false),
            Evidence::Text
        );
        // The text of a link is text; its destination is not.
        assert_eq!(
            evidence_shape(Some("[the report](http://x/report.pdf)"), false),
            Evidence::Text
        );
    }

    #[test]
    fn an_empty_evidence_is_no_evidence() {
        // A text that holds nothing but blanks carries no evidence, and
        // a record without one is refused by the finalize.
        assert_eq!(evidence_shape(None, false), Evidence::None);
        assert_eq!(evidence_shape(Some("   \n  "), false), Evidence::None);
        assert!(!evidence_shape(None, false).satisfies_finalize());
    }

    #[test]
    fn the_finalize_rules_name_what_is_missing() {
        let case = TestRunCase {
            case_number: "TC-CLI-007".into(),
            ..Default::default()
        };
        let write = TestRunCaseWrite {
            conclusion: Some("N/A".into()),
            ..Default::default()
        };
        let missing = missing_for_finalize("N/A", &write, &case, false);
        assert!(missing.contains(&"evidence"), "{missing:?}");
        assert!(missing.contains(&"na_reason"), "{missing:?}");
        assert!(missing.contains(&"na_criteria_source"), "{missing:?}");
    }

    #[test]
    fn what_the_record_already_carries_is_not_missing() {
        // The evidence and the reason may have been written before the
        // conclusion — the record carries them.
        let case = TestRunCase {
            evidence: "see the report".into(),
            na_reason: "the feature is off".into(),
            na_criteria_source: "the release notes".into(),
            ..Default::default()
        };
        let write = TestRunCaseWrite {
            conclusion: Some("N/A".into()),
            ..Default::default()
        };
        let missing = missing_for_finalize("N/A", &write, &case, false);
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn the_image_types_the_server_accepts_are_read_from_the_name() {
        assert_eq!(image_mime(Path::new("shot.png")), Some("image/png"));
        assert_eq!(image_mime(Path::new("shot.JPG")), Some("image/jpeg"));
        assert_eq!(image_mime(Path::new("shot.jpeg")), Some("image/jpeg"));
        assert_eq!(image_mime(Path::new("shot.webp")), Some("image/webp"));
        assert_eq!(image_mime(Path::new("shot.gif")), Some("image/gif"));
        // A log is not an image: it belongs into the Markdown field.
        assert_eq!(image_mime(Path::new("run.log")), None);
        assert_eq!(image_mime(Path::new("shot")), None);
    }

    #[test]
    fn the_calendar_date_of_a_day_is_computed() {
        // 2026-11-11 is 20768 days after 1970-01-01.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_768), (2026, 11, 11));
        // A leap day: 2024-01-01 is 19723 days after 1970-01-01, and
        // February has 29 days that year.
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn the_re_test_marker_names_the_original_batch() {
        let batch = TestRun {
            retest_of: Some("r0".into()),
            retest_of_number: Some("RUN-20261108-02".into()),
            ..Default::default()
        };
        // The marker is English: the CLI speaks English everywhere, and
        // the label of the web UI is not copied into the output.
        assert_eq!(retest_marker(&batch), "retest of RUN-20261108-02");
        // A batch that re-tests nothing carries no marker.
        assert_eq!(retest_marker(&TestRun::default()), "");
        assert_eq!(
            retest_marker(&TestRun {
                retest_of_number: Some(String::new()),
                ..Default::default()
            }),
            ""
        );
    }

    #[test]
    fn a_duplicate_names_the_case_and_the_code() {
        let err = duplicate_case_error("TC-CLI-007", "RUN-20261109-01", None);
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        let message = err.to_string();
        assert!(message.contains("duplicate_case_number"), "{message}");
        assert!(message.contains("TC-CLI-007"), "{message}");
        assert!(message.contains("RUN-20261109-01"), "{message}");
    }

    #[test]
    fn the_answer_of_the_confirmation_decides_the_write() {
        // The answer is read as it was typed: the case of the letters
        // does not matter, and every other answer refuses the write.
        assert!(is_confirmation("y"));
        assert!(is_confirmation("Y"));
        assert!(is_confirmation("yes"));
        assert!(is_confirmation("yes\n"));
        assert!(!is_confirmation("n"));
        assert!(!is_confirmation("no"));
        assert!(!is_confirmation(""));
        assert!(!is_confirmation("yeah"));
    }

    #[test]
    fn a_confirmed_write_goes_through_and_a_refused_one_writes_nothing() {
        let confirmed = confirm_with("The verdict fail of step 3", &mut "y\n".as_bytes());
        assert!(confirmed.is_ok(), "{confirmed:?}");
        let refused = confirm_with("The verdict fail of step 3", &mut "n\n".as_bytes())
            .expect_err("an answer that is not y refuses the write");
        assert_eq!(refused.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        let message = refused.to_string();
        assert!(message.contains("Aborted"), "{message}");
        // An empty answer is a refusal as well: the default is "no".
        let empty = confirm_with("The verdict fail of step 3", &mut "".as_bytes())
            .expect_err("an empty answer refuses the write");
        assert!(empty.to_string().contains("Aborted"));
    }

    #[test]
    fn a_write_that_cannot_be_confirmed_is_refused() {
        // The test runs without a terminal, so the write cannot be
        // confirmed and nothing is written.
        let err =
            refuse_unconfirmable(false, "The verdict fail of step 3 of TC-CLI-007").unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        let hint = err.hint().expect("a hint");
        assert!(hint.contains("--yes"), "{hint}");
        // With --yes the write goes through.
        assert!(refuse_unconfirmable(true, "The verdict fail of step 3").is_ok());
    }
}
