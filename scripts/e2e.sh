#!/usr/bin/env bash
# Live end-to-end smoke for pbot against a real Plane instance.
# Covers: create → markdown comment → state change → attachment upload →
# inline image embed → test case → test run → cleanup. Requires ~/.plane_api
# credentials and a reachable instance; paces calls to stay under the instance
# rate limit.
#
# Usage: scripts/e2e.sh [-p PROJECT]   (default project: PLANECLI)

set -euo pipefail

PROJECT="${1:-PLANECLI}"
PBOT="${PBOT:-pbot}"
PAUSE="${PAUSE:-4}"          # seconds between calls (rate-limit buffer)
MARK="pbot-e2e-$$"           # unique marker for cleanup
FAILURES=0

say()  { printf '\n== %s\n' "$*"; }
ok()   { printf '   ok: %s\n' "$*"; }
skip() { printf '   skip: %s\n' "$*"; }
die()  { printf '   FAIL: %s\n' "$*" >&2; FAILURES=$((FAILURES + 1)); }

cleanup() {
  say "cleanup $MARK"
  "$PBOT" wi ls -p "$PROJECT" --no-cache --json 2>/dev/null \
    | jq -r '.[] | select(.name | startswith("pbot-e2e")) | .id' \
    | while read -r id; do
        "$PBOT" wi delete "$id" -p "$PROJECT" >/dev/null 2>&1 || true
        sleep "$PAUSE"
      done
}
trap cleanup EXIT

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
png="/tmp/$MARK.png"
python3 -c "import base64;open('$png','wb').write(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))"
sleep "$PAUSE"
$PBOT attachment attach "$id" -f "$png" -p "$PROJECT" --json >/dev/null 2>&1 && ok "attach" || die "attach"

say "inline image (-i) on update"
sleep "$PAUSE"
# The file was attached under the same name one step ago, and the CLI
# refuses a duplicate name on purpose — --force is the documented way
# through, and this is the step that proves the inline image works.
$PBOT wi update "$id" -p "$PROJECT" -i "$png" --force --json >/dev/null 2>&1 \
  && ok "inline img" || die "inline img"
html="$($PBOT wi show "$id" --no-comments --no-cache --json 2>/dev/null | jq -r .description_html)"
grep -q '<img ' <<<"$html" && ok "inline img in the description" || die "inline img"
rm -f "$png"

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
# The module code is 2 to 6 UPPERCASE letters, so the pid is spelled out in
# letters: two smoke runs never fight over the same code.
letters_of() {
  local n=$1 out="" alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZ"
  while [[ $n -gt 0 && ${#out} -lt 5 ]]; do
    out="${alphabet:$(( n % 26)):1}$out"
    n=$(( n / 26 ))
  done
  printf '%s' "${out:-A}"
}
MODULE_CODE="E$(letters_of $$)"

say "test case: module, case, batch, verdict, conclusion, finalize"
if ! $PBOT tc module ls -p "$PROJECT" --no-cache --json >/dev/null 2>&1; then
  skip "the instance serves no test-case endpoints — the tc/run smoke is skipped"
else
  png="/tmp/$MARK-tc.png"
  python3 -c "import base64;open('$png','wb').write(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='))"

  sleep "$PAUSE"
  $PBOT tc module create -p "$PROJECT" --code "$MODULE_CODE" \
    --description "pbot e2e $MARK" --json >/dev/null 2>&1 \
    && ok "module $MODULE_CODE" || die "tc module create"

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
    --json 2>/dev/null || true)"
  case_number="$(jq -r '.number // empty' <<<"$case_json")"
  [[ -n "$case_number" ]] && ok "case $case_number" || die "tc create"

  sleep "$PAUSE"
  run_json="$($PBOT run create -p "$PROJECT" --version 1.6.0 --build "$MARK" \
    --commit 0000000 --env "pbot e2e" --executed-by me --json 2>/dev/null || true)"
  run_uuid="$(jq -r '.id // empty' <<<"$run_json")"
  run_number="$(jq -r '.number // empty' <<<"$run_json")"
  [[ -n "$run_uuid" ]] && ok "batch $run_number" || die "run create"

  sleep "$PAUSE"
  $PBOT run add "$run_uuid" "$case_number" -p "$PROJECT" --json >/dev/null 2>&1 \
    && ok "case added to the batch" || die "run add"

  sleep "$PAUSE"
  $PBOT run step "$run_uuid" --case "$case_number" --step 2 --verdict fail \
    --image "$png" --yes -p "$PROJECT" --json >/dev/null 2>&1 \
    && ok "step 2 fails with a screenshot" || die "run step"

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
  $PBOT run conclude "$run_uuid" --case "$case_number" --conclusion FAIL \
    --image "$png" --yes -p "$PROJECT" --json >/dev/null 2>&1 \
    && ok "conclusion FAIL with the case evidence" || die "run conclude"

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
  $PBOT run finalize "$run_uuid" -p "$PROJECT" --json >/dev/null 2>&1 \
    && ok "finalize" || die "run finalize"

  # A completed batch is frozen: every later write to it is refused.
  sleep "$PAUSE"
  frozen="$($PBOT run conclude "$run_uuid" --case "$case_number" --conclusion PASS \
    --evidence "a write into a completed batch" --yes -p "$PROJECT" --json 2>&1)" && frozen_rc=0 || frozen_rc=$?
  if [[ $frozen_rc -ne 0 ]] && grep -q 'run_completed' <<<"$frozen"; then
    ok "a write into the completed batch is refused (run_completed)"
  else
    die "a write into the completed batch (exit $frozen_rc)"
  fi

  rm -f "$png"
  printf '   note: the case %s and the batch %s stay in the project —\n' "$case_number" "$run_number"
  printf '         the test-case feature has no delete for a member of the project.\n'
fi

say "result"
if [[ "$FAILURES" -eq 0 ]]; then echo "E2E PASS"; else echo "E2E FAIL ($FAILURES)"; exit 1; fi
