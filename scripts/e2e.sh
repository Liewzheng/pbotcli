#!/usr/bin/env bash
# Live end-to-end smoke for pbot against a real Plane instance.
# Covers: create → markdown comment → state change → attachment upload →
# inline image embed → test case → test run → cleanup. Requires ~/.plane_api
# credentials and a reachable instance; paces calls to stay under the instance
# rate limit.
#
# Needs: bash (3.2 is enough), python3, jq, awk, and the credentials of a
# real instance.
#
# Usage: scripts/e2e.sh [-p PROJECT]   (default project: PLANECLI)

set -euo pipefail

PROJECT="${1:-PLANECLI}"
PBOT="${PBOT:-pbot}"
PAUSE="${PAUSE:-4}"          # seconds between calls (rate-limit buffer)
MARK="pbot-e2e-$$"           # unique marker for cleanup
FAILURES=0
# The scratch file holds the error of the last command, so a failure says
# why it failed instead of only naming the step.
ERR_FILE="$(mktemp)"
# The generated images live in a directory of their own: the name of the
# file has to end in .png (the CLI reads the type of an image from its
# extension), and a predictable path in a shared /tmp is not worth the risk.
TMP_ROOT="${TMPDIR:-/tmp}"
TMP_DIR="$(mktemp -d "$TMP_ROOT/${MARK}-XXXXXX")"
# The cleanup is installed right here, so a failure between the two temp
# files and the trap cannot leave them behind.
trap cleanup EXIT

say()  { printf '\n== %s\n' "$*"; }
ok()   { printf '   ok: %s\n' "$*"; }
skip() { printf '   skip: %s\n' "$*"; }
die()  { printf '   FAIL: %s\n' "$*" >&2; FAILURES=$((FAILURES + 1)); }
# The summary of the run, and the exit code that goes with it.
result() {
  say "result"
  if [[ "$FAILURES" -eq 0 ]]; then echo "E2E PASS"; else echo "E2E FAIL ($FAILURES)"; exit 1; fi
}
# A failure that makes the rest of the run meaningless: the run ends
# there, with the summary — and with the cleanup, which is a trap.
fatal() { die "$*"; result; }

cleanup() {
  say "cleanup $MARK"
  "$PBOT" wi ls -p "$PROJECT" --no-cache --json 2>/dev/null \
    | jq -r '.[] | select(.name | startswith("pbot-e2e")) | .id' \
    | while read -r id; do
        "$PBOT" wi delete "$id" -p "$PROJECT" >/dev/null 2>&1 || true
        sleep "$PAUSE"
      done
  rm -f "$ERR_FILE"
  rm -rf "$TMP_DIR"
}

export http_proxy="${http_proxy:-}" https_proxy="${https_proxy:-}" all_proxy="${all_proxy:-}"
# Use the correct proxy if the env one is malformed (double-encoded).
if [[ "${https_proxy:-}" == *"%3A"* ]]; then
  export https_proxy="http://192.168.1.180:7890" http_proxy="http://192.168.1.180:7890"
fi

say "whoami"
out="$($PBOT whoami --json 2>/dev/null)" && jq -e .id >/dev/null <<<"$out" \
  && ok "whoami" || die "whoami"

say "create work item"
sleep "$PAUSE"
created="$($PBOT wi create "pbot-e2e $MARK" -p "$PROJECT" --state Todo \
  --labels "" --desc-md "# $MARK"$'\n\n''- a'$'\n''- b' --json 2>/dev/null)"
id="$(jq -r .id <<<"$created")"
[[ -n "$id" && "$id" != "null" ]] && ok "created $id" || { die "create"; exit 1; }
sleep "$PAUSE"

say "markdown description landed"
desc="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null)"
grep -q '<h1>' <<<"$(jq -r .description_html <<<"$desc")" && ok "desc md h1" || die "desc md"

say "comment (--body-md)"
sleep "$PAUSE"
$PBOT comment create "$id" --body-md "## 进度"$'\n\n'"正文" -p "$PROJECT" --json >/dev/null 2>&1 \
  && ok "comment" || die "comment"

say "state change -> In Progress"
sleep "$PAUSE"
$PBOT wi update "$id" -p "$PROJECT" --state "In Progress" --json >/dev/null 2>&1
st="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null | jq -r .state_detail_name)"
[[ "$st" == "In Progress" ]] && ok "state=$st" || die "state=$st"

