/*
Copyright 2022 Daniel Brotsky. All rights reserved.

All of the copyrighted work in this repository is licensed under the
GNU Affero General Public License, reproduced in the LICENSE-AGPL file.

Attribution:

Some source files in this repository are derived from files in two Adobe Open
Source projects: the Adobe License Decoder repository found at this URL:
    https://github.com/adobe/adobe-license-decoder.rs
and the FRL Online Proxy repository found at this URL:
    https://github.com/adobe/frl-online-proxy

The files in those original works are copyright 2022 Adobe and the use of those
materials in this work is permitted by the MIT license under which they were
released.  That license is reproduced here in the LICENSE-MIT file.
*/
mod admin;
mod user;

pub use admin::{ActivationType, Configuration, OcFileSpec, PreconditioningData};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureSpecifier {
    #[serde(deserialize_with = "adlu_base::base64_encoded_json::deserialize")]
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
