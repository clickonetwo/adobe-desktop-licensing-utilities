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
use warp::{Filter, Reply};

use adlu_base::Timestamp;

use crate::{AdobeSignatures, CustomerSignatures};

/// There are two kinds of requests and responses: activation
/// and deactivation.  But pretty much all the actions you
/// take with both kinds are the same: cache them, send the
/// requests to server, get the responses from the server.
/// Also, there are times when we need to process collections
/// of requests that are ordered by timestamp, so having
/// an umbrella type that can be carried in a collection
/// allows sorting that collection by timestamp.
#[derive(Debug, Clone)]
pub enum FrlRequest {
    Activation(Box<FrlActivationRequest>),
    Deactivation(Box<FrlDeactivationRequest>),
}

impl FrlRequest {
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            FrlRequest::Activation(req) => &req.timestamp,
            FrlRequest::Deactivation(req) => &req.timestamp,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            FrlRequest::Activation(req) => &req.request_id,
            FrlRequest::Deactivation(req) => &req.request_id,
        }
    }

    /// a [`warp::Filter`] that produces an `FrlRequest` from a well-formed
    /// activation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide an activation hander at a specific endpoint.
    pub fn activation_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
        warp::post()
            .and(warp::header::<String>("X-Session-Id"))
            .and(warp::header::<String>("X-Request-Id"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::body::json())
            .map(
                |session_id: String,
                 request_id: String,
                 api_key: String,
                 parsed_body: FrlActivationRequestBody| {
                    FrlRequest::Activation(Box::new(FrlActivationRequest {
                        timestamp: Timestamp::now(),
                        api_key,
                        request_id,
                        session_id,
                        parsed_body,
                    }))
                },
            )
    }

    /// a [`warp::Filter`] that produces an `FrlRequest` from a well-formed
    /// deactivation network request posted to a warp server.  You can compose this filter with
    /// a path filter to provide a deactivation hander at a specific endpoint.
    pub fn deactivation_filter(
    ) -> impl Filter<Extract = (Self,), Error = warp::Rejection> + Clone {
        warp::delete()
            .and(warp::header::<String>("X-Request-Id"))
            .and(warp::header::<String>("X-Api-Key"))
            .and(warp::query::<FrlDeactivationQueryParams>())
            .map(
                |request_id: String,
                 api_key: String,
                 params: FrlDeactivationQueryParams| {
                    FrlRequest::Deactivation(Box::new(FrlDeactivationRequest {
                        timestamp: Timestamp::now(),
                        api_key,
                        request_id,
                        params,
                    }))
                },
            )
    }

    pub fn to_network(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        match self {
            FrlRequest::Activation(req) => req.to_network(builder),
            FrlRequest::Deactivation(req) => req.to_network(builder),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FrlResponse {
    Activation(Box<FrlActivationResponse>),
    Deactivation(Box<FrlDeactivationResponse>),
}

impl FrlResponse {
    pub fn timestamp(&self) -> &Timestamp {
        match self {
            FrlResponse::Activation(resp) => &resp.timestamp,
            FrlResponse::Deactivation(resp) => &resp.timestamp,
        }
    }

    pub async fn from_network(req: &FrlRequest, resp: reqwest::Response) -> Result<Self> {
        match req {
            FrlRequest::Activation(_) => {
                let resp = FrlActivationResponse::from_network(resp).await?;
                Ok(FrlResponse::Activation(Box::new(resp)))
            }
            FrlRequest::Deactivation(_) => {
                let resp = FrlDeactivationResponse::from_network(resp).await?;
                Ok(FrlResponse::Deactivation(Box::new(resp)))
            }
        }
    }
}

impl From<FrlResponse> for warp::reply::Response {
    fn from(resp: FrlResponse) -> Self {
        match resp {
            FrlResponse::Activation(resp) => resp.into_response(),
            FrlResponse::Deactivation(resp) => resp.into_response(),
        }
    }
}

impl Reply for FrlResponse {
    fn into_response(self) -> warp::reply::Response {
        self.into()
    }
}

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
pub struct FrlActivationRequestBody {
    pub app_details: FrlAppDetails,
    pub asnp_template_id: String,
    pub device_details: FrlDeviceDetails,
    pub npd_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npd_precedence: Option<i32>,
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
    #[serde(deserialize_with = "adlu_base::base64_encoded_json::deserialize")]
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
            "asnpTemplateId" : "WXpRNVptS...elided...wNy05ZjNjMWFhM2VhZjc",
            "deviceDetails" : 
            {
                "currentDate" : "2022-06-28T17:08:01.736-0700",
                "deviceId" : "2c93c8798aa2b6253c651e6efd5fe4694595a8dad82dc3d35de233df5928c2fa",
                "enableVdiMarkerExists" : false,
                "isOsUserAccountInDomain" : false,
                "isVirtualEnvironment" : false,
                "osName" : "MAC",
                "osUserId" : "b693be356ac52411389a6c06eede8b4e47e583818384cddc62aff78c3ece084d",
                "osVersion" : "12.4.0"
            },
            "npdId" : "YzQ5ZmIwOTYtNDc0Ny00MGM5LWJhNGQtMzFhZjFiODEzMGUz",
            "npdPrecedence" : 80
        }"#;
        let request: super::FrlActivationRequestBody =
            serde_json::from_str(request_str).unwrap();
        assert_eq!(request.app_details.ngl_app_id, "Photoshop1");
        assert!(!request.device_details.is_os_user_account_in_domain);
        assert_eq!(request.npd_precedence, Some(80));
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
                    "enigmaData": "{\\\"productId\\\":45,\\\"serialKey\\\":\\\"104545012345537554907209\\\",\\\"clearSerialKey\\\":null,\\\"locale\\\":\\\"ALL\\\",\\\"associatedLocales\\\":\\\"ALL\\\",\\\"platform\\\":0,\\\"isk\\\":454042,\\\"customerId\\\":0,\\\"deliveryMethod\\\":3,\\\"pc\\\":true,\\\"rb\\\":false}",
                    "graceTime": "8553600000",
                    "createdForVdi": "false",
                    "profileStatus": "PROFILE_AVAILABLE",
                    "effectiveEndTimestamp": "1741507201000",
                    "licenseExpiryWarningStartTimestamp": "1749456001000",
                    "nglLibRefreshInterval": "86400000",
                    "licenseId": "8A935605037F4F02B7BA",
                    "licensedFeatures": "[\\\"AMT_SUBSCRIPTION_6.0\\\",\\\"Bridge_Base_4.0\\\",\\\"Bridge_Base_5.0\\\",\\\"Bridge_Base_6.0\\\",\\\"Bridge_CameraRaw_4.0\\\",\\\"Bridge_CameraRaw_5.0\\\",\\\"Bridge_CameraRaw_6.0\\\",\\\"Bridge_MiniBridge_1.0\\\",\\\"Bridge_MiniBridge_2.0\\\",\\\"Bridge_MiniBridge_3.0\\\",\\\"Euclid_Base_1.0\\\",\\\"MobileCenter_Base_3.0\\\",\\\"Photoshop_14.0\\\",\\\"Photoshop_Base_12.0\\\",\\\"Photoshop_Base_13.0\\\",\\\"Photoshop_Base_14.0\\\",\\\"Photoshop_Premium_12.0\\\",\\\"Photoshop_Premium_13.0\\\",\\\"Photoshop_Premium_14.0\\\"]",
                    "appRefreshInterval": "86400000",
                    "appEntitlementStatus": "SUBSCRIPTION"
                }
            },
            "customerCertSignedValues": {
                "signatures": {
                    "customerSignature2": "LV5a3B2I-1_Tr2Lev54_PntYTWKrevciaodE7lH933NpEeJrp1qBJPobXS0dKhSzUmiFyGeCCUat5SursCpce2291WcRIHOYAJjkMKfQ3Lr_L_gewGcnDvyCEbGHRluoqgXvGOJgD5gFJviDXAw754cIpoKhfnPCk9WtgfCQbnG6enmC6hWXCIgRx1dgAggC3HNW0Rh_13HdG71UCyYTYpMYXxZef-aM0UTvrVHEOM7NXyGu5mCnQZFIqhc--JCGdENPtUgEFBPqwWQfbMrdN31572iimU3FyIVy4L1yEQBHQFP3Ra9RX-_7gzmuo_wR9c0kBy7QpxM7UDYt-deqLb68i0txT3yFb_iWt0c55wORmOMcA3ZOelHPkZ6T01ey9-SeT-d2aTxTXzGIquu7eTBE_focGXEVfB11V4oQ3qG73uaxRfLQ_bkQYeh1vXLc9hvxswAtFb5aZ797XeJ3nb2uVGgr2AXFmeRQrowGEK4uK7DFUbrMwuZtj_1b6ZOh7n2Z8gIGQHlPkm3mqPLYt8mj7e1JCnc4_TcIccphWG6pkyZlq9Xuhdg8Yqg-IAW50qOP0uTAlXXYjLHt0SICIB6uPcnwG8frfAnP0X_vDkxf_uwaaPqBADcLWSOgu5i4KFbOPVayZqST1WbhJxlcw5odSbapwrfcRTTjtolQDSI",
                    "customerSignature1": "mmzlAlEcU6X_a-F77AaGRRrzAh-btGgRg_ymvttxR-fegm_8UxFL8qHJgZmsT4QvgqjuWOb5l05sn5kNeBcBAf-Tw1Gth_WOy9bWo8JS2fd3HBRyQSJkpsyhHPzBkqfEqUpNZkjm0tR73cS8aWtDTAkbF7BBNoEAVfIfvnPRId3tn_HMf04em26ED19MoYajBCyl-LuF25yqLutOh6-2By1A_ujajIFESL7jw2aTWXgf_eG7d6fMMliGn29FUvn-afEYUXO6Lhgfy9qrPtfPB4f7LNIWMTTKKtfxDotnQsF75Qri-A3OTeAe-tMXD55HIyJl-tZEXE14Dp9tapRhYA47MPYgGIVYch1OduGWPopqjAtzCn4423IugZRtQAzJWuT07qmBTBQHL_2cSWLve1UsSg9pjEroO9Grhayn6xxk44vIyoEz2g47JyZFRw0Oa8XxTN1e8FAlQZ6at-rh5F9afrenAb2ndmJxCG9NKI7_PmRy4U9v1BCxS34NT--S2gWIRKZHdc8wRIEZHy6MkF-80Y7KuRf9HpZOUR1Msxfap6bsnw9iHGE1akT8xMMZLOgRsWaLcab5jyhWVnLVeMTaFUtsRKrKQtnyvt6jUsaAz0EhyXigJ2R8XOmVsbdo8A86Adbfs_EXerQGGTHMuIVvJ5K1KHoHJ5dPXZI3oYY"
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
    fn test_parse_deactivation_response() {
        let response_str = r#"{"invalidationSuccessful":true}"#;
        let response: super::FrlDeactivationResponseBody =
            serde_json::from_str(response_str).unwrap();
        assert!(response.invalidation_successful);
    }
}