say "attachment upload"
png="$TMP_DIR/shot.png"
python3 -c "import base64,sys;open(sys.argv[1],'wb').write(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))" "$png"
sleep "$PAUSE"
$PBOT attachment attach "$id" -f "$png" -p "$PROJECT" --json >/dev/null 2>&1 && ok "attach" || die "attach"

say "inline image (-i) on update"
sleep "$PAUSE"
# A file of its own name: the CLI refuses a duplicate attachment name on
# purpose, and this step is not about that refusal.
inline_png="$TMP_DIR/inline.png"
cp "$png" "$inline_png"
$PBOT wi update "$id" -p "$PROJECT" -i "$inline_png" --json >/dev/null 2>&1 \
  && ok "inline img" || die "inline img"
html="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null | jq -r .description_html)"
grep -q '<img ' <<<"$html" && ok "inline img in the description" || die "inline img"
# The files stay until the end of the run: the test-case block copies the
# same image, and the cleanup removes the whole directory anyway.

say "verify attachment uploaded"
sleep "$PAUSE"
up="$($PBOT attachment ls "$id" -p "$PROJECT" --no-cache --json 2>/dev/null | jq '[.[] | select(.is_uploaded==true)] | length')"
[[ "$up" -ge 2 ]] && ok "attachments uploaded=$up" || die "attachments uploaded=$up"

