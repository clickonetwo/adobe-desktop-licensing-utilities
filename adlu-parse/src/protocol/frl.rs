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
use warp::Reply;

use adlu_base::Timestamp;

use crate::{AdobeSignatures, CustomerSignatures};

#[derive(Debug, Clone)]
pub struct FrlActivationRequest {
    pub timestamp: Timestamp,
    pub api_key: String,
    pub request_id: String,
    pub session_id: String,
    pub parsed_body: FrlActivationRequestBody,
}

impl FrlActivationRequest {
    pub fn activation_id(&self) -> String {
        let d_id = self.deactivation_id();
        let factors: Vec<&str> = vec![
            &self.parsed_body.app_details.ngl_app_id,
            &self.parsed_body.app_details.ngl_lib_version,
            &d_id,
        ];
        factors.join("|")
    }

    pub fn deactivation_id(&self) -> String {
        let factors: Vec<&str> = vec![
            &self.parsed_body.npd_id,
            if self.parsed_body.device_details.enable_vdi_marker_exists
                && self.parsed_body.device_details.is_virtual_environment
            {
                &self.parsed_body.device_details.os_user_id
            } else {
                &self.parsed_body.device_details.device_id
            },
        ];
        factors.join("|")
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        builder
            .header("X-Request-Id", &self.request_id)
            .header("X-Session-Id", &self.session_id)
            .header("X-Api-Key", &self.api_key)
            .json(&self.parsed_body)
    }
}

#[derive(Debug, Clone)]
pub struct FrlDeactivationRequest {
    pub timestamp: Timestamp,
    pub api_key: String,
    pub request_id: String,
    pub params: FrlDeactivationQueryParams,
}

