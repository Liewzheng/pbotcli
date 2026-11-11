//! The error vocabulary of the test-case feature.
//!
//! The feature answers a refused write with a stable envelope —
//! `{"error": …, "code": …, "errors": [{code, message, path, field,
//! case_number, step_sequence}]}` — and a permission failure with the DRF
//! body `{"detail": …}` that carries the server's own remedy in the same
//! sentence.
//!
//! A client keys on the **stable code**, never on the English text: the
//! code is what the server promises to keep, the Chinese sentence is what
//! it shows next to the offending field. This module turns both into the
//! typed [`PlaneError`] of the workspace, so a handler prints the code and
//! the sentence and never has to parse prose.
//!
//! The mapping is shared by both halves of the feature (the cases of this
//! mission and the batches of the next one), which is why it lives in the
//! client and not in a command handler.

use pbotcli_core::PlaneError;
use serde_json::Value;

/// One entry of the `errors` list of a refused write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErrorEntry {
    /// The stable reason code (`expected_is_unjudgeable`, `conclusion_is_final`, …).
    pub code: String,
    /// The sentence the server shows next to the offending field.
    pub message: String,
    /// The path of the field inside the record (`cases[TC-CLI-001].steps[3].actual_value`).
    pub path: String,
    /// The name of the field on its own (`actual_value`).
    pub field: String,
    /// The number of the case the problem belongs to, when the server names one.
    pub case_number: Option<String>,
    /// The number of the step the problem belongs to, when the server names one.
    pub step_sequence: Option<i64>,
}

impl ErrorEntry {
    /// Read one entry out of the JSON, ignoring a field of an unexpected
    /// shape instead of refusing the whole envelope.
    fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let text = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default()
        };
        let code = text("code");
        let message = text("message");
        // An entry without a code and without a sentence says nothing.
        if code.is_empty() && message.is_empty() {
            return None;
        }
        Some(ErrorEntry {
            code,
            message,
            path: text("path"),
            field: text("field"),
            case_number: object
                .get("case_number")
                .and_then(Value::as_str)
                .map(str::to_string),
            step_sequence: object.get("step_sequence").and_then(Value::as_i64),
        })
    }

    /// Where the problem sits: the case and the step it names, else the
    /// field. Empty when the server named no place.
    pub fn locator(&self) -> String {
        let mut parts = Vec::new();
        if let Some(case) = &self.case_number {
            parts.push(format!("case {case}"));
        }
        if let Some(step) = self.step_sequence {
            parts.push(format!("step {step}"));
        }
        if !parts.is_empty() {
            return parts.join(", ");
        }
        if !self.field.is_empty() {
            return format!("field {}", self.field);
        }
        self.path.clone()
    }
}

/// The envelope of a refused write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErrorEnvelope {
    /// The server's own one-line summary (English).
    pub error: Option<String>,
    /// The stable code of the refusal as a whole.
    pub code: Option<String>,
    /// One entry per problem.
    pub errors: Vec<ErrorEntry>,
}

impl ErrorEnvelope {
    /// True when the envelope carries a stable code — either on its own or
    /// on one of its entries. A body without one is not a refusal the
    /// feature describes, and it is handled as a plain API failure.
    pub fn has_code(&self) -> bool {
        self.code.as_deref().is_some_and(|c| !c.is_empty())
            || self.errors.iter().any(|e| !e.code.is_empty())
    }

    /// The stable codes of the envelope: the one of the refusal first, then
    /// the ones of its entries.
    pub fn codes(&self) -> Vec<String> {
        let mut codes = Vec::new();
        if let Some(code) = self.code.as_deref().filter(|c| !c.is_empty()) {
            codes.push(code.to_string());
        }
        for entry in &self.errors {
            if !entry.code.is_empty() && !codes.contains(&entry.code) {
                codes.push(entry.code.clone());
            }
        }
        codes
    }
}

/// Parse the envelope of a refused write.
///
/// `None` when the body is not an envelope: a DRF `{"detail": …}` body, a
/// 404 `{"error": …}` body or a plain-text answer carries no code and is
/// not a refusal the feature describes.
pub fn parse_error_envelope(body: &str) -> Option<ErrorEnvelope> {
    let value: Value = serde_json::from_str(body).ok()?;
    let object = value.as_object()?;
    let code = object
        .get("code")
        .and_then(Value::as_str)
        .map(str::to_string);
    let error = object
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string);
    let errors = object
        .get("errors")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(ErrorEntry::from_value).collect())
        .unwrap_or_default();
    let envelope = ErrorEnvelope {
        error,
        code,
        errors,
    };
    if envelope.has_code() {
        Some(envelope)
    } else {
        None
    }
}