# --- the test-case feature -------------------------------------------------
#
# A case and a batch are the two halves of the feature. The instance has to
# serve them; an instance without the endpoints skips the whole block with a
# clear message instead of failing the smoke.
#
# The batch is addressed by its **uuid**: the number of a batch is resolved by
# reading the list of the batches, and a list that carries a batch without an
# executor cannot be deserialized — the uuid skips that read entirely.
#
# The module code is 2 to 6 UPPERCASE letters, so the pid and a random number
# are spelled out in letters: two smoke runs never fight over the same code.
# The sum is taken modulo 26^4 (456976), so the code never grows past four
# letters (plus the E), and the padding keeps it away from the two-letter
# end of the rule whatever the pid of the run is. The number is written out
# rather than computed with `**`: the script has to run on the bash 3.2
# that macOS ships.
letters_of() {
  local n=$1 out="" alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZ"
  while [[ $n -gt 0 && ${#out} -lt 4 ]]; do
    out="${alphabet:$(( n % 26)):1}$out"
    n=$(( n / 26 ))
  done
  printf '%s' "${out:-A}"
}
seed_letters="$(letters_of $(( ($$ + RANDOM) % 456976 )))"
while [[ ${#seed_letters} -lt 3 ]]; do seed_letters="A$seed_letters"; done
MODULE_CODE="E$seed_letters"
# The version under test is the version of the binary under test.
PBOT_VERSION="$("$PBOT" --version | awk '{print $NF}')"

say "test case: module, case, batch, verdict, conclusion, finalize"
probe_rc=0
probe_out="$($PBOT tc module ls -p "$PROJECT" --no-cache --json 2>&1)" || probe_rc=$?
if [[ $probe_rc -eq 3 ]]; then
  # A 404: the instance does not serve the test-case routes at all.
  skip "the instance serves no test-case endpoints — the tc/run smoke is skipped"
elif [[ $probe_rc -ne 0 ]]; then
  # Anything else — a 401, a 429, a transport failure — is a real failure and
  # must not pass as "the feature is not deployed here".
  die "tc module ls (exit $probe_rc): ${probe_out%%$'\n'*}"
else
  png="$TMP_DIR/tc.png"
  cp "$TMP_DIR/shot.png" "$png"

  sleep "$PAUSE"
  if out="$($PBOT tc module create -p "$PROJECT" --code "$MODULE_CODE" \
    --description "pbot e2e $MARK" --json 2>"$ERR_FILE")"; then
    ok "module $MODULE_CODE"
  else
    die "tc module create: $(head -n 1 "$ERR_FILE")"
  fi

  sleep "$PAUSE"
  # `|| true` keeps a failed command from killing the script under `set -e`:
  # the check below reports the failure instead.
  case_json="$($PBOT tc create -p "$PROJECT" --module "$MODULE_CODE" \
    --title "pbot-e2e $MARK" \
    --purpose "The smoke test of the test-case feature." \
    --object-overview "The tc and run command groups." \
    --priority P2 \
    --precondition "The module code is registered in the project." \
    --environment "The instance the smoke test runs against." \
    --step "Write the verdict of step 2 | The step record carries the verdict" \
    --step "Write the conclusion of the case | The case record carries the conclusion" \
    --step "Close the batch | The batch is completed" \
    --json 2>"$ERR_FILE" || true)"
  case_number="$(jq -r '.number // empty' <<<"$case_json")"
  if [[ -n "$case_number" ]]; then
    ok "case $case_number"
  else
    # Every step below needs the case: the run ends here instead of
    # reporting a dozen failures that all come from this one.
    fatal "tc create: $(head -n 1 "$ERR_FILE")"
  fi

  sleep "$PAUSE"
  run_json="$($PBOT run create -p "$PROJECT" --version "$PBOT_VERSION" --build "$MARK" \
    --commit 0000000 --environment "pbot e2e" --executed-by me --json 2>"$ERR_FILE" || true)"
  # One jq for the two values of the answer. The split is done with the
  # shell and not with `mapfile`: the script has to run on the bash 3.2
  # that macOS ships.
  run_fields="$(jq -r '[(.id // ""), (.number // "")] | join(" ")' <<<"$run_json")"
  run_uuid="${run_fields%% *}"
  run_number="${run_fields##* }"
  if [[ -n "$run_uuid" ]]; then
    ok "batch $run_number"
  else
    fatal "run create: $(head -n 1 "$ERR_FILE")"
  fi

  sleep "$PAUSE"
  if out="$($PBOT run add "$run_uuid" "$case_number" -p "$PROJECT" --json 2>"$ERR_FILE")"; then
    ok "case added to the batch"
  else
    die "run add: $(head -n 1 "$ERR_FILE")"
  fi

  sleep "$PAUSE"
  if out="$($PBOT run step "$run_uuid" --case "$case_number" --step 2 --verdict fail \
    --image "$png" --yes -p "$PROJECT" --json 2>"$ERR_FILE")"; then
    ok "step 2 fails with a screenshot"
  else
    die "run step: $(head -n 1 "$ERR_FILE")"
  fi

  # The evidence of a case hangs on the case: a screenshot of a step does not
  # satisfy the finalize, so this finalize has to be refused.
  sleep "$PAUSE"
  fin_out="$($PBOT run finalize "$run_uuid" -p "$PROJECT" --json 2>&1)" && fin_rc=0 || fin_rc=$?
  if [[ $fin_rc -eq 5 ]] && grep -q 'evidence_missing' <<<"$fin_out"; then
    ok "finalize without case evidence is refused (evidence_missing)"
  else
    die "finalize without case evidence (exit $fin_rc)"
  fi

  sleep "$PAUSE"
  if out="$($PBOT run conclude "$run_uuid" --case "$case_number" --conclusion FAIL \
    --image "$png" --yes -p "$PROJECT" --json 2>"$ERR_FILE")"; then
    ok "conclusion FAIL with the case evidence"
  else
    die "run conclude: $(head -n 1 "$ERR_FILE")"
  fi

  # The conclusion is written once: the second write is refused. The batch is
  # still open here, so the refusal names the conclusion — a write after the
  # finalize is refused with run_completed instead, because a completed batch
  # is frozen.
  sleep "$PAUSE"
  again="$($PBOT run conclude "$run_uuid" --case "$case_number" --conclusion PASS \
    --evidence "a second conclusion write" --yes -p "$PROJECT" --json 2>&1)" && again_rc=0 || again_rc=$?
  if [[ $again_rc -ne 0 ]] && grep -q 'conclusion_is_final' <<<"$again"; then
    ok "the second conclusion write is refused (conclusion_is_final)"
  else
    die "the second conclusion write (exit $again_rc)"
  fi

  sleep "$PAUSE"
  if out="$($PBOT run finalize "$run_uuid" -p "$PROJECT" --json 2>"$ERR_FILE")"; then
    ok "finalize"
  else
    die "run finalize: $(head -n 1 "$ERR_FILE")"
  fi

  # A completed batch is frozen: every later write to it is refused.
  sleep "$PAUSE"
  frozen="$($PBOT run conclude "$run_uuid" --case "$case_number" --conclusion PASS \
    --evidence "a write into a completed batch" --yes -p "$PROJECT" --json 2>&1)" && frozen_rc=0 || frozen_rc=$?
  if [[ $frozen_rc -ne 0 ]] && grep -q 'run_completed' <<<"$frozen"; then
    ok "a write into the completed batch is refused (run_completed)"
  else
    die "a write into the completed batch (exit $frozen_rc)"
  fi

  printf '   note: the case %s and the batch %s stay in the project —\n' "$case_number" "$run_number"
  printf '         the test-case feature has no delete for a member of the project.\n'
fi

result