impl FrlDeactivationRequest {
    pub fn deactivation_id(&self) -> String {
        let factors: Vec<&str> = vec![
            &self.params.npd_id,
            if self.params.enable_vdi_marker_exists != 0
                && self.params.is_virtual_environment != 0
            {
                &self.params.os_user_id
            } else {
                &self.params.device_id
            },
        ];
        factors.join("|")
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
pub struct FrlDeactivationQueryParams {
    pub npd_id: String,
    pub device_id: String,
    pub os_user_id: String,
    pub enable_vdi_marker_exists: i8,
    pub is_virtual_environment: i8,
    pub is_os_user_account_in_domain: i8,
}

impl FrlDeactivationQueryParams {
    pub fn mock_from_device_id(device_id: &str) -> Self {
        Self {
            npd_id: "YzQ5ZmIw...elided...jFiOD".to_string(),
            device_id: device_id.to_string(),
            os_user_id: "b693be35...elided...2aff7".to_string(),
            enable_vdi_marker_exists: 0,
            is_virtual_environment: 0,
            is_os_user_account_in_domain: 0,
        }
    }

    pub fn to_query_params(&self) -> String {
        serde_urlencoded::to_string(self).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlActivationRequestBody {
    pub app_details: FrlAppDetails,
    pub asnp_template_id: String,
    pub device_details: FrlDeviceDetails,
    pub npd_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npd_precedence: Option<i32>,
}

impl FrlActivationRequestBody {
    pub fn mock_from_device_id(device_id: &str) -> Self {
        Self {
            app_details: FrlAppDetails {
                current_asnp_id: "".to_string(),
                ngl_app_id: "MockApp1".to_string(),
                ngl_app_version: "10.1.3".to_string(),
                ngl_lib_version: "1.23.0.5".to_string(),
            },
            asnp_template_id: "WXpRNVpt...elided...wNy05Z".to_string(),
            device_details: FrlDeviceDetails {
                current_date: "2022-06-28T17:08:01.736-0700".to_string(),
                device_id: device_id.to_string(),
                enable_vdi_marker_exists: false,
                is_os_user_account_in_domain: false,
                is_virtual_environment: false,
                os_name: "MAC".to_string(),
                os_user_id: "b693be35...elided...2aff7".to_string(),
                os_version: "12.4.0".to_string(),
            },
            npd_id: "YzQ5ZmIw...elided...jFiOD".to_string(),
            npd_precedence: Some(80),
        }
    }

    pub fn to_body_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlAppDetails {
    #[serde(default)]
    pub current_asnp_id: String,
    pub ngl_app_id: String,
    pub ngl_app_version: String,
    pub ngl_lib_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlDeviceDetails {
    pub current_date: String,
    pub device_id: String,
    pub enable_vdi_marker_exists: bool,
    pub is_os_user_account_in_domain: bool,
    pub is_virtual_environment: bool,
    pub os_name: String,
    pub os_user_id: String,
    pub os_version: String,
}

#[derive(Debug, Clone)]
pub struct FrlActivationResponse {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub body: String,
    pub parsed_body: Option<FrlActivationResponseBody>,
}

impl FrlActivationResponse {
    pub async fn from_network(response: reqwest::Response) -> Result<Self> {
        let request_id = match response.headers().get("X-Request-Id") {
            None => {
                return Err(eyre!("No activation request-id"));
            }
            Some(val) => {
                val.to_str().wrap_err("Invalid activation request-id")?.to_string()
            }
        };
        let body = response.text().await.wrap_err("Failure to receive body")?;
        let parsed_body: Option<FrlActivationResponseBody> =
            if cfg!(feature = "parse_responses") {
                Some(
                    serde_json::from_str::<FrlActivationResponseBody>(&body)
                        .wrap_err("Invalid activation response")?,
                )
            } else {
                None
            };
        Ok(FrlActivationResponse {
            timestamp: Timestamp::now(),
            request_id,
            body,
            parsed_body,
        })
    }
}

impl From<FrlActivationResponse> for warp::reply::Response {
    fn from(act_resp: FrlActivationResponse) -> Self {
        ::http::Response::builder()
            .header("X-Request-Id", &act_resp.request_id)
            .body(act_resp.body.into())
            .unwrap()
    }
}

impl Reply for FrlActivationResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone)]
pub struct FrlDeactivationResponse {
    pub timestamp: Timestamp,
    pub request_id: String,
    pub body: String,
    pub parsed_body: Option<FrlDeactivationResponseBody>,
}

impl FrlDeactivationResponse {
    pub async fn from_network(response: reqwest::Response) -> Result<Self> {
        let request_id = match response.headers().get("X-Request-Id") {
            None => {
                return Err(eyre!("No deactivation request-id"));
            }
            Some(val) => {
                val.to_str().wrap_err("Invalid deactivation request-id")?.to_string()
            }
        };
        let body = response.text().await.wrap_err("Failure to receive body")?;
        let parsed_body: Option<FrlDeactivationResponseBody> =
            if cfg!(feature = "parse_responses") {
                Some(
                    serde_json::from_str::<FrlDeactivationResponseBody>(&body)
                        .wrap_err("Invalid deactivation response")?,
                )
            } else {
                None
            };
        Ok(FrlDeactivationResponse {
            timestamp: Timestamp::now(),
            request_id,
            body,
            parsed_body,
        })
    }
}

impl From<FrlDeactivationResponse> for warp::reply::Response {
    fn from(act_resp: FrlDeactivationResponse) -> Self {
        ::http::Response::builder()
            .header("X-Request-Id", &act_resp.request_id)
            .body(act_resp.body.into())
            .unwrap()
    }
}

impl Reply for FrlDeactivationResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlActivationResponseBody {
    pub adobe_cert_signed_values: FrlAdobeCertSignedValues,
    pub customer_cert_signed_values: FrlCustomerCertSignedValues,
}

impl FrlActivationResponseBody {
    pub fn mock_from_device_id(device_id: &str) -> Self {
        Self {
            adobe_cert_signed_values: FrlAdobeCertSignedValues {
                signatures: AdobeSignatures {
                    signature1: "laj2sLb...elided...Oi9zqEy12olv6M".to_string(),
                    signature2: "aSAqFfd...elided...XkbpwFzAWgoLQ".to_string(),
                },
                values: FrlAdobeSignedValues {
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
            customer_cert_signed_values: FrlCustomerCertSignedValues {
                signatures: CustomerSignatures {
                    customer_signature2: "LV5a3B2I...elided...lQDSI".to_string(),
                    customer_signature1: "mmzlAlEc...elided...I3oYY".to_string(),
                },
                values: FrlCustomerSignedValues {
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
pub struct FrlAdobeCertSignedValues {
    pub signatures: AdobeSignatures,
    pub values: FrlAdobeSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlAdobeSignedValues {
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
pub struct FrlCustomerCertSignedValues {
    pub signatures: CustomerSignatures,
    #[serde(with = "adlu_base::base64_encoded_json")]
    pub values: FrlCustomerSignedValues,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrlCustomerSignedValues {
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
pub struct FrlDeactivationResponseBody {
    invalidation_successful: bool,
}

impl FrlDeactivationResponseBody {
    pub fn mock_from_device_id(_device_id: &str) -> Self {
        FrlDeactivationResponseBody { invalidation_successful: true }
    }

    pub fn to_body_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_parse_activation_request() {
        let request_str = r#"
        {
            "appDetails" : 
            {
                "currentAsnpId" : "",
                "nglAppId" : "Photoshop1",
                "nglAppVersion" : "23.4.1",
                "nglLibVersion" : "1.30.0.1"
            },
            "asnpTemplateId" : "WXpRNVpt...elided...wNy05Z",
            "deviceDetails" : 
            {
                "currentDate" : "2022-06-28T17:08:01.736-0700",
                "deviceId" : "2c93c879...elided...8c2fa",
                "enableVdiMarkerExists" : false,
                "isOsUserAccountInDomain" : false,
                "isVirtualEnvironment" : false,
                "osName" : "MAC",
                "osUserId" : "b693be35...elided...2aff7",
                "osVersion" : "12.4.0"
            },
            "npdId" : "YzQ5ZmIw...elided...jFiOD",
            "npdPrecedence" : 80
        }"#;
        let request: super::FrlActivationRequestBody =
            serde_json::from_str(request_str).unwrap();
        assert_eq!(request.app_details.ngl_app_id, "Photoshop1");
        assert!(!request.device_details.is_os_user_account_in_domain);
        assert_eq!(request.npd_precedence, Some(80));
    }

    #[test]
    fn test_parse_mock_activation_request() {
        let body = super::FrlActivationRequestBody::mock_from_device_id("test-id");
        let request: super::FrlActivationRequestBody =
            serde_json::from_str(body.to_body_string().as_str()).unwrap();
        assert_eq!(request.device_details.device_id, "test-id");
        assert_eq!(request.app_details.ngl_app_id, "MockApp1");
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
        let response: super::FrlActivationResponseBody =
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
        let response = super::FrlActivationResponseBody::mock_from_device_id("test-id");
        let body = response.to_body_string();
        let response: super::FrlActivationResponseBody =
            serde_json::from_str(&body).unwrap();
        assert_eq!(response.customer_cert_signed_values.values.device_id, "test-id");
    }

    #[test]
    fn test_parse_deactivation_request() {
        let query = "npdId=YzQ5ZmIw...elided...zMGUz&deviceId=2c93c879...elided...28c2fa&osUserId=b693be35...elided...e084d&enableVdiMarkerExists=0&isVirtualEnvironment=0&isOsUserAccountInDomain=0";
        let body: super::FrlDeactivationQueryParams =
            serde_urlencoded::from_str(query).unwrap();
        assert_eq!(body.npd_id, "YzQ5ZmIw...elided...zMGUz");
        assert_eq!(body.os_user_id, "b693be35...elided...e084d");
    }

    #[test]
    fn test_parse_mock_deactivation_request() {
        let params = super::FrlDeactivationQueryParams::mock_from_device_id("test-id");
        let body: super::FrlDeactivationQueryParams =
            serde_urlencoded::from_str(&params.to_query_params()).unwrap();
        assert_eq!(body.npd_id, "YzQ5ZmIw...elided...jFiOD");
        assert_eq!(body.device_id, "test-id");
    }

    #[test]
    fn test_parse_deactivation_response() {
        let response_str = r#"{"invalidationSuccessful":true}"#;
        let response: super::FrlDeactivationResponseBody =
            serde_json::from_str(response_str).unwrap();
        assert!(response.invalidation_successful);
    }

    #[test]
    fn test_parse_mock_deactivation_response() {
        let mock = super::FrlDeactivationResponseBody::mock_from_device_id("test-id");
        let response: super::FrlDeactivationResponseBody =
            serde_json::from_str(&mock.to_body_string()).unwrap();
        assert!(response.invalidation_successful);
    }
}
