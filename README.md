# pbotcli (pbot)

`pbot` is an independent command-line client for [Plane.so](https://plane.so) (SaaS or
self-hosted): projects, work items, cycles, modules, documents, labels, states, intake queues,
attachments, and comments.

Its defining feature is **fuzzy resource resolution**: anywhere a resource is expected you can
pass a name, an identifier (`ABC-123`), or a UUID. Close names resolve on their own —
`pbot wi ls -p "Front"` finds `Frontend`.

This repository is `pbotcli`: a Cargo workspace under `crates/`. The Python implementation that
used to ship in this repository has been removed; the docs in `docs/` that describe it are kept
as historical reference.

## Install

Pre-built binaries and installers are published on the
[Releases page](https://github.com/Liewzheng/pbotcli/releases). Pick whichever
form fits your platform — the shell installer is the same artifact on Linux and
macOS and is the fastest way to try the CLI; the GUI installers (`dmg` / `msi`)
are there for users who want a double-click install.

### Homebrew (macOS / Linux)

The tap at [`Liewzheng/homebrew-tap`](https://github.com/Liewzheng/homebrew-tap)
publishes the CLI as a Homebrew formula called `pbotcli`:

```sh
brew tap Liewzheng/tap
brew install pbotcli
```

The formula is rebuilt from the release tarball at every tagged release by the
`publish-homebrew-formula` job in `release.yml`; you get the same `pbotcli` and
`pbot` binaries the other installers ship, installed into the standard
Homebrew prefix.

### macOS

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Liewzheng/pbotcli/releases/latest/download/pbotcli-cli-installer.sh | sh
```

Or download a `.dmg` from the latest release page (Apple Silicon: `pbotcli-aarch64-apple-darwin.dmg`;
Intel: `pbotcli-x86_64-apple-darwin.dmg`), open it, and drag `pbotcli` and `pbot` into a
directory on your `$PATH` — common choices are `~/bin` (user-local, no sudo needed),
`/opt/homebrew/bin` (Homebrew on Apple Silicon), or `/usr/local/bin` (system-wide; on newer
macOS this directory may require `sudo` to write to).

### Linux

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Liewzheng/pbotcli/releases/latest/download/pbotcli-cli-installer.sh | sh
```

Or download a `.deb` from the latest release page and install it with your
package manager — on Debian / Ubuntu (x86_64):

```sh
apt install ./pbotcli-cli_<version>_amd64.deb      # or sudo apt install
```

The arm64 build is `pbotcli-cli_<version>_arm64.deb` on aarch64 hosts;
`<version>` is the release's version number (e.g. `1.5.0`). Both are built by
the `build-linux-deb` job in `release.yml` — natively on `ubuntu-22.04`
(amd64) and `ubuntu-22.04-arm` (arm64), so no cross-compilation is involved.
Each `.deb` ships both the `pbotcli` binary and the `pbot` binary in
`/usr/bin/` — they are the two `[[bin]]` targets in
`crates/pbotcli-cli/Cargo.toml`, and `pbot` is a short alias of the same CLI,
not a symlink.

### Windows

```powershell
irm https://github.com/Liewzheng/pbotcli/releases/latest/download/pbotcli-cli-installer.ps1 | iex
```

Or download `pbotcli-cli-<version>-x86_64.msi` from the latest release page and
double-click to install through the standard Windows installer UI. As an
alternative, download `pbotcli-cli-x86_64-pc-windows-msvc.zip` and unzip it
somewhere on your `%PATH%`. ARM64 Windows is not currently a release target —
use the shell installer if you're on such a device.

### From a checkout

The CLI is not published to crates.io yet. A build from a checkout is a **test build** — use it to
exercise a branch, then remove it with `cargo uninstall pbotcli-cli`:

```bash
cargo install --path crates/pbotcli-cli --locked
```

That installs both binaries into `~/.cargo/bin`:

| Binary | Notes |
|---|---|
| `pbotcli` | Full package name. |
| `pbot` | Short-named binary built from the same `pbotcli-cli` package; used in every example here. |

The durable install is the Homebrew tap above — `brew install pbotcli`, then `brew upgrade pbotcli`;
never leave a test build on `PATH` beside the released `pbotcli`.

## Configure

Credentials come from environment variables or a config file discovered from
several locations, highest priority first: `~/.config/pbot/config.toml` (TOML),
`~/.pbot`, `~/.planecli`, then the legacy `~/.plane_api` (lowercase `key=value` lines):

```ini
base_url=https://api.plane.so
api_key=your-personal-access-token
workspace=your-workspace-slug
```

`pbot configure` writes the active config file interactively (the highest-priority
existing candidate, `~/.plane_api` by default, `chmod 600`); the TOML file
takes the same keys at the top level or under `[auth]`. Exporting
`PLANE_BASE_URL` / `PLANE_API_KEY` / `PLANE_WORKSPACE` also works. Precedence is
**flags > environment variables > config file**, per setting. `base_url` is your instance
(`https://api.plane.so` for SaaS); `workspace` is the workspace slug, not the instance name.

## Quick start

```bash
pbot whoami                          # the authenticated user
pbot project ls                      # projects in the workspace
pbot wi ls -p "Frontend"             # work items, resolved by name
pbot wi create "Fix login timeout" -p "Frontend" --assignee me --priority urgent
pbot wi show ABC-123                 # by identifier, UUID, or name
```

Every command takes `--help`; `docs/cli-command-reference.md` lists every flag, alias, and
example.

## Test cases

`pbot tc` manages the **test cases** of a project, `pbot run` manages the **batches** that
execute them. A case is named by its number (`TC-<module code>-<NNN>`) and a batch by its number
(`RUN-<YYYYMMDD>-<NN>`) — **the server assigns both numbers**, the CLI never builds one.

Two rules shape the whole feature: a verdict (of a step) and a conclusion (of a case) are written
**once** — the server refuses a second write with `conclusion_is_final`, and there is **no flag
that overwrites a verdict** — and a **completed** batch is **frozen**: every later write to it is
refused with `run_completed`.

The end-to-end flow, start to finish:

```bash
# 1. register the module code — it is the prefix of the case number
pbot tc module create -p PLANECLI --code CLI --description "The command line" --json

# 2. create the case: six mandatory header fields and at least one step
#    (a step is written as "action | expected result")
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

# 3. create the batch — --executed-by defaults to "me"
pbot run create -p PLANECLI --version 1.6.0 --build 42 --commit abc1234 \
  --environment bench --json

# 4. add the case to the batch (RUN-20261112-01 is the number the server assigned)
pbot run add RUN-20261112-01 TC-CLI-001 -p PLANECLI --json

# 5. write the verdict of step 2 with a screenshot
pbot run step RUN-20261112-01 --case TC-CLI-001 --step 2 --verdict fail \
  --image ./shot.png --yes -p PLANECLI --json

# 6. write the conclusion of the case — with evidence ON THE CASE
pbot run conclude RUN-20261112-01 --case TC-CLI-001 --conclusion FAIL \
  --evidence "the button stayed grey" --yes -p PLANECLI --json

# 7. a second conclusion write is refused — the code says why
pbot run conclude RUN-20261112-01 --case TC-CLI-001 --conclusion PASS --yes -p PLANECLI
# Error: invalid_payload
# conclusion_is_final (field conclusion): 结论一经写入不再修改，请作废该记录后在新批次中重测。

# 8. close the batch
pbot run finalize RUN-20261112-01 -p PLANECLI --json
```

Four details the flow above depends on:

- **`--yes` is required off a terminal.** A verdict and a conclusion are written once, so both
  commands ask before they write: on a terminal they prompt on stderr, and a script that is not on
  a terminal has to pass `--yes`. Without it the command refuses before any request and nothing
  is written.
- **The evidence hangs on the case, not on the step.** The server counts case-level evidence only:
  a case-level text (`--evidence` / `--evidence-file`) **or** a case-level file (`--image`).
  A screenshot of a **step** does not count — the finalize answers `evidence_missing` with the
  path `cases[TC-CLI-001].evidence` and names the case. The screenshot of step 2 above is
  therefore *not* enough; step 6 has to carry the evidence of the case.
- **A refused write prints the stable code of the server** (`conclusion_is_final`,
  `evidence_missing`, `run_executed_by_missing`, `duplicate_case_number`, …) next to the sentence
  of the server. Key on the code, never on the prose — it is the first word of the first line of
  the error.
- **The order of the two refusals matters.** A second conclusion write on an **open** batch is
  refused with `conclusion_is_final`; the same write **after** the finalize is refused with
  `run_completed`, because a completed batch is frozen and the freeze is checked first.

The exit codes **of this feature** — the two groups have their own mapping, so a permission
failure and a coded refusal are classes of their own: `0` success, `2` authentication **or**
permission **or** a usage error, `3` not found, `4` API or transport failure, `5` validation —
both the local checks and every server refusal that carries a stable code. The other commands
follow the general table of the reference, where a 403 is exit 4.

`docs/cli-command-reference.md` has every command and flag of both groups, including the
evidence rule, the edges of `--run-date` (UTC), and the shapes of Markdown the local pre-check and
the server do not read the same way.

## Output

Tables go to **stderr**, JSON to **stdout**, so a pipe always carries JSON alone:

```bash
pbot wi ls -p "Frontend" --json 2>/dev/null | jq '.[].name'
```

`--json 2>/dev/null` is the pattern for scripting; `--no-cache` bypasses the disk cache for one
command, and `pbot cache clear` empties it.

## Documentation

- [docs/README.md](docs/README.md) — documentation index and key concepts
- [docs/cli-command-reference.md](docs/cli-command-reference.md) — every command and flag
  ([中文](docs/cli-command-reference.zh.md))
- [AGENTS.md](AGENTS.md) — conventions and gotchas for contributors
- [skill/](skill/) — a portable skill that teaches AI coding agents to drive `pbot`

## License

MIT — see [LICENSE](LICENSE).
