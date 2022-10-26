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
use eyre::{eyre, Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use warp::Reply;

use adlu_base::Timestamp;

use crate::{AdobeSignatures, CustomerSignatures};

#[derive(Clone)]
pub struct NulActivationRequest {
    pub timestamp: Timestamp,
    pub authorization: String,
    pub api_key: String,
    pub request_id: String,
    pub session_id: String,
    pub body: Vec<u8>,
    pub parsed_body: Option<NulActivationRequestBody>,
}

impl std::fmt::Debug for NulActivationRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NulActivationRequest")
            .field("timestamp", &self.timestamp)
            .field("authorization", &self.authorization)
            .field("api_key", &self.api_key)
            .field("request_id", &self.request_id)
            .field("session_id", &self.session_id)
            .field("body", &String::from_utf8_lossy(&self.body).to_string())
            .finish()
    }
}

impl NulActivationRequest {
    pub fn from_parts(
        authorization: String,
        request_id: String,
        session_id: String,
        api_key: String,
        body: bytes::Bytes,
    ) -> Self {
        let body = body.to_vec();
        let parsed_body = if let Ok(parse) =
            serde_json::from_slice::<NulActivationRequestBody>(&body)
        {
            Some(parse)
        } else {
            None
        };
        Self {
            timestamp: Timestamp::now(),
            authorization,
            api_key,
            request_id,
            session_id,
            body,
            parsed_body,
        }
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("Authorization", &self.authorization)
            .header("X-Request-Id", &self.request_id)
            .header("X-Session-Id", &self.session_id)
            .header("X-Api-Key", &self.api_key)
            // because the body is bytes, we have to set the content type
            .header("Content-Type", "application/json")
            .body(self.body.clone())
    }
}

#[derive(Debug, Clone)]
pub struct NulDeactivationRequest {
    pub timestamp: Timestamp,
    pub api_key: String,
    pub request_id: String,
    pub params: HashMap<String, String>,
}