/// The sentence the server wrote about the failure.
///
/// The DRF `detail` of a 403 carries the server's remedy in the same
/// sentence and is echoed verbatim — the caller cannot guess the remedy
/// and must not paraphrase it.
fn server_text(body: &str, envelope: Option<&ErrorEnvelope>) -> String {
    // The `detail` of a DRF body carries the remedy of the server ("Manage
    // scopes.", "a workspace admin must add …") in the same sentence, so it
    // wins over the one-line summary of the envelope.
    if let Ok(value) = serde_json::from_str::<Value>(body)
        && let Some(detail) = value.get("detail").and_then(Value::as_str)
        && !detail.trim().is_empty()
    {
        return detail.trim().to_string();
    }
    if let Some(envelope) = envelope
        && let Some(error) = envelope.error.as_deref().filter(|e| !e.is_empty())
    {
        return error.to_string();
    }
    crate::snippet_or_redact(body.trim())
}

/// The message of a refusal: the stable code, then one entry per problem
/// with its place and the sentence the server shows for it.
fn format_envelope(envelope: &ErrorEnvelope, status: u16) -> String {
    let mut parts = Vec::new();
    if let Some(code) = envelope.code.as_deref().filter(|c| !c.is_empty()) {
        parts.push(code.to_string());
    }
    for entry in &envelope.errors {
        let mut line = entry.code.clone();
        if line.is_empty() {
            line = "error".to_string();
        }
        let locator = entry.locator();
        if !locator.is_empty() {
            line.push_str(&format!(" ({locator})"));
        }
        if !entry.message.is_empty() {
            line.push_str(": ");
            line.push_str(&entry.message);
        }
        parts.push(line);
    }
    if parts.is_empty() {
        return format!("API error (HTTP {status})");
    }
    parts.join(" | ")
}

/// The next step to run, named by the stable code the server refused with.
fn hint_for(envelope: &ErrorEnvelope) -> String {
    for code in envelope.codes() {
        let hint = match code.as_str() {
            "module_code_format" => {
                "The module code must be 2 to 6 uppercase letters (for example CLI)."
            }
            "module_description_missing" => "Pass --description with the module description.",
            "module_code_taken" => {
                "The module code is already registered in this project; pick another one."
            }
            "module_not_in_project" => {
                "Use a module of this project: run `pbot tc module ls -p <project>`."
            }
            "issue_not_in_project" => {
                "Link a work item of the same project: run `pbot wi ls -p <project>`."
            }
            "issue_link_exists" => "The work item is already linked to this test case.",
            "invalid_test_case" => "Fix the fields the server named and run the command again.",
            "invalid_payload" => "Check the payload the server refused and run the command again.",
            "invalid_batch" => "Fix the batch the server refused and run the command again.",
            "conflict" => {
                "The record changed in the meantime; read it again and re-apply your change."
            }
            _ => continue,
        };
        return hint.to_string();
    }
    "Fix what the server reported and run the command again.".to_string()
}

