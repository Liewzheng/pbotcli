# pbot

**English** · [中文](cli-command-reference.zh.md)

## NAME

`pbot` — command-line client for [Plane.so](https://plane.so) (SaaS or self-hosted): work items, projects, cycles, modules, labels, states, documents, intake queues, and comments.

`pbot` is an alias of `pbotcli`; both names run the same binary.

## SYNOPSIS

```
pbot [global flags] <command> [<subcommand>] [<argument>...] [flags]
```

## DESCRIPTION

Every resource argument accepts a **name**, an **identifier** (`ABC-123`), or a **UUID**; close names resolve by fuzzy match (token-sort-ratio, threshold 60). The authenticated user can be referenced as `me` wherever an assignee is expected; under a service-principal token `me` resolves to that principal's **shadow bot workspace member** (see [pbot whoami](#pbot-whoami)).

A lookup that spans projects (`wi show`, `wi ls` without `-p`) **skips** a project the credential cannot read — HTTP 403, which is what a service principal sees on every project it has no grant for, or 404 — and warns which projects were skipped, instead of failing the whole lookup on the first one. Only the work-item read of a project decides that skip: a denial on the project's states or labels is a different failure and aborts the command rather than being reported as "no read access" to a project whose items were readable. Any other failure (a rejected credential, a 5xx) still aborts too; a `wi show` that then matches nothing lists the projects the search could not cover in its hint.

Human-readable tables are written to **stderr**, machine-readable JSON to **stdout**, so `pbot ... --json 2>/dev/null` always yields clean JSON. Reads are cached on disk; writes invalidate the affected resource and, where the API can silently ignore a write (dates, triage, uploads), the result is verified by reading it back.

## GLOBAL OPTIONS

| Option | Description |
|---|---|
| `--json` | Print JSON to stdout; the human table goes to stderr. |
| `--no-cache` | Bypass the disk cache for this one command. |
| `-h, --help` | Show help for any command. |
| `-V, --version` | Print the version. |

## ENVIRONMENT

| Variable | Description |
|---|---|
| `PLANE_BASE_URL` | Instance base URL, e.g. `https://api.plane.so`. |
| `PLANE_API_KEY` | Service token (`plane_api_...`). |
| `PLANE_WORKSPACE` | Workspace slug (not the instance name). |

Configuration is discovered from several files, highest priority first: `~/.config/pbot/config.toml` (TOML with top-level `base_url` / `api_key` / `workspace`, optionally under `[auth]`), `~/.pbot`, `~/.planecli`, then the legacy `~/.plane_api` (`key=value` lines with lowercase keys; `chmod 600`). Precedence: CLI flags > environment variables > config file. `pbot configure` keeps writing `~/.plane_api`.

> Self-hosted instances behind a proxy: run `env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u all_proxy pbot ...` so CLI traffic reaches the instance directly.

## EXIT CODES

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic error |
| 2 | Authentication error **or permission denied** — and a **usage error** (a flag or an argument clap refuses) |
| 3 | Resource not found |
| 4 | API error (includes HTTP status and field-level detail) — and a transport failure |
| 5 | Validation error (client-side **or** server-side, when the server names a reason) |

Two details worth knowing before a script keys on these:

- **A usage error exits 2, the same code as an authentication or a permission failure.** A missing
  required flag, an unknown flag, or a subcommand that needs an argument is refused by clap
  before any request is made; the message and the usage line go to **stderr**, so stdout stays
  clean (`--json 2>/dev/null` yields nothing, not a broken document). Code 2 therefore means
  "the CLI or the credential could not get that far" — it does not distinguish a wrong password
  from a wrong flag.
- **A refusal of the server that names a reason exits 5, not 4.** A `400` or a `409` whose body
  carries a stable code (`invalid_test_case`, `conclusion_is_final`, `evidence_missing`, …) is a
  validation failure the caller can act on; a `400` or a `409` without a code is still exit 5,
  because a conflict of this API is a refusal of the write rather than a transport failure. Exit 4
  is what is left: the transport failed, or the server answered with something the feature does
  not describe.

The classes of the test-case feature (`tc`, `run`) in one table:

| Class | Exit | Examples |
|---|---|---|
| Success | 0 | the write landed, the read answered |
| Authentication / permission | 2 | `401`, `403` — a service principal without the grant on the project |
| Usage error | 2 | a missing required flag, an unknown flag |
| Not found | 3 | a case or a batch that no project holds |
| API / transport | 4 | a 5xx, a connection error, an answer the client cannot read |
| Validation | 5 | a local check, and every server refusal that carries a stable code |

## COMMANDS

| Command | Purpose |
|---|---|
| [`whoami`](#pbot-whoami) | Show the authenticated user. |
| [`configure`](#pbot-configure) | Write credentials interactively (to the active config file). |
| [`user`](#pbot-user) | Workspace members. |
| [`cache`](#pbot-cache) | Manage the local disk cache. |
| [`project`](#pbot-project) | Projects. |
| [`wi`](#pbot-wi) | Work items (aliases: `work-item`, `issues`, `issue`). |
| [`relations`](#pbot-relations) | Work-item relations (alias: `relation`). |
| [`comment`](#pbot-comment) | Comments on work items. |
| [`label`](#pbot-label) | Labels. |
| [`state`](#pbot-state) | States. |
| [`module`](#pbot-module) | Modules. |
| [`cycle`](#pbot-cycle) | Cycles (sprints). |
| [`intake`](#pbot-intake) | Intake queue. |
| [`doc`](#pbot-doc) | Documents / pages. |
| [`attachment`](#pbot-attachment) | Work-item attachments (alias: `attachments`). |
| [`tc`](#pbot-tc) | Test cases of a project. |
| [`run`](#pbot-run) | Test runs (batches) of a project. |

---

## pbot whoami

Show the authenticated user.

```
pbot whoami [--json]
```

`--json` prints the raw `/api/v1/users/me/` payload. The table's `name` row says which kind of
credential is in use: a **human** account prints its plain name, a **service principal** prints
`<name> (service principal)` — a token issued for a bot, with no person fields to show — or
`<name> (SP owned by <owner>)` when the payload names the owning human.

**Why the owner is usually absent.** `/api/v1/users/me/` deliberately never serializes the owner's
profile: a service principal is not allowed to read the human who owns it, and that is a server-side
permission decision, not a client limitation. The owner is therefore read forward-compatibly — the
marker above appears only if the payload ever carries an `owner` (object or name) or `owner_name`
field. The session-only `/api/workspaces/<slug>/service-principals/` endpoint does return `owner`,
but it rejects a service-principal token (403), so the CLI does not probe it.

### `me` under a service principal

`me` resolves to the credential's **shadow bot workspace member**: the `User` row Plane credits the
principal's writes to, listed by `pbot user ls` as `svc-<name> (bot)`. This matters because a
service principal answers `/users/me/` with its `ServicePrincipal.id`, which is **not** a member id —
assigning it does nothing and filtering on it returns nothing. `me` never sends that id to the API.

Every service-principal `me` result says what it resolved to, so an unattended caller can act
without reading the source:

- Every write made with `--assignee me` — `wi assign`, `wi create`, `wi update` — prints, once the
  write has landed,
  `Work item assigned to <name> (service principal) as its shadow bot member '<member name>' (member id <uuid>).`
  `<member name>` is the matched row's `display_name` (the field `pbot user ls --json` reports, and
  what `pbot user ls`'s table shows for a row that has one), so the member can be
  found from the message.
- `wi ls --assignee me` with no matching rows prints a note naming the principal, the shadow bot
  member and its id, and the next command to run (`pbot wi ls -p <project> --assignee '<member name or uuid>'`,
  members listed by `pbot user ls`).
- When no shadow bot member exists, `me` fails (exit 3) with a hint naming `pbot user ls` instead of
  handing back an id that would be silently ignored.
- If the workspace member list itself cannot be read, `wi ls --assignee` with a name or `me` fails
  (exit 3) with a hint naming `pbot user ls` — the filter cannot be answered without the list, and
  dropping the filter would return rows nobody asked for — while an explicit member id still filters,
  because there the list only supplies the display name.
- A plain listing still runs either way: for it the member list is enrichment, so the assignee column
  falls back to the member id's first 8 characters and a warning says which call was denied —
  `warning: could not read the workspace member list (…); assignee names will show the member id's
  first 8 characters — check access with 'pbot user ls'`. The list is only read when it will be used:
  a listing with no `--assignee` whose rows carry no assignees makes no member request and prints no
  warning.

A human credential's `me` output is unchanged: it resolves to the account's own member id, and an
explicit name or UUID never prints an identity note.

## pbot configure

Write credentials interactively (prompts for base URL, API key, workspace slug), saving to the active config file (highest-priority existing candidate, `~/.plane_api` by default), then clear the disk cache.

```
pbot configure
```

## pbot user

### pbot user list

List workspace members.

**Aliases** `ls`

```
pbot user list [--json]
```

## pbot cache

### pbot cache clear

Remove the local disk cache.

```
pbot cache clear
```

---

## pbot project

### pbot project list

List projects in the workspace.

**Aliases** `ls`

```
pbot project list [--json]
```

**Options**

| Option | Description |
|---|---|
| `--state <state>` | Filter by project state. |
| `--sort <field>` | Sort by `created` or `linear` (default `linear`). |
| `-l, --limit <n>` | Maximum results (default 50). |

### pbot project show

Show project details.

```
pbot project show <project> [--json]
```

### pbot project create

Create a project.

```
pbot project create <name> [-i <identifier>] [-d <description>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-i, --identifier <id>` | Short identifier (auto-generated from the name if omitted). |
| `-d, --description <text>` | Description. |

### pbot project update

Update a project.

```
pbot project update <project> [--name <name>] [-i <identifier>] [-d <description>] [--json]
```

### pbot project delete

Delete a project.

```
pbot project delete <project>
```

---

## pbot wi

Manage work items. **Aliases for the group:** `work-item`, `issues`, `issue`.

### pbot wi list

List work items. Without `-p`, lists across all projects.

```
pbot wi list [-p <project>] [--assignee <name|me>] [--state <s>] [--labels <a,b>]
             [--parent <id>] [--sort <field>] [-l <n>] [--json]
```

**Aliases** `ls`

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project name, identifier, or UUID. Omit to span all projects. |
| `--assignee <name\|me>` | Filter by assignee. |
| `--state <s>` | Filter by state name (comma-separated). |
| `--labels <a,b>` | Filter by label name (comma-separated). |
| `--parent <id>` | List child work items of a parent (`ABC-123`, UUID, or name). |
| `--sort <field>` | `created` (default) or `updated`. |
| `-l, --limit <n>` | Maximum results (default 50). |

**Examples**

```
$ pbot wi ls -p PLANECLI --state "In Progress" --assignee me --json
$ pbot wi ls -p PLANECLI --parent PLANECLI-38 --json
```

### pbot wi show

Show work item details. JSON includes `web_url`, `parent`, and `sub_issues`.

```
pbot wi show <issue> [-p <project>] [--no-comments] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project name/ID (required for name-based lookup). |
| `--no-comments` | Skip fetching comments. |

### pbot wi create

Create a work item.

**Aliases** `new`

```
pbot wi create <title> [-p <project>] [--assign <name|me>] [--state <s>]
              [--labels <a,b>] [--priority <p>] [--parent <id>]
              [-d <text> | --desc-md <markdown>] [--start-date <date>]
              [--target-date <date>] [-i <file>...] [--force] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project (required). |
| `--assignee, --assign <name\|me>` | Assignee. |
| `--state <s>` | State name (validated against the project). |
| `--labels <a,b>` | Comma-separated label names (validated against the project). |
| `--priority <p>` | `urgent`, `high`, `medium`, `low`, `none` (or `1`–`4`, `0`). |
| `--parent <id>` | Parent work item (`ABC-123`, UUID, or name). |
| `-d, --description <text>` | Description as plain text (wrapped in a paragraph). |
| `--desc-md <markdown>` | Description as native markdown (headings, lists, code, links). Mutually exclusive with `-d`. |
| `--start-date <YYYY-MM-DD>` | Start date (validated; verified by read-back). |
| `--target-date <YYYY-MM-DD>` | Target end date. |
| `-i, --image <file>` | Upload an image and embed it in the description (repeatable). |
| `--force` | Upload images even if an attachment with the same name exists. |

**Examples**

```
$ pbot wi create "Fix login" -p PLANECLI --assign me --state Todo --priority high --json
$ pbot wi create "Sub-task" -p PLANECLI --parent PLANECLI-9 --desc-md "# 背景

- 要点" --json
```

### pbot wi update

Update a work item. Date and parent writes are verified by read-back.

```
pbot wi update <issue> [-p <project>] [--state <s>] [--priority <p>]
              [--assign <name|me>] [--labels <a,b>] [--clear-labels]
              [--name <title>] [-d <text> | --desc-md <markdown>]
              [--start-date <date>] [--target-date <date>]
              [--parent <id>] [--clear-parent] [-i <file>...] [--force] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <name\|id>` | Project (needed for name-based lookup). |
| `--state <s>` | New state (validated, lists available on miss). |
| `--priority <p>` | New priority. |
| `--assignee, --assign <name\|me>` | New assignee. |
| `--labels <a,b>` | Set labels (comma-separated). |
| `--clear-labels` | Remove all labels. |
| `--name <title>` | New title. |
| `-d, --description <text>` | New description (plain text). |
| `--desc-md <markdown>` | New description as markdown. Mutually exclusive with `-d`. |
| `--start-date <date>` / `--target-date <date>` | New dates. |
| `--parent <id>` | Set the parent (rejects self, cross-project, and unknown parents locally). |
| `--clear-parent` | Remove the parent. |
| `-i, --image <file>` | Upload an image and append it to the description (repeatable). |
| `--force` | Allow a duplicate-named image upload. |

**Examples**

```
$ pbot wi update PLANECLI-40 --state "In Progress" --json
$ pbot wi update PLANECLI-40 --parent PLANECLI-38 --json
$ pbot wi update PLANECLI-40 --clear-parent --json
```

### pbot wi delete

Delete a work item.

```
pbot wi delete <issue> [-p <project>]
```

### pbot wi search

Search work items by text.

```
pbot wi search <query> [-p <project>] [-l <n>] [--json]
```

### pbot wi assign

Assign a work item (defaults to yourself).

```
pbot wi assign <issue> [--assign <name|me>] [-p <project>]
```

**See also** `pbot relations`, `pbot comment`

---

## pbot relations

Manage work-item relations. **Alias for the group:** `relation`.

Relationship types: `blocking`, `blocked_by`, `duplicate`, `relates_to`, `start_before`, `start_after`, `finish_before`, `finish_after`.

### pbot relations list

List the relations of a work item (all eight buckets).

**Aliases** `ls`

```
pbot relations list <issue> [-p <project>] [--json]
```

### pbot relations add

Create one or more relations from an issue to other issues.

```
pbot relations add <issue> --type <type> --to <target>... [-p <project>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--type <type>` | One of the eight relationship types. |
| `-t, --to <target>` | Target work item (`ABC-123`, UUID, or name); repeatable. |

**Examples**

```
$ pbot relations add PLANECLI-38 --type relates_to --to PLANECLI-40 -p PLANECLI --json
```

### pbot relations remove

**Not available yet** — the backend exposes no relation-deletion endpoint. The command fails with a validation error and a pointer to the backend request.

---

## pbot comment

### pbot comment list

List comments on a work item (oldest first).

**Aliases** `ls`

```
pbot comment list <issue> [-p <project>] [-l <n>] [--json]
```

### pbot comment create

Add a comment.

**Aliases** `new`

```
pbot comment create <issue> (-b <text> | --body-md <markdown> | --body-file <PATH> | --body-md-file <PATH>) [-p <project>] [--json]
```

Exactly one of `-b` / `--body-md` / `--body-file` / `--body-md-file` is required — passing two
fails at parse time (no request is sent). `--body-file` and `--body-md-file` accept a literal
path (`./-` stays a file named `-`); pass `-` to read the body from stdin (for example
`pbot wi show ABC-1 | pbot comment create ABC-2 --body -`). Use these for bodies past the
kernel single-argument cap (~130 KB).

**Options**

| Option | Description |
|---|---|
| `-b, --body <text>` | Comment text as plain text (paragraphs, `code`, bare URLs converted to HTML). |
| `--body-md <markdown>` | Comment text as native markdown. Mutually exclusive with the other body flags. |
| `--body-file <PATH>` | Read the plain-text body from a UTF-8 file (`-` reads stdin). Equivalent to `--body "$(cat PATH)"`. Mutually exclusive with the other body flags. |
| `--body-md-file <PATH>` | Read the markdown body from a UTF-8 file (`-` reads stdin). Equivalent to `--body-md "$(cat PATH)"`. Mutually exclusive with the other body flags. |

### pbot comment update

Update a comment.

```
pbot comment update <comment-id> --issue <issue> (-b <text> | --body-md <markdown> | --body-file <PATH> | --body-md-file <PATH>) [--json]
```

The four body flags are mutually exclusive (see `comment create`).

### pbot comment delete

Delete a comment.

```
pbot comment delete <comment-id> --issue <issue> [-p <project>]
```

---

## pbot label

### pbot label list

List labels in a project.

**Aliases** `ls`

```
pbot label list -p <project> [--json]
```

### pbot label show

Show label details.

```
pbot label show <label> -p <project> [--json]
```

### pbot label create

Create a label.

```
pbot label create <name> -p <project> [--color <#RRGGBB>] [--json]
```

### pbot label update

Update a label.

```
pbot label update <label> -p <project> [--name <name>] [--color <#RRGGBB>] [--json]
```

### pbot label delete

Delete a label.

```
pbot label delete <label> -p <project>
```

---

## pbot state

### pbot state list

List states in a project.

**Aliases** `ls`

```
pbot state list -p <project> [--group <group>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--group <group>` | Filter by group: `backlog`, `unstarted`, `started`, `completed`, `cancelled`. |

### pbot state show

Show state details.

```
pbot state show <state> -p <project> [--json]
```

### pbot state create

Create a state.

```
pbot state create <name> -p <project> [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state update

Update a state.

```
pbot state update <state> -p <project> [--name <name>] [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state delete

Delete a state.

```
pbot state delete <state> -p <project>
```

---

## pbot module

### pbot module list

List modules in a project.

**Aliases** `ls`

```
pbot module list -p <project> [--json]
```

### pbot module show

Show module details.

```
pbot module show <module> -p <project> [--json]
```

### pbot module create

Create a module.

```
pbot module create <name> -p <project> [-d <text>] [--start-date <date>]
                    [--end-date <date>] [--status <status>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--status <status>` | `backlog`, `planned`, `in-progress`, `paused`, `completed`, `cancelled`. |

### pbot module update

Update a module.

```
pbot module update <module> -p <project> [--name <name>] [-d <text>] [--status <status>]
                    [--start-date <date>] [--end-date <date>] [--json]
```

### pbot module delete

Delete a module.

```
pbot module delete <module> -p <project>
```

---

## pbot cycle

### pbot cycle list

List cycles in a project.

**Aliases** `ls`

```
pbot cycle list -p <project> [--json]
```

### pbot cycle show

Show cycle details.

```
pbot cycle show <cycle> -p <project> [--json]
```

### pbot cycle create

Create a cycle.

```
pbot cycle create <name> -p <project> [-d <text>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle update

Update a cycle.

```
pbot cycle update <cycle> -p <project> [--name <name>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle delete

Delete a cycle.

```
pbot cycle delete <cycle> -p <project>
```

### pbot cycle add-item

Add a work item to a cycle.

```
pbot cycle add-item <cycle> <issue> -p <project>
```

### pbot cycle remove-item

Remove a work item from a cycle.

```
pbot cycle remove-item <cycle> <issue> -p <project>
```

### pbot cycle items

List the work items of a cycle.

```
pbot cycle items <cycle> -p <project> [--json]
```

---

## pbot intake

### pbot intake list

List a project's intake queue. Each row carries the queue wrapper `id` and the work item `issue_id`.

**Aliases** `ls`

```
pbot intake list -p <project> [--json]
```

### pbot intake create

Create an intake item.

**Aliases** `new`

```
pbot intake create <name> -p <project> [-d <text>] [-P <priority>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-d, --description <text>` | Description (HTML-escaped, so tags show as text). |
| `-P, --priority <priority>` | Priority (`none`, `low`, `medium`, `high`, `urgent`). |

### pbot intake accept

Accept an intake item (triage). Requires the project Admin role; the CLI verifies the status actually changed and fails loudly otherwise.

```
pbot intake accept <issue-id> -p <project> [--json]
```

### pbot intake decline

Decline an intake item (triage). Requires the project Admin role.

```
pbot intake decline <issue-id> -p <project> [--json]
```

### pbot intake delete

Delete an intake item. **Destructive:** for any status other than `accepted`, this also deletes the underlying work item.

```
pbot intake delete <issue-id> -p <project>
```

### pbot intake enabled

Report whether intake is enabled for a project.

```
pbot intake enabled <project> [--json]
```

---

## pbot doc

Every `doc` subcommand needs `-p <project>`, because the v1 API serves pages
under a project only and there is no workspace-level pages route. A missing `-p`
on `list`, `create`, `update`, `archive`, or `delete` is rejected locally before
any request — exit 5 with a `Hint:` naming `-p <project>`. `show` is the one
exception: it attempts the lookup and turns the resulting 404 into the same
hint, so a page UUID that only exists in another scope still gets a clear answer.
This covers `--dry-run` as well: the `create` and `update` previews take the same
`-p` and are rejected the same way, where they previously printed a plan without it.

### pbot doc list

List documents. `-p` is required (project pages).

**Aliases** `ls`

```
pbot doc list -p <project> [--json]
```

### pbot doc show

Show document details.

**Aliases** `read`

```
pbot doc show <doc> [-p <project>] [--json]
```

`-p <project>` is required in practice: the v1 API serves project pages only,
so omitting it hits a workspace-level route the server does not provide. The
command then fails with the original 404 message plus a hint that names `-p` —
for example, `Error: Resource not found (HTTP 404).` followed by
``Hint: This server has no workspace-level pages route; pass `-p <project>` to look up a project page.``
(exit code 3). With `-p` the lookup, success output, and exit code are
unchanged.

### pbot doc create

Create a document.

**Aliases** `new`

```
pbot doc create --title <title> -p <project>
                (--content <markdown> | --content-md <markdown> | --content-raw <text> | --content-html <html>)
                [--dry-run] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--title <title>` | Page title (required). |
| `-c, --content <markdown>` | Content as Markdown (headings, lists, code, bold/italic, links, images — converted to HTML). Alias of `--content-md`. **BREAKING from v1.3.x**: a `# heading` line is now rendered as `<h1>heading</h1>` instead of literal text; pass `--content-raw` to keep the old plain-text behaviour. |
| `--content-md <markdown>` | Alias of `--content`; same path (markdown → HTML). |
| `--content-raw <text>` | Content as plain text: blank lines separate paragraphs, single newlines become `<br/>`. Markdown syntax (`#`, `-`, `*`, `[ ]`) is treated literally. This is what `--content` did before v1.4.0. |
| `--content-html <html>` | Content as raw HTML, stored verbatim (rich layout). |
| `-p, --project <name\|id>` | Project (required; omitting it is rejected locally with exit code 5). |
| `--dry-run` | Report the target and every image verdict without writing anything; exits non-zero when an image is missing or its MIME is not allowed. References the grammar rejects are listed under `unprocessed` and counted in the summary — they do not change the exit code. |

The four content inputs are mutually exclusive.

**Images in `--content` / `--content-md` need no extra flags.** A local path
(`./a.png` or `file://…`) is uploaded as a page asset and its source is replaced
with the asset id; a remote URL or an existing asset id is kept as-is. Images inside
fenced code blocks are left alone. A missing file, an unreadable file, or a MIME
outside `image/jpeg|png|webp|gif` fails the command (SVG is rejected by the server).

A `src` is one token with no spaces — the shape the Markdown converter turns into
`<img>` — so a local path that contains spaces has to be wrapped in angle
brackets, `![alt](<./a b.png>)`, and `%XX` escapes are decoded
(`file:///tmp/a%20b.png` finds `a b.png`). Anything else that looks like an image
but cannot be handled — a space without brackets, a `"title"` suffix, an empty
`src`, or an angle-wrapped non-local `src` — is not an error: the reference stays
literal Markdown on the page and `pbot` says so on stderr. `--dry-run` lists
those under `unprocessed` and counts them in its summary, so a report cannot read
as clean while the page is about to keep raw `![…](…)` text.

Both the warning and the report are **human-readable, not a machine contract**:
the warning is `warning: line N: <reference> was not uploaded — <reason>; it will
stay literal markdown on the page`, and the summary is
`summary: N to upload, N kept, N error(s), N unprocessed`. `--json` returns the
page object and does not carry either — scripts that must detect an unhandled
reference should parse the `--dry-run` summary or check the stored body.

`doc update --dry-run` uses the same target page and the same image grammar and
report as `doc create`.

**Examples**

```
$ pbot doc create --title "Runbook" -p PLANECLI --content-md "$(cat runbook.md)" --json
$ pbot doc create --title "Plain note" -p PLANECLI --content-raw "see # heading" --json
$ pbot doc create --title "Spec" -p PLANECLI --content-html "$(cat spec.html)" --json
$ pbot doc create --title "Spec" -p PLANECLI --content-md "$(cat spec.md)" --dry-run
```

### pbot doc update

Update a document.

```
pbot doc update <doc> -p <project> [--title <title>]
                     [--content <markdown> | --content-md <markdown> | --content-raw <text> | --content-html <html>]
                     [--dry-run] [--json]
```

**Options**

| Option | Description |
|---|---|
| `--title <title>` | New page title. |
| `-c, --content <markdown>` | New content as Markdown (alias of `--content-md`). Same BREAKING behaviour as `doc create --content`. |
| `--content-md <markdown>` | New content as Markdown (image handling as in `doc create`). |
| `--content-raw <text>` | New content as plain text (the v1.3.x `--content` behaviour). |
| `--content-html <html>` | New content as raw HTML, stored verbatim. |
| `-p, --project <name\|id>` | Project (required; omitting it is rejected locally with exit code 5). |
| `--dry-run` | Report the target and every image verdict without writing anything; exits 5 when an image cannot be processed. |

### pbot doc archive

Archive (trash) a page without deleting it. The page stays recoverable in the web UI trash.

```
pbot doc archive <doc> -p <project>
```

### pbot doc delete

Delete a page. The API only deletes archived pages, so this archives the page first (verifying the archive landed) and then deletes it.

```
pbot doc delete <doc> -p <project>
```

---

## pbot attachment

**Alias for the group:** `attachments`.

### pbot attachment attach

Upload a file attachment to a work item (three-step presigned upload, verified by read-back).

**Aliases** `upload`, `new`

```
pbot attachment attach <issue> -f <file> [-p <project>] [--force] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-f, --file <path>` | File to upload. |
| `--force` | Upload even if an attachment with the same name already exists. |

### pbot attachment list

List attachments on a work item.

**Aliases** `ls`

```
pbot attachment list <issue> [-p <project>] [--json]
```

---

## pbot tc

The **test cases** of a project. A case is named by its number (`TC-<module code>-<NNN>`), and the
**server assigns that number** — the CLI never builds one. The module code is the prefix of the
number, so it is registered first (`pbot tc module create`); the server refuses a case whose
module is not registered in the project.

A case carries **six mandatory header fields** — title, purpose, object overview, priority,
preconditions, environment — and **at least one step**. The CLI asks for all six, so its `--help`
tells the truth about what the server refuses.

Every read answers `--json` on stdout and a table on stderr; every write prints the number the
server assigned. A refused write prints the **stable code** the server answered with
(`invalid_test_case`, `module_code_format`, `module_code_taken`, `expected_is_unjudgeable`, …)
together with the sentence of the server, one problem per line, each with the case and the step it
belongs to. **Key on the code, never on the prose** — the code is the first word of the first line
of the error, and it is the part the server promises to keep.

### pbot tc list

List the test cases of a project.

**Aliases** `ls`

```
pbot tc list -p <project> [--module <code>] [--priority <P>] [--issue <work item>]
                  [--status <active|all>] [--search <text>] [-l <limit>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <project>` | Project name, identifier, or UUID (required). |
| `--module <module>` | Filter by the module code, or the UUID of a module. |
| `--priority <priority>` | Filter by priority: `P0`, `P1`, `P2`, `P3`. |
| `--issue <issue>` | Filter by a linked work item (identifier or UUID). |
| `--status <status>` | Filter by status: `active` (the default of the server) or `all`. |
| `--search <text>` | Filter by the number or the title of a case. |
| `-l, --limit <limit>` | Maximum results to show (default 50). |

### pbot tc show

Show one test case: its header fields, its steps and its work-item links.

```
pbot tc show <case> -p <project> [--json]
```

`<case>` is the number (`TC-CLI-001`), a UUID, or the title.

### pbot tc create

Create a test case. The number is assigned by the server.

```
pbot tc create -p <project> --module <code> --title <title> --purpose <purpose>
                  --object-overview <text> --priority <P> --precondition <text>
                  --environment <text> [--related-feature <text>] [--remarks <text>]
                  [--status <active|void>] [--step "action | expected result"]...
                  [--steps-file <path>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <project>` | Project name, identifier, or UUID (required). |
| `--module <module>` | Module code of the case (the prefix of its number). |
| `--title <title>` | Title of the case. **Required** by the server. |
| `--purpose <purpose>` | What the case checks. **Required** by the server. |
| `--object-overview <text>` | What is under test. **Required** by the server. |
| `--priority <priority>` | `P0`, `P1`, `P2`, `P3`. **Required** by the server. |
| `--precondition <text>` | What has to hold before the case runs. **Required** by the server. Alias `--preconditions`. |
| `--environment <text>` | The environment and the version under test. **Required** by the server. Alias `--env`. |
| `--related-feature <text>` | The feature the case belongs to. |
| `--remarks <text>` | Free-form remarks. |
| `--status <status>` | `active` (the default) or `void`. |
| `--step "action \| expected result"` | One step (repeatable). |
| `--steps-file <path>` | A file with the whole step table. |

**The step format is `action | expected result`** — an action, a vertical bar, an expected
result, on one line. Both halves are required; a line without a bar, or with an empty half, is
refused locally (exit 5) before any request.

`--steps-file` takes the whole table in one of two shapes:

- a **JSON array** of steps — `[{"action": "Open the page", "expected": "The list is shown"}]`
  (`expected_result` is accepted for the second key as well);
- **plain lines**, one `action | expected result` per line. Blank lines and lines starting with
  `#` are skipped.

Both shapes feed the same table. The table is the **whole** table: a create writes every step it
was given, and a later `tc steps` with steps replaces the table as a whole (see below).

**Example**

```bash
pbot tc create -p PLANECLI --module CLI \
  --title "Log in with a valid account" \
  --purpose "Check the login path of the web UI" \
  --object-overview "The login form of the web UI" \
  --priority P1 \
  --precondition "The account exists and is active" \
  --environment "Windows 10, Chrome 120" \
  --step "Open the login page | The form is shown" \
  --step "Log in with the valid account | The dashboard is shown" \
  --json
```

### pbot tc update

Update a test case. Every field is optional; an unset field is left alone.

```
pbot tc update <case> -p <project> [--title <title>] [--purpose <purpose>]
                  [--object-overview <text>] [--priority <P>] [--precondition <text>]
                  [--environment <text>] [--related-feature <text>] [--remarks <text>]
                  [--status <active|void>] [--step "action | expected result"]...
                  [--steps-file <path>] [--json]
```

**The number and the module of a case cannot be changed.** The number is bound to the module and
is part of the record of every run, and the server keeps both. `--module` and `--number` exist so
the refusal names the field the caller tried to change; either one is refused locally (exit 5)
before any request, so nothing has to be undone.

`--step` / `--steps-file` replace the **whole** step table: every step has to be passed again, the
server drops the rows that are not in the list.

### pbot tc steps

Read or replace the step table of a test case.

```
pbot tc steps <case> -p <project> [--step "action | expected result"]...
                  [--steps-file <path>] [--json]
```

Without a step the table is read; with steps the **whole** table is replaced.

> **The table of the steps is printed, not the number of the case.** `pbot tc steps TC-CLI-001`
> prints the rows of the table — a `Step`, `Action` and `Expected result` table on stderr, and the
> same rows as a JSON array on stdout — and **not** the `TC-CLI-001` number. The number of the
> case is what `pbot tc show` prints; the table of the steps is what `pbot tc steps` prints.

### pbot tc issues

List, add or remove the work-item links of a test case.

```
pbot tc issues <case> -p <project> [-a <work item>] [-r <work item>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-a, --add <work item>` | Link a work item (identifier or UUID). |
| `-r, --remove <work item>` | Unlink a work item (identifier or UUID). |

`--add` and `--remove` are mutually exclusive. Without either the links are listed.

### pbot tc delete

Delete a test case. **Destructive:** the server only lets a project admin remove a case, and the
refusal carries the server's own sentence.

```
pbot tc delete <case> -p <project> [--json]
```

### pbot tc module list

List the registered module codes of a project.

**Aliases** `ls`

```
pbot tc module list -p <project> [--json]
```

### pbot tc module create

Register a module code. The server owns the rules of a code: 2 to 6 uppercase letters, unique
inside the project. A refused code is reported with the server's own code (`module_code_format`,
`module_code_taken`) and its sentence.

```
pbot tc module create -p <project> --code <code> [-d <description>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <project>` | Project name, identifier, or UUID (required). |
| `--code <code>` | The module code, for example `CLI`. |
| `-d, --description <text>` | What the module covers. |

---

## pbot run

The **test runs (batches)** of a project. A batch is one continuous execution of test cases. It is
named by its number (`RUN-<YYYYMMDD>-<NN>`), and the **server assigns that number** — the CLI
never builds one and never writes one.

Two rules of the feature shape this whole group:

- A **verdict** (the verdict of a step) and a **conclusion** (the verdict of a case) are written
  **once**. The server refuses a second write with `conclusion_is_final`, and a wrong one is
  corrected by voiding the record and running the case again in a new batch. So both writes ask
  before they write, and **no flag of this group overwrites a verdict that is already recorded**.
- A **completed** batch is frozen: every write to it is refused with `run_completed`, and the CLI
  never retries such a refusal on its own.

### pbot run list

List the batches of a project.

**Aliases** `ls`

```
pbot run list -p <project> [--status <status>] [--run-date <YYYY-MM-DD>] [-l <limit>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <project>` | Project name, identifier, or UUID (required). |
| `--status <status>` | Filter by status: `draft`, `in_progress`, `completed`, or `all`. |
| `--run-date <date>` | Filter by the day of the execution (`YYYY-MM-DD`). |
| `-l, --limit <limit>` | Maximum results to show (default 50). |

### pbot run show

Show one batch: its header, its cases, their steps and their files.

```
pbot run show <run> -p <project> [--json]
```

`<run>` is the number (`RUN-20261109-01`) or a UUID.

### pbot run create

Create a batch. The number is assigned by the server.

```
pbot run create -p <project> --version <version> --build <build> --commit <commit>
                  --environment <env> [--run-date <YYYY-MM-DD>] [--retest-of <run>]
                  [--executed-by <who>] [--json]
```

**Options**

| Option | Description |
|---|---|
| `-p, --project <project>` | Project name, identifier, or UUID (required). |
| `--version <version>` | The version under test (frozen once the batch is created). |
| `--build <build>` | The build under test (frozen once the batch is created). |
| `--commit <commit>` | The commit under test (frozen once the batch is created). |
| `--environment <env>` | The environment the batch ran in. Alias `--env`. |
| `--run-date <YYYY-MM-DD>` | The day of the execution (default: today, in **UTC**). |
| `--retest-of <run>` | The batch this one re-tests (number or UUID). |
| `--executed-by <who>` | Who executed the batch: `me` (the default), a name, or a UUID. |

`--version`, `--build`, `--commit` and `--environment` are asked for because the server refuses to
close a batch that does not carry them, and this group has no command that fills them in later.

**`--executed-by` defaults to `me`**, which under a service-principal token resolves to the
principal's shadow bot member — the same `me` the rest of the CLI uses. The batch needs an
executor: a batch without one cannot be closed, and the server refuses the finalize with
`run_executed_by_missing`.

**`--run-date` is UTC, and it is frozen after the creation.** The default is *today in UTC*, not
today where the operator sits: an operator at UTC+8 who creates a batch between 00:00 and 08:00
local time gets the **previous UTC day** in `run_date`, and the number of the batch carries that
day too (`RUN-<YYYYMMDD>-<NN>`). Pass `--run-date` explicitly when the day has to be the local
one. The day of a batch cannot be changed afterwards — the server refuses the write with
`run_date_fixed`.

**`--retest-of` links the batch to the batch it re-tests.** A re-test runs in the project of its
original and never in a ring: the server refuses a link into another project
(`retest_of_project_mismatch`), a link into itself (`retest_of_self`), and a link that would close
a ring (`retest_of_cycle`). The listing shows the link in the `Retest of` column as
`retest of RUN-20261109-01` — **in English**, unlike the web UI, which writes 补测.

### pbot run add

Add test cases to a batch.

```
pbot run add <run> <case>... -p <project> [--json]
```

The server **snapshots** every case that is added, so a later edit of the case never rewrites what
the batch recorded. A case that is already in the batch is refused with `duplicate_case_number`;
the CLI reads the batch first and refuses the duplicate before the request, naming the case.

### pbot run step

Write the verdict and the measured value of one step.

```
pbot run step <run> --case <case> --step <n> --verdict <verdict>
                  [--actual <markdown>] [--actual-file <path>] [--image <path>]
                  [--yes] -p <project> [--json]
```

**Options**

| Option | Description |
|---|---|
| `--case <case>` | The case of the batch (`TC-CLI-007`). |
| `--step <n>` | The number of the step in the case (1, 2, 3 …). |
| `--verdict <verdict>` | `pass`, `fail` or `unset` — a step that was not executed is `unset`. `N/A` is a conclusion of a case, not a verdict of a step, and is refused locally. |
| `--actual <markdown>` | The measured value. |
| `--actual-file <path>` | A file with the measured value. |
| `--image <path>` | A screenshot of the step. |
| `-y, --yes` | Confirm the write (needed when the command is not on a terminal). |

**The measured value reaches the server as it was typed — no conversion, no rewriting.** `--actual`
sends the string verbatim; `--actual-file` sends the bytes of the file verbatim, newlines and
all. Neither is Markdown-converted on the way: what the file holds is what the server stores.

**`--yes` is required off a terminal.** The verdict of a step is written once, so the command asks
before it writes: on a terminal it prompts on stderr (`… — write it? [y/N]`), and a script that is
not on a terminal has to pass `--yes`. Without `--yes` and without a terminal the command refuses
before any request (exit 5) and nothing is written.

**A screenshot of a step does not satisfy the finalize.** The evidence of a case hangs on the case
itself — see [the evidence rule](#the-evidence-rule-of-a-case) below.

### pbot run conclude

Write the conclusion of one case of a batch.

```
pbot run conclude <run> --case <case> --conclusion <conclusion>
                      [--evidence <markdown>] [--evidence-file <path>] [--image <path>]
                      [--remarks <text>] [--actual-result-summary <text>]
                      [--na-reason <text>] [--na-criteria-source <text>]
                      [--manual-executor <who>] [--manual-condition <text>]
                      [--yes] -p <project> [--json]
```

**Options**

| Option | Description |
|---|---|
| `--case <case>` | The case of the batch (`TC-CLI-007`). |
| `--conclusion <conclusion>` | `PASS`, `FAIL`, `N/A` or `MANUAL` (`NA` is accepted and normalized to `N/A`). |
| `--evidence <markdown>` | The evidence of the case. |
| `--evidence-file <path>` | A file with the evidence of the case (the bytes reach the server verbatim). |
| `--image <path>` | A screenshot of the case — the evidence of a case on its own is enough. |
| `--remarks <text>` | Free-form remarks. |
| `--actual-result-summary <text>` | The one-line phenomenon of the case (the summary shows it). |
| `--na-reason <text>` | Why the case does not apply (a `N/A` case needs it). |
| `--na-criteria-source <text>` | Where the criterion of a `N/A` case comes from. |
| `--manual-executor <who>` | Who ran the case by hand (a `MANUAL` case needs it). |
| `--manual-condition <text>` | Under which condition the case was run by hand. |
| `-y, --yes` | Confirm the write (needed when the command is not on a terminal). |

The conclusion of a case is written **once**: the server refuses a second write with
`conclusion_is_final`, and there is **no flag that overwrites it**. A wrong conclusion is
corrected by voiding the record and running the case again in a new batch
(`pbot run create --retest-of <run>`).

**Which code the refusal carries depends on the order.** A second write on an **open** batch is
refused with `conclusion_is_final`; the same write **after** the finalize is refused with
`run_completed`, because a completed batch is frozen and the freeze is checked first. Read the
code, not the order you expected.

### pbot run remove

Take a case out of a batch.

```
pbot run remove <run> --case <case> -p <project> [--json]
```

A record that already carries a verdict or a conclusion stays in the batch: the server refuses the
removal with `run_case_removal_after_execution`.

### pbot run finalize

Close a batch: the server runs the validity rules over it.

```
pbot run finalize <run> -p <project> [--json]
```

A refused finalize lists **every** problem it found, one per line, each with the case and the step
it belongs to:

```
Error: invalid_batch
conclusion_invalid (case TC-CLI-001): 结论必须是 PASS、FAIL、N/A 或需人工。
evidence_missing (case TC-CLI-001): 缺少必填项：证据。
Hint: Every reason above names the case and the step it belongs to; fix them and finalize again. …
```

The batch stays open until every problem is fixed. A batch that is closed is **frozen** — every
later write to it is refused with `run_completed`.

### pbot run summary

The summary of a batch: the four counts and the deviations.

```
pbot run summary <run> -p <project> [--json]
```

**The four counts are not a partition of the total.** A case that failed on one step and was never
executed on another is counted twice, so `fail` and `manual` may both count the same case: a batch
with one concluded `FAIL` case reports `total: 1`, `fail: 1`, `manual: 1`. Do not add the counts
up and compare the sum with `total`.

---

## The evidence rule of a case

The server refuses to close a batch whose cases carry no evidence, and it counts **case-level**
evidence only: a non-empty **case-level text** **or** at least one **case-level attachment**. A
screenshot of a **step** is bound to the step record, so it does **not** count — the finalize
answers `evidence_missing` with the path `cases[TC-CLI-001].evidence` and names the case.

The two ways to write the evidence of a case are `--evidence` / `--evidence-file` (a text) and
`--image` (a file); either one on its own is enough:

```bash
# a text
pbot run conclude RUN-20261109-01 --case TC-CLI-007 --conclusion FAIL --evidence "the button stayed grey" --yes
# a screenshot, and nothing else
pbot run conclude RUN-20261109-01 --case TC-CLI-007 --conclusion FAIL --image ./shot.png --yes
```

The CLI reads the text the way the server reads it — a value that holds nothing but a placeholder
(`—`, `TBD`, `待补充`, …) carries no text, and a value that holds nothing but a picture
(`![](shot.png)`) carries none either, because a picture is a file and not a sentence. The notice
the CLI prints is **warn-only**: it never refuses a write, and the server still refuses the batch
and names the case.

### The residue: shapes the two reads do not agree on

The local pre-check and the server do not read every shape of Markdown the same way. The
difference is always **warn-only** — the CLI never refuses a write over it, and the server still
decides — but a tester who knows the shapes is not surprised by a notice the server does not
justify, or by a refusal the CLI did not warn about.

**Shapes the CLI reads as text and the server reads as no text** (the CLI stays quiet, the server
refuses the finalize): block markup around a placeholder — `> TBD`, `# TBD`, `- TBD`, `1. TBD`,
`* TBD` — a placeholder inside a link — `[TBD](url)` — `~~TBD~~`, `&nbsp;TBD`, a fenced block with
an info string — ` ```text TBD ``` ` — `~~~~` fences, and a link reference definition —
`[ref]: shot.png`.

**Shapes the CLI reads as no text and the server reads as text** (the CLI warns, the server
accepts): an unclosed emphasis — `**TBD`, `***TBD`, `TBD**`, `** ![](x.png) **`.

### The orphan upload

`--image` **uploads and binds the file before the write is made**, so a write the server refuses
can leave an attachment bound to the record — the file is on the record, the verdict is not. It
never changes a verdict: the refused write stays refused, and the record keeps the verdict it
already had. The file can be written again on the next attempt; the conclusion cannot.

---

## SEE ALSO

- `pbot <command> <subcommand> --help` — per-command help generated from the CLI definition.
- Plane v1 API reference: `docs/api/plane-v1-api.md`.