impl NulDeactivationRequest {
    pub fn from_parts(
        request_id: String,
        api_key: String,
        params: HashMap<String, String>,
    ) -> Self {
        Self { timestamp: Timestamp::now(), api_key, request_id, params }
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("X-Request-Id", &self.request_id)
            .header("X-Api-Key", &self.api_key)
            .query(&self.params)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulActivationRequestBody {
    pub app_details: NulAppDetails,
    pub device_details: NulDeviceDetails,
    pub device_token_hash: String,
}

impl NulActivationRequestBody {
    pub fn mock_from_device_id(device_id: &str) -> Self {
        Self {
            app_details: NulAppDetails {
                app_name_for_locale: "MockApp".to_string(),
                app_version_for_locale: "2022".to_string(),
                current_asnp_id: "".to_string(),
                e_tag: "".to_string(),
                locale: "en_US".to_string(),
                ngl_app_id: "MockApp1".to_string(),
                ngl_app_launch_state: "WORKFLOW_STATE".to_string(),
                ngl_app_profile_scope: "".to_string(),
                ngl_app_version: "10.1.3".to_string(),
                ngl_lib_runtime_mode: "NAMED_USER_ONLINE".to_string(),
                ngl_lib_version: "1.23.0.5".to_string(),
            },
            device_details: NulDeviceDetails {
                current_date: "2022-06-28T17:08:01.736-0700".to_string(),
                current_timestamp: 1656450481736,
                device_id: device_id.to_string(),
                device_name: "mock_device".to_string(),
                embedded_browser_version: "".to_string(),
                enable_vdi_marker_exists: false,
                is_os_user_account_in_domain: false,
                is_virtual_environment: false,
                os_name: "MAC".to_string(),
                os_user_id: "b693be35...elided...2aff7".to_string(),
                os_version: "12.4.0".to_string(),
            },
            device_token_hash: "9f5d39712...elided...246aa4b".to_string(),
        }
    }

    pub fn valid_from_device_id(device_id: &str) -> Self {
        let timestamp = Timestamp::now();
        let device_date = timestamp.to_device_date();
        Self {
            app_details: NulAppDetails {
                app_name_for_locale: "Premiere Pro".to_string(),
                app_version_for_locale: "2022".to_string(),
                current_asnp_id: "57ba50f3-e5d8-4509-8718-cadfd8b22286".to_string(),
                e_tag: "_9tdZvyiUM8CC_nKU4s98fVIDBc74U1Vh8r2J_XKjXn7AIqaH48IfvM7ZkWGl"
                    .to_string(),
                locale: "en_US".to_string(),
                ngl_app_id: "PremierePro1".to_string(),
                ngl_app_launch_state: "WORKFLOW_STATE".to_string(),
                ngl_app_profile_scope: "".to_string(),
                ngl_app_version: "22.6.2".to_string(),
                ngl_lib_runtime_mode: "NAMED_USER_ONLINE".to_string(),
                ngl_lib_version: "1.30.0.1".to_string(),
            },
            device_details: NulDeviceDetails {
                current_date: device_date,
                current_timestamp: timestamp.millis,
                device_id: device_id.to_string(),
                device_name: "dan".to_string(),
                embedded_browser_version: "WK-17613.3.9.1.16".to_string(),
                enable_vdi_marker_exists: false,
                is_os_user_account_in_domain: false,
                is_virtual_environment: false,
                os_name: "MAC".to_string(),
                os_user_id:
                    "b693be356ac52411389a6c06eede8b4e47e583818384cddc62aff78c3ece084d"
                        .to_string(),
                os_version: "12.6.0".to_string(),
            },
            device_token_hash: "".to_string(),
        }
    }

    pub fn to_body_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulAppDetails {
    #[serde(default)]
    pub app_name_for_locale: String,
    #[serde(default)]
    pub app_version_for_locale: String,
    #[serde(default)]
    pub current_asnp_id: String,
    #[serde(default)]
    pub e_tag: String,
    pub locale: String,
    pub ngl_app_id: String,
    #[serde(default)]
    pub ngl_app_launch_state: String,
    #[serde(default)]
    pub ngl_app_profile_scope: String,
    pub ngl_app_version: String,
    #[serde(default)]
    pub ngl_lib_runtime_mode: String,
    pub ngl_lib_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulDeviceDetails {
    pub current_date: String,
    #[serde(default)]
    pub current_timestamp: i64,
    pub device_id: String,
    pub device_name: String,
    #[serde(default)]
    pub embedded_browser_version: String,
    #[serde(default)]
    pub enable_vdi_marker_exists: bool,
    #[serde(default)]
    pub is_os_user_account_in_domain: bool,
    #[serde(default)]
    pub is_virtual_environment: bool,
    pub os_name: String,
    pub os_user_id: String,
    pub os_version: String,
}

#[derive(Debug, Clone)]
pub struct NulActivationResponse {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub body: String,
}

impl NulActivationResponse {
    pub async fn from_network(response: reqwest::Response) -> Result<Self> {
        let request_id = super::get_response_id(&response)?;
        let body = response.text().await.wrap_err("Failure to receive body")?;
        Ok(NulActivationResponse { timestamp: Timestamp::now(), request_id, body })
    }
}

impl From<NulActivationResponse> for warp::reply::Response {
    fn from(act_resp: NulActivationResponse) -> Self {
        ::http::Response::builder()
            .header("X-Request-Id", &act_resp.request_id)
            .body(act_resp.body.into())
            .unwrap()
    }
}

impl Reply for NulActivationResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone)]
pub struct LicenseSession {
    pub session_id: String,
    pub session_start: Timestamp,
    pub session_end: Timestamp,
    pub app_id: String,
    pub app_version: String,
    pub app_locale: String,
    pub ngl_version: String,
    pub os_name: String,
    pub os_version: String,
    pub user_id: String,
}

impl LicenseSession {
    pub fn merge(&self, other: LicenseSession) -> Result<Self> {
        if self.session_id != other.session_id {
            Err(eyre!("Can't merge sessions with different IDs"))
        } else {
            let mut result = self.clone();
            result.session_end = other.session_end;
            Ok(result)
        }
    }
}

impl From<&NulActivationRequest> for LicenseSession {
    fn from(req: &NulActivationRequest) -> Self {
        let parsed_body =
            req.parsed_body.as_ref().expect("Can't create a report with a parsed body");
        let session_id = if let Some(start) = req.session_id.find('/') {
            req.session_id[0..start].to_string()
        } else {
            req.session_id.clone()
        };
        Self {
            session_id,
            session_start: req.timestamp.clone(),
            session_end: req.timestamp.clone(),
            app_id: parsed_body.app_details.ngl_app_id.clone(),
            app_version: parsed_body.app_details.ngl_app_version.clone(),
            app_locale: parsed_body.app_details.locale.clone(),
            ngl_version: parsed_body.app_details.ngl_lib_version.clone(),
            os_name: parsed_body.device_details.os_name.clone(),
            os_version: parsed_body.device_details.os_name.clone(),
            user_id: parsed_body.device_details.os_name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NulDeactivationResponse {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub body: String,
    pub parsed_body: Option<NulDeactivationResponseBody>,
}

impl NulDeactivationResponse {
    pub async fn from_network(response: reqwest::Response) -> Result<Self> {
        let request_id = super::get_response_id(&response)?;
        let body = response.text().await.wrap_err("Failure to receive body")?;
        let parsed_body: Option<NulDeactivationResponseBody> =
            if cfg!(feature = "parse_responses") {
                Some(
                    serde_json::from_str::<NulDeactivationResponseBody>(&body)
                        .wrap_err("Invalid deactivation response")?,
                )
            } else {
                None
            };
        Ok(NulDeactivationResponse {
            timestamp: Timestamp::now(),
            request_id,
            body,
            parsed_body,
        })
    }
}

impl From<NulDeactivationResponse> for warp::reply::Response {
    fn from(act_resp: NulDeactivationResponse) -> Self {
        ::http::Response::builder()
            .header("X-Request-Id", &act_resp.request_id)
            .body(act_resp.body.into())
            .unwrap()
    }
}

impl Reply for NulDeactivationResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulActivationResponseBody {
    pub adobe_cert_signed_values: NulAdobeCertSignedValues,
    pub customer_cert_signed_values: NulCustomerCertSignedValues,
}

impl NulActivationResponseBody {
    pub fn mock_from_device_id(device_id: &str) -> Self {
        Self {
            adobe_cert_signed_values: NulAdobeCertSignedValues {
                signatures: AdobeSignatures {
                    signature1: "laj2sLb...elided...Oi9zqEy12olv6M".to_string(),
                    signature2: "aSAqFfd...elided...XkbpwFzAWgoLQ".to_string(),
                },
                values: NulAdobeSignedValues {
                    license_expiry_timestamp: "1750060801000".to_string(),
                    enigma_data: "{{...elided...}}".to_string(),
                    grace_time: "8553600000".to_string(),
                    created_for_vdi: "false".to_string(),
                    profile_status: "PROFILE_AVAILABLE".to_string(),
                    effective_end_timestamp: "1741507201000".to_string(),
                    license_expiry_warning_start_timestamp: "1749456001000".to_string(),
                    ngl_lib_refresh_interval: "86400000".to_string(),
                    license_id: "012...elided...345".to_string(),
                    licensed_features: "[[...elided...]]".to_string(),
                    app_refresh_interval: "86400000".to_string(),
                    app_entitlement_status: "SUBSCRIPTION".to_string(),
                },
            },
            customer_cert_signed_values: NulCustomerCertSignedValues {
                signatures: CustomerSignatures {
                    customer_signature2: "LV5a3B2I...elided...lQDSI".to_string(),
                    customer_signature1: "mmzlAlEc...elided...I3oYY".to_string(),
                },
                values: NulCustomerSignedValues {
                    npd_id: "YzQ5ZmIw...elided...jFiOD".to_string(),
                    asnp_id: "221bf...elided...c23ff".to_string(),
                    creation_timestamp: 1656461282009,
                    cache_lifetime: 93599518991,
                    response_type: "FRL_INITIAL".to_string(),
                    cache_expiry_warning_control: CacheExpiryWarningControl {
                        warning_start_timestamp: 1749456001000,
                        warning_interval: 86400000,
                    },
                    previous_asnp_id: "".to_string(),
                    device_id: device_id.to_string(),
                    os_user_id: "b693be35...elided...2aff7".to_string(),
                    device_date: "2022-06-28T17:08:01.736-0700".to_string(),
                    session_id: "b9d543...elided...81312/SUBSEQUENT".to_string(),
                },
            },
        }
    }

    pub fn to_body_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulAdobeCertSignedValues {
    pub signatures: AdobeSignatures,
    pub values: NulAdobeSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulAdobeSignedValues {
    pub license_expiry_timestamp: String,
    pub enigma_data: String,
    pub grace_time: String,
    pub created_for_vdi: String,
    pub profile_status: String,
    pub effective_end_timestamp: String,
    pub license_expiry_warning_start_timestamp: String,
    pub ngl_lib_refresh_interval: String,
    pub license_id: String,
    pub licensed_features: String,
    pub app_refresh_interval: String,
    pub app_entitlement_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulCustomerCertSignedValues {
    pub signatures: CustomerSignatures,
    #[serde(with = "adlu_base::base64_encoded_json")]
    pub values: NulCustomerSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulCustomerSignedValues {
    pub npd_id: String,
    pub asnp_id: String,
    pub creation_timestamp: i64,
    pub cache_lifetime: i64,
    pub response_type: String,
    pub cache_expiry_warning_control: CacheExpiryWarningControl,
    pub previous_asnp_id: String,
    pub device_id: String,
    pub os_user_id: String,
    pub device_date: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheExpiryWarningControl {
    warning_start_timestamp: i64,
    warning_interval: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NulDeactivationResponseBody {
    invalidation_successful: bool,
}

impl NulDeactivationResponseBody {
    pub fn mock_from_device_id(_device_id: &str) -> Self {
        NulDeactivationResponseBody { invalidation_successful: true }
    }

    pub fn to_body_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    #[test]
    fn test_parse_activation_request() {
        let request_str = r#"
        {
            "appDetails" : 
            {
                "appNameForLocale" : "Premiere Pro",
                "appVersionForLocale" : "2022",
                "currentAsnpId" : "57ba50f3-e5d8-4509-8718-cadfd8b22286",
                "eTag" : "_9tdZvyiUM8CC_nKU4s98fVIDBc74U1Vh8r2J_XKjXn7AIqaH48IfvM7ZkWGl-g9",
                "locale" : "en_US",
                "nglAppId" : "PremierePro1",
                "nglAppLaunchState" : "WORKFLOW_STATE",
                "nglAppProfileScope" : "",
                "nglAppVersion" : "22.6.2",
                "nglLibRuntimeMode" : "NAMED_USER_ONLINE",
                "nglLibVersion" : "1.30.0.1"
            },
            "deviceDetails" : 
            {
                "currentDate" : "2022-10-13T00:52:53.058-0400",
                "currentTimestamp" : 1665636773396,
                "deviceId" : "2c93c8798aa2b6253c651e6efd5fe4694595a8dad82dc3d35de233df5928c2fa",
                "deviceName" : "dan",
                "embeddedBrowserVersion" : "WK-17613.3.9.1.16",
                "enableVdiMarkerExists" : false,
                "isOsUserAccountInDomain" : false,
                "isVirtualEnvironment" : false,
                "osName" : "MAC",
                "osUserId" : "b693be356ac52411389a6c06eede8b4e47e583818384cddc62aff78c3ece084d",
                "osVersion" : "12.6.0"
            },
            "deviceTokenHash" : "9f5d39712181d23ad8f6d6a50feb8a3c50e08ae0ffc323a411bc529caf9ed779ad68abc9ac83e87818b9188d0de4b32721425c5abb98c0dfae6f8efe7246aa4b"
        }"#;
        let request: super::NulActivationRequestBody =
            serde_json::from_str(request_str).unwrap();
        assert_eq!(request.app_details.ngl_app_id, "PremierePro1");
        assert!(!request.device_details.is_os_user_account_in_domain);
    }

    #[test]
    fn test_parse_mock_activation_request() {
        let body = super::NulActivationRequestBody::mock_from_device_id("test-id");
        let request: super::NulActivationRequestBody =
            serde_json::from_str(body.to_body_string().as_str()).unwrap();
        assert_eq!(request.device_details.device_id, "test-id");
        assert_eq!(request.app_details.ngl_app_id, "MockApp1");
    }

    #[test]
    fn test_parse_valid_activation_request() {
        let body = super::NulActivationRequestBody::valid_from_device_id("test-id");
        let request: super::NulActivationRequestBody =
            serde_json::from_str(body.to_body_string().as_str()).unwrap();
        assert_eq!(request.device_details.device_id, "test-id");
        assert_eq!(request.app_details.ngl_app_id, "PremierePro1");
    }

    #[test]
    fn test_parse_activation_response() {
        let response_str = r#"
        {
            "adobeCertSignedValues": {
                "signatures": {
                    "signature1": "laj2sLb...elided...Oi9zqEy12olv6M",
                    "signature2": "aSAqFfd...elided...XkbpwFzAWgoLQ"
                },
                "values": {
                    "licenseExpiryTimestamp": "1750060801000",
                    "enigmaData": "{{...elided...}}",
                    "graceTime": "8553600000",
                    "createdForVdi": "false",
                    "profileStatus": "PROFILE_AVAILABLE",
                    "effectiveEndTimestamp": "1741507201000",
                    "licenseExpiryWarningStartTimestamp": "1749456001000",
                    "nglLibRefreshInterval": "86400000",
                    "licenseId": "012...elided...345",
                    "licensedFeatures": "[[...elided...]]",
                    "appRefreshInterval": "86400000",
                    "appEntitlementStatus": "SUBSCRIPTION"
                }
            },
            "customerCertSignedValues": {
                "signatures": {
                    "customerSignature2": "LV5a3B2I...elided...lQDSI",
                    "customerSignature1": "mmzlAlEc...elided...I3oYY"
                },
                "values": "eyJucGRJZCI6Ill6UTVabUl3T1RZdE5EYzBOeTAwTUdNNUxXSmhOR1F0TXpGaFpqRmlPREV6TUdVeiIsImFzbnBJZCI6IjIyMWJmYWQ1LTBhZTMtNDY4MC05Mjc1LWY3ZDVjYTFjMjNmZiIsImNyZWF0aW9uVGltZXN0YW1wIjoxNjU2NDYxMjgyMDA5LCJjYWNoZUxpZmV0aW1lIjo5MzU5OTUxODk5MSwicmVzcG9uc2VUeXBlIjoiRlJMX0lOSVRJQUwiLCJjYWNoZUV4cGlyeVdhcm5pbmdDb250cm9sIjp7Indhcm5pbmdTdGFydFRpbWVzdGFtcCI6MTc0OTQ1NjAwMTAwMCwid2FybmluZ0ludGVydmFsIjo4NjQwMDAwMH0sInByZXZpb3VzQXNucElkIjoiIiwiZGV2aWNlSWQiOiIyYzkzYzg3OThhYTJiNjI1M2M2NTFlNmVmZDVmZTQ2OTQ1OTVhOGRhZDgyZGMzZDM1ZGUyMzNkZjU5MjhjMmZhIiwib3NVc2VySWQiOiJiNjkzYmUzNTZhYzUyNDExMzg5YTZjMDZlZWRlOGI0ZTQ3ZTU4MzgxODM4NGNkZGM2MmFmZjc4YzNlY2UwODRkIiwiZGV2aWNlRGF0ZSI6IjIwMjItMDYtMjhUMTc6MDg6MDEuNzM2LTA3MDAiLCJzZXNzaW9uSWQiOiJiOWQ1NDM4OS1mZGM0LTQzMjctYTc3My0xY2FmYTY5NmE1MzEuMTY1NjQ2MTI4MTMxMi9TVUJTRVFVRU5UIn0"
            }
        }
        "#;
        let response: super::NulActivationResponseBody =
            serde_json::from_str(response_str).unwrap();
        assert_eq!(
            response.adobe_cert_signed_values.values.profile_status,
            "PROFILE_AVAILABLE"
        );
        assert_eq!(
            response.customer_cert_signed_values.values.npd_id,
            "YzQ5ZmIwOTYtNDc0Ny00MGM5LWJhNGQtMzFhZjFiODEzMGUz"
        )
    }

    #[test]
    fn test_parse_mock_activation_response() {
        let response = super::NulActivationResponseBody::mock_from_device_id("test-id");
        let body = response.to_body_string();
        let response: super::NulActivationResponseBody =
            serde_json::from_str(&body).unwrap();
        assert_eq!(response.customer_cert_signed_values.values.device_id, "test-id");
    }

    #[test]
    fn test_parse_deactivation_request() {
        let query = "deviceId=2c93c879...elided...28c2fa&osUserId=b693be35...elided...e084d&enableVdiMarkerExists=0&isVirtualEnvironment=0&isOsUserAccountInDomain=0";
        let parse: HashMap<String, String> = serde_urlencoded::from_str(query).unwrap();
        assert_eq!(parse["deviceId"], "2c93c879...elided...28c2fa");
        assert_eq!(parse["osUserId"], "b693be35...elided...e084d");
    }
}