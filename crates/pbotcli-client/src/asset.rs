//! The asset upload of the test-case feature.
//!
//! A file that is the evidence of a test record is uploaded through the
//! generic asset endpoint of the workspace
//! (`POST /api/v1/workspaces/{ws}/assets/`). The upload **is** the bind:
//! the body carries the `entity_type` and the `entity_identifier` of the
//! record the file belongs to, and the file hangs on that record from the
//! moment it is registered. A second bind call would be refused with
//! `asset_already_bound` — the file has one owner.
//!
//! The endpoint accepts only images for this feature: a log or a command
//! output belongs into the Markdown field of the record, not into an
//! attachment. The gate is here, next to the upload, so every caller of the
//! feature gets it.

use crate::PlaneClient;
use pbotcli_core::PlaneError;
use serde::Deserialize;
use serde_json::Value;

/// The image types the server accepts as the evidence of a test record.
///
/// The server's own list (`/api/assets/v2/…` keeps images only): jpeg, jpg,
/// png, webp and gif.
pub const TEST_EVIDENCE_MIME_TYPES: [&str; 5] = [
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/webp",
    "image/gif",
];

/// The entity type of the file of one step of a batch record — the
/// screenshot of what the tester saw.
pub const ENTITY_TYPE_STEP_RECORD: &str = "TEST_RUN_STEP_RECORD";

/// The entity type of the file of the evidence of one case of a batch.
pub const ENTITY_TYPE_CASE_EVIDENCE: &str = "TEST_RUN_CASE_EVIDENCE";

/// The answer of the asset endpoint: the id of the file, the url it is
/// served under, and the presigned target the bytes go to.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AssetUpload {
    #[serde(default, rename = "asset_id")]
    pub asset_id: String,
    #[serde(default, rename = "asset_url")]
    pub asset_url: String,
    /// The presigned upload target; its shape varies between Plane
    /// versions, so it is kept raw.
    #[serde(default, rename = "upload_data")]
    pub upload_data: Value,
}

/// Reject a file that is not one of the images the server accepts.
///
/// The refusal is a validation error (exit 5): the file was refused before
/// any request was made, so nothing has to be undone.
pub fn ensure_test_evidence_mime(mime: &str) -> Result<(), PlaneError> {
    if TEST_EVIDENCE_MIME_TYPES.contains(&mime) {
        return Ok(());
    }
    Err(PlaneError::validation_with_hint(
        format!("'{mime}' is not an image type the server accepts as evidence."),
        format!(
            "Allowed types: {}. A log or a command output belongs into the Markdown field of the record, not into an attachment.",
            TEST_EVIDENCE_MIME_TYPES.join(", ")
        ),
    ))
}

