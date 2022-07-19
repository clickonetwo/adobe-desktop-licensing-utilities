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
use serde::{Deserialize, Serialize};
use serde_json::Value;

use adlu_base::{get_saved_credential, u64encode};

use super::admin::{ActivationType, OcFileSpec};
use super::SignatureSpecifier;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedOnlineLicense {
    // pub adobe_time: String,  // it's spelled "AdobeTime" and we don't care
    #[serde(deserialize_with = "adlu_base::template_json::deserialize")]
    pub asnp: CachedOnlineAsnp,
    pub creation_timestamp: i64,
    pub creator_id: String,
    #[serde(deserialize_with = "adlu_base::template_json::deserialize")]
    pub cust_asnp: CachedOnlineCustAsnp,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedOnlineAsnp {
    pub asnp_spec_version: String,
    #[serde(deserialize_with = "adlu_base::base64_encoded_json::deserialize")]
    pub payload: CachedOnlineAsnpPayload,
    pub signatures: Vec<SignatureSpecifier>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedOnlineAsnpPayload {
    pub app_profile: String,
    #[serde(deserialize_with = "adlu_base::template_json::deserialize")]
    pub legacy_profile: LegacyProfile,
    pub user_profile: String,
    pub frl_profile: String,
    pub relationship_profile: String,
    pub control_profile: Value,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyProfile {
    license_id: String,
    license_type: i32,
    effective_end_timestamp: i64,
    // others
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedOnlineCustAsnp {
    pub asnp_spec_version: String,
    #[serde(deserialize_with = "adlu_base::base64_encoded_json::deserialize")]
    pub payload: CachedOnlineCustAsnpPayload,
    pub signatures: Vec<SignatureSpecifier>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedOnlineCustAsnpPayload {
    pub npd_id: String,
    pub asnp_id: String,
    pub creation_timestamp: i64,
    pub cache_lifetime: i64,
    pub response_type: String,
    // others
}

pub fn get_cached_expiry(oc_spec: &OcFileSpec) -> Option<String> {
    let npd_id = oc_spec.npd_id();
    let app_name = oc_spec.app_id();
    let cert_group_id = oc_spec.cert_group_id();
    // each type of licensing uses a different cert group for cached data
    let cert_group_base = &cert_group_id[..cert_group_id.len() - 2];
    let cert_group_suffix = match oc_spec.activation_type() {
        ActivationType::FrlOnline(_) => "03",
        ActivationType::FrlOffline => "06",
        ActivationType::FrlIsolated(_) => "06",
        ActivationType::FrlLan(_) => "09",
        ActivationType::Sdl => "13",
        ActivationType::Unknown(_) => &cert_group_id[cert_group_id.len() - 2..],
    };
    let cert_name = format!("{}{}", cert_group_base, cert_group_suffix);
    let note_key = u64encode(&format!("{}{{}}{}", app_name, &cert_name)).unwrap();
    if let Ok(json) = get_saved_credential(&note_key) {
        if let Ok(license) = serde_json::from_str::<CachedOnlineLicense>(&json) {
            if npd_id.eq(&license.cust_asnp.payload.npd_id) {
                let timestamp =
                    license.asnp.payload.legacy_profile.effective_end_timestamp;
                return Some(timestamp.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_get_online_expiration() {
        let npd_id = "YzQ5ZmIwOTYtNDc0Ny00MGM5LWJhNGQtMzFhZjFiODEzMGUz";
        let path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = path.join("../rsrc/credentials/ps-online-mac.json");
        let json = std::fs::read_to_string(path).expect("Couldn't read test json");
        let license = serde_json::from_str::<CachedOnlineLicense>(&json)
            .expect("Couldn't read cached license");
        if npd_id.eq(&license.cust_asnp.payload.npd_id) {
            let timestamp = license.asnp.payload.legacy_profile.effective_end_timestamp;
            assert_eq!(timestamp, 1740902401000);
        } else {
            panic!("Couldn't read or parse ")
        }
    }
}
