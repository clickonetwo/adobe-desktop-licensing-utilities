/*
Copyright 2021 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
mod admin;
mod user;

pub use admin::{ActivationType, Configuration, OcFileSpec, PreconditioningData};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureSpecifier {
    #[serde(deserialize_with = "frl_base::base64_encoded_json::deserialize")]
    pub header: SignatureHeaderData,
    pub signature: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHeaderData {
    pub content_signature_alg: String,
    pub trusted_cert_fingerprint_alg: String,
    pub trusted_cert_fingerprint_index: i32,
    pub certificate_details: Vec<CertificateDetails>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateDetails {
    pub id: String,
    pub subject_name: String,
    pub hex_serial_number: String,
    pub sha1_hash: String,
    pub sequence: i32,
    pub download_path: String,
}