impl PlaneClient {
    /// Upload an image as the evidence of a test record.
    ///
    /// The bytes go through the whole chain: register the upload (which
    /// binds the file to the record when `entity_identifier` is given),
    /// push them to the presigned target, mark the asset uploaded and read
    /// it back — a 2xx is not proof that the write landed (ADR-0007).
    ///
    /// `entity_identifier` is the id of the record the file belongs to. A
    /// file that has no record yet (the case side, where the record is a
    /// batch record that does not exist yet) is registered unbound: the
    /// server keeps it for a later bind.
    pub async fn upload_test_evidence(
        &self,
        project_id: &str,
        file_name: &str,
        mime: &str,
        bytes: Vec<u8>,
        entity_type: &str,
        entity_identifier: Option<&str>,
    ) -> Result<AssetUpload, PlaneError> {
        ensure_test_evidence_mime(mime)?;
        let size = bytes.len() as u64;
        let mut body = serde_json::json!({
            "name": file_name,
            "type": mime,
            "size": size,
            "entity_type": entity_type,
            "project_id": project_id,
        });
        if let Some(identifier) = entity_identifier {
            body["entity_identifier"] = Value::String(identifier.to_string());
        }
        let path = format!("/api/v1/workspaces/{}/assets/", self.workspace);
        let created: AssetUpload = self
            .request_json_with(
                reqwest::Method::POST,
                &path,
                &[],
                Some(&body),
                crate::testcase_error_mapper,
            )
            .await?;
        if created.asset_id.is_empty() {
            return Err(PlaneError::Api {
                message: "the asset register response did not include an asset id.".into(),
            });
        }

        // The bytes ride to the presigned target; the signature lives in the
        // form fields, so this request carries no API key.
        self.upload_to_presigned(&created.upload_data, file_name, mime, bytes)
            .await?;

        self.confirm_asset(&created.asset_id).await?;

        // Read the asset back: the confirm PATCH answering 2xx does not
        // prove that the file is there (ADR-0007).
        let (status, _) = self.get_workspace_asset(&created.asset_id).await?;
        if status != 200 {
            return Err(PlaneError::Api {
                message: "The asset was not confirmed by the server after upload.".into(),
            });
        }
        Ok(created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_image_types_pass_the_gate() {
        for mime in TEST_EVIDENCE_MIME_TYPES {
            assert!(ensure_test_evidence_mime(mime).is_ok(), "{mime}");
        }
    }

    #[test]
    fn a_non_image_is_refused_with_the_allowed_types() {
        let err = ensure_test_evidence_mime("text/plain").unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("text/plain"));
        let hint = err.hint().expect("a hint");
        assert!(hint.contains("image/png"), "{hint}");
    }

    #[test]
    fn an_empty_type_is_refused() {
        assert!(ensure_test_evidence_mime("").is_err());
    }

    #[tokio::test]
    async fn the_upload_registers_the_entity_and_pushes_the_bytes() {
        let mut server = mockito::Server::new_async().await;
        let upload_url = format!("{}/upload", server.url());
        let register = server
            .mock("POST", "/api/v1/workspaces/ws/assets/")
            .match_body(mockito::Matcher::Json(serde_json::json!({
                "name": "shot.png",
                "type": "image/png",
                "size": 4,
                "entity_type": "TEST_RUN_STEP_RECORD",
                "entity_identifier": "rec1",
                "project_id": "p1",
            })))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "asset_id": "a1",
                    "asset_url": "http://x/a1",
                    "upload_data": {"url": upload_url, "fields": {}},
                })
                .to_string(),
            )
            .create_async()
            .await;
        let upload = server
            .mock("POST", "/upload")
            .with_status(200)
            .create_async()
            .await;
        let confirm = server
            .mock("PATCH", "/api/v1/workspaces/ws/assets/a1/")
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"is_uploaded": true}),
            ))
            .with_status(204)
            .create_async()
            .await;
        let readback = server
            .mock("GET", "/api/v1/workspaces/ws/assets/a1/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"asset_id":"a1","asset_url":"http://x/a1"}"#)
            .create_async()
            .await;

        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let uploaded = client
            .upload_test_evidence(
                "p1",
                "shot.png",
                "image/png",
                b"data".to_vec(),
                ENTITY_TYPE_STEP_RECORD,
                Some("rec1"),
            )
            .await
            .unwrap();
        assert_eq!(uploaded.asset_id, "a1");
        register.assert_async().await;
        upload.assert_async().await;
        confirm.assert_async().await;
        readback.assert_async().await;
    }

    #[tokio::test]
    async fn a_file_that_is_not_an_image_never_reaches_the_server() {
        let mut server = mockito::Server::new_async().await;
        // The endpoint is mocked, but it may not be called: the gate refuses
        // the file before any request is made.
        let m = server
            .mock("POST", "/api/v1/workspaces/ws/assets/")
            .expect(0)
            .with_status(500)
            .create_async()
            .await;
        let client = PlaneClient::with_cache(&cfg(&server.url()), true).unwrap();
        let err = client
            .upload_test_evidence(
                "p1",
                "run.log",
                "text/plain",
                b"log".to_vec(),
                ENTITY_TYPE_CASE_EVIDENCE,
                Some("rec1"),
            )
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), pbotcli_core::errors::EXIT_VALIDATION);
        assert!(err.to_string().contains("text/plain"));
        m.assert_async().await;
    }

    fn cfg(base: &str) -> pbotcli_core::Config {
        pbotcli_core::Config {
            base_url: base.to_string(),
            api_key: "test-key".into(),
            workspace: "ws".into(),
        }
    }
}