/// Map a failure of the test-case feature onto the typed error of the
/// workspace.
///
/// * `401` / `403` → `Auth` (exit 2): the credential or its grant is the
///   problem. The `detail` of a 403 carries the server's remedy and is
///   echoed verbatim.
/// * `404` → `NotFound` (exit 3).
/// * `400` / `409` **with** a stable code → `Validation` (exit 5):
///   the server read the request and refused it for a reason the caller
///   can act on. A `409` and a `400` of the same code are the same class
///   of failure, so a duplicate that arrives as `409` reads like the
///   `400` of the same reason.
/// * everything else → `Api` (exit 4): the transport or the server failed.
pub fn map_testcase_error(status: u16, body: &str) -> PlaneError {
    let envelope = parse_error_envelope(body);
    match status {
        401 => PlaneError::Auth {
            message: format!(
                "Authentication failed (HTTP 401): {}",
                server_text(body, envelope.as_ref())
            ),
        },
        403 => PlaneError::Auth {
            message: format!(
                "Permission denied (HTTP 403): {}",
                server_text(body, envelope.as_ref())
            ),
        },
        404 => PlaneError::NotFound {
            message: format!(
                "Not found (HTTP 404): {}",
                server_text(body, envelope.as_ref())
            ),
            hint: None,
        },
        400 | 409 => match envelope.as_ref().filter(|e| e.has_code()) {
            Some(envelope) => PlaneError::Validation {
                message: format_envelope(envelope, status),
                hint: Some(hint_for(envelope)),
            },
            None => PlaneError::Api {
                message: format!(
                    "API error (HTTP {status}): {}",
                    server_text(body, envelope.as_ref())
                ),
            },
        },
        _ => PlaneError::Api {
            message: format!(
                "API error (HTTP {status}): {}",
                server_text(body, envelope.as_ref())
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_envelope_with_every_field() {
        let body = r#"{
            "error": "The test case is not valid.",
            "code": "invalid_test_case",
            "errors": [{
                "code": "expected_is_unjudgeable",
                "message": "预期结果必须可观测。",
                "path": "cases[TC-CLI-001].steps[3].expected_result",
                "field": "expected_result",
                "case_number": "TC-CLI-001",
                "step_sequence": 3
            }]
        }"#;
        let envelope = parse_error_envelope(body).expect("envelope");
        assert_eq!(envelope.code.as_deref(), Some("invalid_test_case"));
        assert_eq!(envelope.errors.len(), 1);
        let entry = &envelope.errors[0];
        assert_eq!(entry.code, "expected_is_unjudgeable");
        assert_eq!(entry.case_number.as_deref(), Some("TC-CLI-001"));
        assert_eq!(entry.step_sequence, Some(3));
        assert_eq!(entry.locator(), "case TC-CLI-001, step 3");
    }

    #[test]
    fn a_body_without_a_code_is_not_an_envelope() {
        assert!(parse_error_envelope(r#"{"detail": "no grant"}"#).is_none());
        assert!(parse_error_envelope(r#"{"error": "Test case not found"}"#).is_none());
        assert!(parse_error_envelope("not json").is_none());
    }

    #[test]
    fn an_entry_with_a_null_case_number_is_read() {
        let body = r#"{"code":"invalid_batch","errors":[
            {"code":"evidence_missing","message":"缺少必填项：证据。","path":"cases[TC-PWR-001].evidence","field":"evidence","case_number":null,"step_sequence":null}]}"#;
        let envelope = parse_error_envelope(body).expect("envelope");
        let entry = &envelope.errors[0];
        assert_eq!(entry.case_number, None);
        assert_eq!(entry.step_sequence, None);
        assert_eq!(entry.locator(), "field evidence");
    }

    #[test]
    fn a_validation_refusal_keeps_the_code_and_the_sentence() {
        let body = r#"{"error":"The test case is not valid.","code":"invalid_test_case","errors":[
            {"code":"expected_is_unjudgeable","message":"预期结果必须可观测。","path":"p","field":"expected_result","case_number":"TC-CLI-001","step_sequence":3}]}"#;
        let err = map_testcase_error(400, body);
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        let message = err.to_string();
        assert!(message.contains("invalid_test_case"), "{message}");
        assert!(message.contains("expected_is_unjudgeable"), "{message}");
        assert!(message.contains("case TC-CLI-001, step 3"), "{message}");
        assert!(message.contains("预期结果必须可观测。"), "{message}");
    }

    #[test]
    fn a_duplicate_that_arrives_as_409_reads_like_the_400_of_the_same_code() {
        let body = r#"{"error":"The case is already in the batch.","code":"invalid_batch","errors":[
            {"code":"duplicate_case_number","message":"同一个用例在批次中重复出现。","path":"cases[TC-CLI-001]","field":"","case_number":"TC-CLI-001","step_sequence":null}]}"#;
        let conflict = map_testcase_error(409, body);
        let refused = map_testcase_error(400, body);
        assert_eq!(conflict.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert_eq!(
            conflict.exit_code(),
            refused.exit_code(),
            "a 409 of the same code is the same class of failure as the 400"
        );
        assert!(conflict.to_string().contains("duplicate_case_number"));
        assert!(
            conflict
                .to_string()
                .contains("同一个用例在批次中重复出现。")
        );
    }

    #[test]
    fn a_permission_failure_echoes_the_remedy_verbatim() {
        let body = r#"{"detail":"Service principal 'Bot' lacks scope for read test_case in project p1. A workspace admin must add the required scope for this service principal at Workspace settings -> Developer -> Service principals -> Manage scopes."}"#;
        let err = map_testcase_error(403, body);
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_AUTH);
        let message = err.to_string();
        assert!(
            message.contains("Manage scopes."),
            "the remedy of the server must survive verbatim: {message}"
        );
        assert!(message.contains("lacks scope for read test_case"));
    }

    #[test]
    fn a_missing_record_is_a_not_found() {
        let err = map_testcase_error(404, r#"{"error":"Test case not found"}"#);
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_NOT_FOUND);
        assert!(err.to_string().contains("Test case not found"));
    }

    #[test]
    fn a_transport_failure_is_an_api_error() {
        let err = map_testcase_error(500, "boom");
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_API);
        assert!(err.to_string().contains("HTTP 500"));
    }

    #[test]
    fn a_refusal_without_a_code_stays_an_api_error() {
        let err = map_testcase_error(400, r#"{"name":["This field is required."]}"#);
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_API);
        assert!(err.to_string().contains("HTTP 400"));
    }

    #[test]
    fn the_hint_names_the_next_command() {
        let body = r#"{"error":"The module code is not valid.","code":"module_code_format","errors":[
            {"code":"module_code_format","message":"模块码必须是 2 至 6 位大写字母。","path":"code","field":"code"}]}"#;
        let err = map_testcase_error(400, body);
        let hint = err.hint().expect("a hint");
        assert!(hint.contains("2 to 6 uppercase letters"), "{hint}");
    }
}
