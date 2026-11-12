# pbot Command Reference

## Table of Contents
- [Global Options](#global-options)
- [Work Items](#work-items)
- [Relations](#relations)
- [Attachments](#attachments)
- [Projects](#projects)
- [Cycles](#cycles)
- [Modules](#modules)
- [Labels](#labels)
- [States](#states)
- [Documents](#documents)
- [Comments](#comments)
- [Intake](#intake)
- [Test cases](#test-cases)
- [Test runs](#test-runs)
- [Users](#users)
- [Cache](#cache)

## Global Options

| Flag | Description |
|---|---|
| `--no-cache` | Bypass cache for this command |
| `--json` | Output JSON to stdout (available on most commands) |
| `--version` / `-V` | Show version |
| `--help` / `-h` | Show help |

### Command aliases

| Full | Aliases |
|---|---|
| `work-item` | `wi`, `issues`, `issue` |
| `project` | `projects` |
| `document` | `doc`, `docs`, `documents` |
| `comment` | `comments` |
| `module` | `modules` |
| `label` | `labels` |
| `state` | `states` |
| `cycle` | `cycles` |
| `user` | `users` |
| `list` | `ls` |
| `show` | `read` |
| `create` | `new` |

## Work Items

Command group: `pbot wi` (aliases: `work-item`, `issues`, `issue`)

### wi ls

```
pbot wi ls [OPTIONS]
```

| Flag | Description |
|---|---|
| `--project` / `-p` | Project name, identifier, or UUID (omit for all projects) |
| `--assignee` | Filter by assignee name or `me` |
| `--state` | Filter by state name (comma-separated for OR) |
| `--labels` | Filter by label name (comma-separated for OR) |
| `--parent` | List children of a parent work item (ABC-123, UUID, or name) |
| `--sort` | Sort by: `created` (default), `updated` |
| `--limit` / `-l` | Max results (default: 50) |
| `--json` | JSON output |

### wi show

```
pbot wi show ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier (ABC-123), UUID, or name (required) |
| `--project` / `-p` | Project (required for name-based lookup) |
| `--no-comments` | Skip fetching the work item's comments |
| `--json` | JSON output |

Bundles the work item's comments in the same call (chronological, oldest → newest;
same data `comment ls` returns). In `--json` output, the `comments` field is a list
(`[]` if there are none), or `null` if the comment fetch failed — the work item
itself still returns and the command still exits 0. In human output, a `Comments`
section follows the work item details, showing `(none)` or `(failed to load)` as
appropriate. Pass `--no-comments` to skip the fetch entirely (the `comments` key is
then omitted from JSON output).

### wi create

```
pbot wi create TITLE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `TITLE` | Work item title (required) |
| `--project` / `-p` | Project (required) |
| `--assignee` / `--assign` | Assignee name, email, or `me` |
| `--state` | State name (e.g. `Todo`, `In Progress`) |
| `--labels` | Comma-separated label names |
| `--priority` | `urgent`, `high`, `medium`, `low`, `none` (or 0-4) |
| `--parent` | Parent work item identifier (ABC-123) for sub-issues |
| `--description` / `-d` | Description as plain text (wrapped in a paragraph) |
| `--desc-md` | Description as markdown (converted to HTML). Mutually exclusive with `-d` |
| `--image` / `-i` | Image file path to embed in the description (repeatable). Uploaded and appended as an img tag |
| `--force` | Upload images even if an attachment with the same file name already exists |
| `--start-date` | Start date `YYYY-MM-DD`. Bad formats are rejected (exit 5) |
| `--target-date` | Target end date `YYYY-MM-DD`. The write is verified against the echoed record |
| `--json` | JSON output |

### wi update

```
pbot wi update ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `--project` / `-p` | Project (required for name-based lookup) |
| `--state` | New state name |
| `--priority` | New priority |
| `--assignee` / `--assign` | New assignee name or `me` |
| `--labels` | Comma-separated labels to set |
| `--clear-labels` | Remove all labels |
| `--name` | New title |
| `--description` / `-d` | New description as plain text (wrapped in a paragraph) |
| `--desc-md` | New description as markdown (converted to HTML). Mutually exclusive with `-d` |
| `--parent` | Set the parent work item (ABC-123, UUID, or name). Mutually exclusive with `--clear-parent` |
| `--clear-parent` | Remove the parent, making the work item top-level |
| `--image` / `-i` | Image file path to embed in the description (repeatable). Uploaded and appended as an img tag |
| `--force` | Upload images even if an attachment with the same file name already exists |
| `--start-date` | New start date `YYYY-MM-DD` |
| `--target-date` | New target end date `YYYY-MM-DD`. The write is verified against the echoed record |
| `--json` | JSON output |

### wi delete

```
pbot wi delete ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `--project` / `-p` | Project (required for name-based lookup) |

### wi search

```
pbot wi search QUERY [OPTIONS]
```

| Parameter | Description |
|---|---|
| `QUERY` | Search text (required) |
| `--project` / `-p` | Project (required) |
| `--sort` | Sort by: `created` (default), `updated` |
| `--limit` / `-l` | Max results (default: 50) |
| `--json` | JSON output |

### wi assign

```
pbot wi assign ISSUE [OPTIONS]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `--assignee` / `--assign` | Assignee (default: `me`) |
| `--project` / `-p` | Project (required for name-based lookup) |

## Attachments

Command group: `pbot attachment` (alias: `attachments`)

```
pbot attachment ls ISSUE [-p PROJECT] [--json]
pbot attachment attach ISSUE -f FILE [-p PROJECT] [--force] [--json]
```

`attach` aliases: `upload`, `new`; `ls` alias: `list`.

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier, UUID, or name (required) |
| `-f` / `--file` | Local file path to upload (required on `attach`) |
| `--project` / `-p` | Project (required for name-based lookup) |
| `--force` | Upload even if an attachment with the same file name already exists on the issue |
| `--json` | JSON output |

Upload follows the API's three-step flow: register an asset (presigned S3 URL), `PUT` the
binary, then mark `is_uploaded` and re-read the asset to verify. Without `--force`, a duplicate
file name is refused with a validation error (exit 5) and a hint to re-run with `--force` — the
CLI never prompts.

Description images uploaded via `wi create -i` / `wi update -i` use the same upload but are not
listed as attachments; the img tag's `src` holds only the asset UUID, which the web editor
resolves at render time.

## Relations

Command group: `pbot relations` (alias: `relation`)

```
pbot relations ls ISSUE [--project/-p PROJECT]
pbot relations add ISSUE --type TYPE -t TARGET [-t TARGET ...] [--project/-p PROJECT]
pbot relations remove ISSUE --type TYPE -t TARGET [--project/-p PROJECT]
```

| Parameter | Description |
|---|---|
| `ISSUE` | Work item identifier (`ABC-123`), UUID, or name |
| `--type` | One of `blocking`, `blocked_by`, `duplicate`, `relates_to`, `start_before`, `start_after`, `finish_before`, `finish_after` |
| `-t` / `--to` | Target work item; repeatable |
| `--project` / `-p` | Project (required when resolving a target by name) |

`ls` renders one row per related work item (`Type`, `Related`, `Issue ID`), or `No relations for X.`
when there are none; `--json` returns the raw per-type relation buckets instead of the flattened rows.

`add` resolves every `-t` target inside the same project and rejects a self-relation before writing,
then reports how many work items were linked.

`remove` **always fails**, by design: Plane exposes relation creation but no delete endpoint. The
command exists so the missing capability is reported as a validation error instead of appearing to
succeed.

## Projects

Command group: `pbot project` (alias: `projects`)

### project ls

```
pbot project ls [OPTIONS]
```

| Flag | Description |
|---|---|
| `--state` / `-s` | Filter: `planned`, `started`, `paused`, `completed`, `canceled` |
| `--limit` / `-l` | Max results (default: 50) |
| `--sort` | Sort: `linear` (default), `created`, `updated` |
| `--json` | JSON output |

### project show

```
pbot project show PROJECT [OPTIONS]
```

### project create

```
pbot project create NAME [OPTIONS]
```

| Parameter | Description |
|---|---|
| `NAME` | Project name (required) |
| `--identifier` / `-i` | Short identifier (e.g. `API`) |
| `--description` / `-d` | Description |
| `--json` | JSON output |

### project update / delete

```
pbot project update PROJECT [OPTIONS]
pbot project delete PROJECT
```

## Cycles

Command group: `pbot cycle` (alias: `cycles`)

### cycle ls / show / create / update / delete

```
pbot cycle ls -p PROJECT
pbot cycle show CYCLE -p PROJECT
pbot cycle create NAME -p PROJECT --start-date YYYY-MM-DD --end-date YYYY-MM-DD
pbot cycle update CYCLE -p PROJECT [OPTIONS]
pbot cycle delete CYCLE -p PROJECT
```

### cycle add-item / remove-item / items

```
pbot cycle add-item CYCLE ISSUE -p PROJECT
pbot cycle remove-item CYCLE ISSUE -p PROJECT
pbot cycle items CYCLE -p PROJECT
```

## Modules

Command group: `pbot module` (alias: `modules`)

```
pbot module ls -p PROJECT
pbot module show MODULE -p PROJECT
pbot module create NAME -p PROJECT [-d DESCRIPTION] [--start-date DATE] [--end-date DATE] [--status STATUS]
pbot module update MODULE -p PROJECT [--name NAME] [-d DESCRIPTION] [--start-date DATE] [--end-date DATE] [--status STATUS]
pbot module delete MODULE -p PROJECT
```

**Module status values** (`--status`): `backlog`, `planned`, `in-progress`, `paused`, `completed`, `cancelled`.
Also accepts Portuguese aliases: `planejado`, `em andamento`, `pausado`, `concluído`, `cancelado` (and `canceled`, `in progress`).

## Labels

Command group: `pbot label` (alias: `labels`)

```
pbot label ls -p PROJECT
pbot label show LABEL -p PROJECT
pbot label create NAME -p PROJECT [--color "#HEX"]
pbot label update LABEL -p PROJECT [--name NAME] [--color "#HEX"]
pbot label delete LABEL -p PROJECT
```

## States

Command group: `pbot state` (alias: `states`)

State groups: `backlog`, `unstarted`, `started`, `completed`, `cancelled`, `triage`

```
pbot state ls -p PROJECT [--group GROUP]
pbot state show STATE -p PROJECT
pbot state create NAME -p PROJECT --group GROUP [--color "#HEX"]
pbot state update STATE -p PROJECT [--color "#HEX"]
pbot state delete STATE -p PROJECT
```

## Documents

Command group: `pbot doc` (aliases: `document`, `documents`, `docs`)

```
pbot doc ls -p PROJECT
pbot doc show TITLE -p PROJECT
pbot doc create --title TITLE --content CONTENT -p PROJECT
pbot doc create --title TITLE --content-md MARKDOWN -p PROJECT
pbot doc create --title TITLE --content-html HTML -p PROJECT
pbot doc update TITLE --content CONTENT -p PROJECT
pbot doc archive TITLE -p PROJECT
pbot doc delete TITLE -p PROJECT
```

Three mutually exclusive content sources: `-c/--content` (plain text, converted to HTML),
`--content-md` (native markdown — headings, lists, fenced code, bold/italic, links, tables,
blockquotes — converted to HTML) and `--content-html` (raw HTML stored verbatim, for rich layout).
Pasting markdown or HTML through `--content` renders it literally, so pick the matching flag.

`doc archive` sets `archived_at`, leaves the page recoverable in the web UI trash, and verifies the
archive landed. `doc delete` archives first and then deletes.

## Comments

Command group: `pbot comment` (alias: `comments`)

```
pbot comment ls ISSUE [--project/-p PROJECT] [--limit/-l N]
pbot comment create ISSUE --body "TEXT" [--project/-p PROJECT]
pbot comment create ISSUE --body-md "MARKDOWN" [--project/-p PROJECT]
pbot comment update COMMENT_ID --issue ISSUE --body "TEXT" [--project/-p PROJECT]
pbot comment delete COMMENT_ID --issue ISSUE [--project/-p PROJECT]
```

Exactly one of `-b/--body` (plain text, converted to HTML) or `--body-md` (markdown subset — code
fences and spans, bold/italic, lists, auto-linked URLs) is required; they are mutually exclusive.
`--body-md` is the one to use when the comment contains code or a URL, since plain text keeps the
backticks literal and leaves URLs unlinked.

`--limit` (default 50) selects the **most recent** N comments, still rendered
oldest → newest — not the first N chronologically. A limit of `0` or a negative
value returns no comments (consistent with the `[:limit]` semantics used elsewhere,
where `0` means "none").

## Intake

Command group: `pbot intake` (no alias)

```
pbot intake ls -p PROJECT
pbot intake create NAME -p PROJECT [--description/-d TEXT] [--priority/-P PRIORITY]
pbot intake accept ISSUE_ID -p PROJECT
pbot intake decline ISSUE_ID -p PROJECT
pbot intake delete ISSUE_ID -p PROJECT
pbot intake enabled PROJECT
```

| Parameter | Description |
|---|---|
| `ISSUE_ID` | **Work item UUID** — the `Issue ID` column of `intake ls`, not the intake wrapper `Intake ID` |
| `--project` / `-p` | Project name, identifier, or UUID (required on every subcommand except `enabled`, which takes the project as its argument) |
| `--description` / `-d` | Item description, plain text (wrapped in a paragraph tag and HTML-escaped) |
| `--priority` / `-P` | `none` (default), `low`, `medium`, `high`, `urgent` — an unknown value exits `5` |
| `--json` | JSON output (all subcommands except `delete`) |

Item `status` is reported as a label: `pending`, `rejected`, `snoozed`, `accepted`, `duplicate`.

`accept` and `decline` **require the project Admin role**. Plane answers `HTTP 200` with the
record unchanged for lower roles, so the CLI compares the returned status against the requested
one and exits `4` ("the intake status was not changed") instead of reporting a false success.

`delete` is destructive beyond the queue: for any status other than `accepted` it also
**permanently deletes the underlying work item**. There is no confirmation prompt and no undo —
use `decline` when the intent is only to reject the submission.

`ls` returns an empty list when the project's intake view is off; `enabled` reports the same
project flag (`intake_enabled`).

## Test cases

Command group: `pbot tc`

```
pbot tc ls -p PROJECT [--module CODE] [--priority P0|P1|P2|P3] [--issue ISSUE]
                      [--status active|all] [--search TEXT] [--limit/-l N]
pbot tc show CASE -p PROJECT
pbot tc create -p PROJECT --module CODE --title TEXT --purpose TEXT --object-overview TEXT
                     --priority P0|P1|P2|P3 --precondition TEXT --environment TEXT
                     [--related-feature TEXT] [--remarks TEXT] [--status active|void]
                     [--step "ACTION | EXPECTED"]... [--steps-file PATH]
pbot tc update CASE -p PROJECT [--title TEXT] [--purpose TEXT] [--object-overview TEXT]
                     [--priority P] [--precondition TEXT] [--environment TEXT]
                     [--related-feature TEXT] [--remarks TEXT] [--status active|void]
                     [--step "ACTION | EXPECTED"]... [--steps-file PATH]
pbot tc steps CASE -p PROJECT [--step "ACTION | EXPECTED"]... [--steps-file PATH]
pbot tc issues CASE -p PROJECT [--add/-a ISSUE] [--remove/-r ISSUE]
pbot tc delete CASE -p PROJECT
pbot tc module ls -p PROJECT
pbot tc module create -p PROJECT --code CODE [--description/-d TEXT]
```

| Parameter | Description |
|---|---|
| `CASE` | Case number (`TC-CLI-001`), UUID, or title |
| `--module` / `--code` | Module code of the case — the prefix of its number. Registered first with `tc module create` |
| `--title`, `--purpose`, `--object-overview`, `--priority`, `--precondition`, `--environment` | The six header fields the **server** refuses a case without. `--preconditions` and `--env` are aliases of `--precondition` and `--environment` |
| `--step "ACTION \| EXPECTED"` | One step, written as `action | expected result`. Both halves are required |
| `--steps-file` | The whole step table: a JSON array `[{"action": "…", "expected": "…"}]`, or one `action | expected result` per line (`#` lines and blank lines are skipped) |
| `--priority` | `P0`, `P1`, `P2`, `P3` |
| `--status` | `active` (the default) or `void` |
| `--issue`, `--add`, `--remove` | The work-item links of the case (`--add` and `--remove` are mutually exclusive; without either the links are listed) |
| `--limit` / `-l` | Maximum results (default 50) |

The server assigns the number of a case — the CLI never builds one. `tc module create` is refused
by the server when the code is not 2 to 6 uppercase letters or is already taken; the refusal
carries the server's code (`module_code_format`, `module_code_taken`).

`tc update` refuses `--module` and `--number` locally (exit 5): the number is bound to the module
and the server keeps both.

`tc steps` **prints the step rows, not the case number** — a `Step` / `Action` / `Expected result`
table on stderr, the same rows as a JSON array on stdout. The number of the case is what
`tc show` prints. With `--step` / `--steps-file` the table is replaced **as a whole**: every step
has to be passed again, the server drops the rows that are not in the list.

`tc delete` is destructive and only a project admin may do it.

## Test runs

Command group: `pbot run`

```
pbot run ls -p PROJECT [--status draft|in_progress|completed|all] [--run-date YYYY-MM-DD] [--limit/-l N]
pbot run show RUN -p PROJECT
pbot run create -p PROJECT --version TEXT --build TEXT --commit TEXT --environment TEXT
                       [--run-date YYYY-MM-DD] [--retest-of RUN] [--executed-by WHO]
pbot run add RUN CASE... -p PROJECT
pbot run step RUN --case CASE --step N --verdict pass|fail|unset
                  [--actual MARKDOWN] [--actual-file PATH] [--image PATH] [--yes/-y]
pbot run conclude RUN --case CASE --conclusion PASS|FAIL|N/A|MANUAL
                      [--evidence MARKDOWN] [--evidence-file PATH] [--image PATH]
                      [--remarks TEXT] [--actual-result-summary TEXT]
                      [--na-reason TEXT] [--na-criteria-source TEXT]
                      [--manual-executor WHO] [--manual-condition TEXT] [--yes/-y]
pbot run remove RUN --case CASE -p PROJECT
pbot run finalize RUN -p PROJECT
pbot run summary RUN -p PROJECT
```

| Parameter | Description |
|---|---|
| `RUN` | Batch number (`RUN-20261109-01`) or UUID |
| `--case` | The case of the batch (`TC-CLI-007`) |
| `--step` | The number of the step in the case (1, 2, 3 …) |
| `--verdict` | `pass`, `fail` or `unset` — a step that was not executed is `unset`. `N/A` is a conclusion of a case, not a verdict of a step |
| `--conclusion` | `PASS`, `FAIL`, `N/A` or `MANUAL` (`NA` is normalized to `N/A`) |
| `--actual`, `--actual-file` | The measured value of a step. **Pass-through**: it reaches the server as it was typed, byte for byte — no Markdown conversion |
| `--evidence`, `--evidence-file` | The evidence of a case. Same pass-through rule |
| `--image` | A screenshot. On `run step` it belongs to the step; on `run conclude` it is the evidence of the case |
| `--yes` / `-y` | Confirm the write. Needed when the command is not on a terminal — otherwise the command asks, and refuses (exit 5) without a terminal |
| `--version`, `--build`, `--commit`, `--environment` | The tested build of the batch. Frozen once the batch is created |
| `--run-date` | The day of the execution. **UTC**, and frozen after the creation. On `run ls` it filters the batches of that day |
| `--retest-of` | The batch this one re-tests. A re-test runs in the project of its original and never in a ring |
| `--executed-by` | Who executed the batch: `me` (the default), a name, or a UUID |
| `--na-reason`, `--na-criteria-source` | Required by the server for a `N/A` case |
| `--manual-executor`, `--manual-condition` | Required by the server for a `MANUAL` case |
| `--actual-result-summary` | The one-line phenomenon of the case (the summary shows it) |
| `--status` (of `run ls`) | `draft`, `in_progress`, `completed`, or `all` |

Two rules shape the whole group: a verdict and a conclusion are written **once** — the server
refuses a second write with `conclusion_is_final`, and **no flag overwrites a verdict** — and a
**completed** batch is **frozen**, so every later write to it is refused with `run_completed`. A
wrong verdict is corrected by voiding the record and running the case again in a new batch
(`pbot run create --retest-of <run>`).

**The order decides the code.** A second conclusion write on an **open** batch is refused with
`conclusion_is_final`; the same write **after** the finalize is refused with `run_completed`,
because the freeze of a completed batch is checked first.

**The evidence hangs on the case, not on the step.** The server counts case-level evidence only:
a case-level text (`--evidence` / `--evidence-file`) **or** a case-level file (`--image`). A
screenshot of a **step** does not count — the finalize answers `evidence_missing` with the path
`cases[TC-CLI-001].evidence` and names the case.

`--run-date` defaults to **today in UTC**: an operator at UTC+8 who creates a batch between 00:00
and 08:00 local time gets the previous UTC day in `run_date`, and the number of the batch carries
that day. The four counts of `run summary` are **not a partition** of the total — a case that
failed on one step and was never executed on another is counted twice, so `fail` and `manual` may
count the same case.

The listing shows the re-test link in the `Retest of` column as `retest of RUN-20261109-01` —
English, unlike the web UI.

## Users

```
pbot user ls         # List workspace members
pbot whoami          # Show authenticated user
```

`whoami`'s `name` row marks the credential kind: a human prints its plain name, a service
principal prints `<name> (service principal)` (or `<name> (SP owned by <owner>)` when the
`/users/me/` payload names the owner, which the API does not send today).

Under a service-principal token `me` means that principal's **shadow bot workspace member**
(`svc-<name> (bot)`, the user Plane credits its writes to) — not the principal id `whoami`
prints, which is not a member and would silently assign nobody. A service-principal `me` result
names what it resolved to, member UUID included; if no such member exists, `me` fails with a hint
naming `pbot user ls` instead of returning an empty result. A human credential's output is
unchanged.

## Cache

```
pbot cache clear     # Clear all cached data
```
