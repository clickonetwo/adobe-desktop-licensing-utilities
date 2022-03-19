/*
Copyright 2022 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
use super::user::get_cached_expiry;
use super::SignatureSpecifier;
use eyre::{eyre, Result, WrapErr};
use frl_base::u64decode;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

pub enum Configuration {
    Packaged(Vec<PreconditioningData>),
    Installed(Vec<OcFileSpec>),
}

impl Configuration {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let info = std::fs::metadata(path.as_ref()).wrap_err("No configuration found")?;
        if info.is_dir() {
            Self::from_directory(path.as_ref())
        } else {
            Self::from_file(path.as_ref())
        }
    }

    pub fn from_directory<P: AsRef<Path>>(path: P) -> Result<Self> {
        if let Some(dir_str) = path.as_ref().to_str() {
            // first look for preconditioning data in the directory,
            // both in json files and package files
            let mut pcs: Vec<PreconditioningData> = Vec::new();
            let pattern = format!("{}/*.json", dir_str);
            for entry in glob::glob(&pattern).expect("Illegal search pattern") {
                let path = entry.expect("Can't read search result");
                match PreconditioningData::from_pc_file(&path) {
                    Ok(pc_data) => pcs.push(pc_data),
                    Err(err) => {
                        eprintln!("Failure on '{}': {}", path.to_string_lossy(), err)
                    }
                }
            }
            let pattern = format!("{}/*.ccp", dir_str);
            for entry in glob::glob(&pattern).expect("Illegal search pattern") {
                let path = entry.expect("Can't read search result");
                match PreconditioningData::from_package(&path) {
                    Ok(pc_data) => pcs.push(pc_data),
                    Err(err) => {
                        eprintln!("Failure on '{}': {}", path.to_string_lossy(), err)
                    }
                }
            }
            if !pcs.is_empty() {
                return Ok(Self::Packaged(pcs));
            }
            // if we don't find any, look for operating config files in the directory
            let pattern = format!("{}/*.operatingconfig", dir_str);
            let mut ocs: Vec<OcFileSpec> = Vec::new();
            for entry in glob::glob(&pattern).expect("Illegal search pattern") {
                let path = entry.expect("Can't read search result");
                let oc = OcFileSpec::from_file(path)?;
                ocs.push(oc)
            }
            if !ocs.is_empty() {
                return Ok(Self::Installed(ocs));
            }
            Err(eyre!(
                "No configuration files found in directory: {}",
                dir_str
            ))
        } else {
            Err(eyre!(
                "Cannot search for configurations in non-UTF8 path: {}",
                path.as_ref().to_string_lossy()
            ))
        }
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let extension = path
            .as_ref()
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("json") {
            let pc_data = PreconditioningData::from_pc_file(path)?;
            return Ok(Self::Packaged(vec![pc_data]));
        }
        if extension.eq_ignore_ascii_case("ccp") {
            let pc_data = PreconditioningData::from_package(path)?;
            return Ok(Configuration::Packaged(vec![pc_data]));
        }
        if extension.eq_ignore_ascii_case("operatingconfig") {
            let oc = OcFileSpec::from_file(path)?;
            return Ok(Configuration::Installed(vec![oc]));
        }
        Err(eyre!(
            "Not a configuration file: {}",
            path.as_ref().to_string_lossy()
        ))
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreconditioningData {
    pub npd_id: String,
    pub npd_spec_version: String,
    pub deployment_mode: String,
    pub operating_configs: Vec<OcFileSpec>,
    pub certificates: Vec<CertFileSpec>,
}

impl PreconditioningData {
    pub fn from_pc_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let json_data =
            std::fs::read_to_string(path).wrap_err("Can't read preconditioning data")?;
        let pc_data =
            serde_json::from_str(&json_data).wrap_err("Invalid preconditioning data")?;
        Ok(pc_data)
    }

    pub fn from_package<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = std::fs::read(path).wrap_err("Cannot read package")?;
        // This may be a zip file, in which case we extract the preconditioning data from
        // the "PkgConfig.xml" file contained in the zip.  Otherwise, this file just contains
        // the preconditioning data in XML form.
        let reader = std::io::Cursor::new(&bytes);
        let html = if let Ok(mut archive) = zip::ZipArchive::new(reader) {
            let mut file = archive
                .by_name("PkgConfig.xml")
                .map_err(|e| eyre!(e))
                .wrap_err("Can't find configuration data in package")?;
            let mut buffer = String::new();
            file.read_to_string(&mut buffer)
                .wrap_err("Can't read package")?;
            buffer
        } else {
            std::str::from_utf8(&bytes)
                .wrap_err("Invalid package format")?
                .to_string()
        };
        let doc = visdom::Vis::load(&html)
            .map_err(|e| eyre!("{}", e))
            .wrap_err("Cannot parse package")?;
        let data_node = doc.find("Preconditioning");
        let json_data = data_node.text();
        let pc_data: PreconditioningData = serde_json::from_str(json_data)
            .wrap_err("Can't parse preconditioning data in ccp file")?;
        Ok(pc_data)
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcFileSpec {
    pub name: String,
    pub extension: String,
    #[serde(skip)]
    pub mod_date: Option<String>, // local mod date when deserialized from file
    #[serde(deserialize_with = "frl_base::base64_encoded_json::deserialize")]
    pub content: OperatingConfig,
}

impl OcFileSpec {
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        if let Some(name) = path.as_ref().file_stem() {
            if let Some(name) = name.to_str() {
                if let Some(extension) = path.as_ref().extension() {
                    if let Some(extension) = extension.to_str() {
                        let mod_date: chrono::DateTime<chrono::Local> =
                            std::fs::metadata(&path)
                                .wrap_err("Can't access operating config")?
                                .modified()
                                .wrap_err("Can't access operating config")?
                                .into();
                        let json_data = std::fs::read_to_string(&path)
                            .wrap_err("Can't read operating config")?;
                        let oc = serde_json::from_str(&json_data)
                            .wrap_err("Invalid operating config data")?;
                        return Ok(Self {
                            name: name.to_string(),
                            extension: extension.to_string(),
                            mod_date: Some(
                                mod_date.format("%Y-%m-%d %H:%M:%S %Z").to_string(),
                            ),
                            content: oc,
                        });
                    }
                }
            }
        }
        Err(eyre!("Invalid operating config filename"))
    }

    pub fn npd_id(&self) -> String {
        self.content.payload.npd_id.clone()
    }

    pub fn app_id(&self) -> String {
        self.content.payload.ngl_app_id.clone()
    }

    pub fn install_date(&self) -> Option<String> {
        self.mod_date.clone()
    }

    pub fn cert_group_id(&self) -> String {
        if let Some(leading_part) = self.name.split('-').next() {
            if let Ok(leading_part) = u64decode(leading_part) {
                let info: Vec<&str> = leading_part.split("{}").collect();
                if info.len() == 2 {
                    return info[1].to_string();
                }
            }
        }
        String::from("Invalid")
    }

    pub fn activation_type(&self) -> ActivationType {
        let payload = &self.content.payload;
        match payload.deployment_mode.as_str() {
            "NAMED_USER_EDUCATION_LAB" => ActivationType::Sdl,
            "FRL_CONNECTED" => {
                let server = payload.profile_server_url.clone();
                ActivationType::FrlOnline(server)
            }
            "FRL_LAN" => {
                let server = payload.profile_server_url.clone();
                ActivationType::FrlLan(server)
            }
            "FRL_ISOLATED" => {
                let codes = payload.asnp_data.as_ref()
                    .expect("Invalid license data (FRL with no Adobe profile data)")
                    .customer_cert_signed_values.as_ref()
                    .expect("Invalid license data (FRL Isolated with no customer-signed values)")
                    .values
                    .challenge_codes.clone();
                let code0 = codes.get(0).expect(
                    "Invalid license data (FRL Isolated with no challenge codes)",
                );
                if code0.len() > 18 {
                    ActivationType::FrlOffline
                } else {
                    let codes = codes
                        .iter()
                        .map(|code| {
                            if code.len() != 18 {
                                "invalid-census-code".to_string()
                            } else {
                                format!(
                                    "{}-{}-{}",
                                    &code[0..6],
                                    &code[6..12],
                                    &code[12..18]
                                )
                            }
                        })
                        .collect();
                    ActivationType::FrlIsolated(codes)
                }
            }
            s => ActivationType::Unknown(s.to_string()),
        }
    }

    pub fn expiry_date(&self) -> String {
        let payload = &self.content.payload;
        if let Some(adobe_values) = payload
            .asnp_data
            .as_ref()
            .and_then(|asnp| asnp.adobe_cert_signed_values.as_ref())
        {
            let timestamp = adobe_values.values.license_expiry_timestamp.as_str();
            frl_base::date_from_epoch_millis(timestamp).unwrap_or_else(|_| {
                panic!("Invalid license data (bad timestamp: {})", timestamp)
            })
        } else {
            "controlled by server".to_string()
        }
    }

    pub fn precedence(&self) -> Precedence {
        let prec = self.content.payload.npd_precedence;
        match prec {
            70 => Precedence::AcrobatStandard,
            100 => Precedence::AcrobatPro,
            80 => Precedence::CcSingleApp,
            90 => Precedence::CcAllApps,
            _ => panic!(
                "Invalid license data: Precedence ({}) must be 70, 80, 90, or 100",
                prec
            ),
        }
    }

    pub fn cached_expiry(&self) -> Option<String> {
        get_cached_expiry(self)
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertFileSpec {
    pub name: String,
    pub extension: String,
    #[serde(skip)]
    pub content: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatingConfig {
    pub oc_spec_version: String,
    pub signatures: Vec<SignatureSpecifier>,
    #[serde(deserialize_with = "frl_base::base64_encoded_json::deserialize")]
    pub payload: OcPayload,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcPayload {
    pub id: String,
    pub npd_id: String,
    pub ngl_app_id: String,
    pub npd_precedence: i32,
    pub asnp_data: Option<AsnpData>,
    pub profile_server_url: String,
    pub profile_request_payload_params: Option<ProfileRequestPayloadParams>,
    pub deployment_mode: String,
    pub branding: BrandingData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_server_cert_fingerprint: Option<String>, // LAN only
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsnpData {
    pub template_id: String,
    pub customer_cert_headers: Vec<SignatureSpecifier>,
    pub adobe_cert_signed_values: Option<AdobeSignedValues>,
    pub customer_cert_signed_values: Option<CustomerSignedValues>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrandingData {
    pub name: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRequestPayloadParams {
    pub device_params: Vec<String>,
    pub app_params: Vec<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdobeSignedValues {
    pub signatures: AdobeSignatures,
    pub values: AdobeValues,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerSignedValues {
    pub signatures: CustomerSignatures,
    #[serde(deserialize_with = "frl_base::base64_encoded_json::deserialize")]
    pub values: CustomerValues,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdobeSignatures {
    pub signature1: String,
    pub signature2: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdobeValues {
    pub license_expiry_timestamp: String,
    pub enigma_data: String,
    pub grace_time: String,
    pub profile_status: String,
    pub effective_end_timestamp: String,
    pub license_expiry_warning_start_timestamp: String,
    pub ngl_lib_refresh_interval: String,
    pub license_id: String,
    pub licensed_features: String,
    pub app_refresh_interval: String,
    pub app_entitlement_status: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerSignatures {
    pub customer_signature2: String,
    pub customer_signature1: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerValues {
    pub npd_id: String,
    pub asnp_id: String,
    pub creation_timestamp: u64,
    pub cache_lifetime: u64,
    pub response_type: String,
    pub cache_expiry_warning_control: CacheExpiryWarningControl,
    pub challenge_codes: Vec<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheExpiryWarningControl {
    pub warning_start_timestamp: u64,
    pub warning_interval: u64,
}

#[cfg(test)]
mod tests {
    use super::OperatingConfig;
    use super::PreconditioningData;
    use legacy::types::{FileInfo, OperatingConfig as ManualOperatingConfig};

    extern crate serde_json; // 1.0.69

    #[test]
    fn test_online_oc() {
        // first parse the actual operating config and make sure it matches the hand parse
        let path = "../rsrc/OperatingConfigs/UGhvdG9zaG9wMXt9MjAxODA3MjAwNA-ODU0YjU5OGQtOTE1Ni00NDZiLWFlZDYtMGQ1ZGM2ZmVhZDBi-80.operatingconfig";
        let info = FileInfo::from_path(path).expect("Can't find online test data");
        let json =
            std::fs::read_to_string(&info.pathname).expect("Can't read online data file");
        let oc1: OperatingConfig =
            serde_json::from_str(&json).expect("Can't parse online data");
        let oc2 = ManualOperatingConfig::from_license_file(&info)
            .expect("Can't manually extract config");
        assert_eq!(oc1.payload.npd_id, oc2.npd_id, "npdIds do not match");
        assert_eq!(oc1.payload.ngl_app_id, oc2.app_id, "appIds do not match");
        // now serialize the OC and make sure it matches the hand-generated reference decode
        let decode = serde_json::to_string(&oc1).unwrap();
        let ref_path = "../rsrc/OperatingConfigs/ps-online-proxy.operatingconfig";
        let ref_decode =
            std::fs::read_to_string(ref_path).expect("Can't read reference JSON");
        assert_eq!(decode, ref_decode);
    }

    #[test]
    fn test_isolated_oc() {
        let path = "../rsrc/OperatingConfigs/SWxsdXN0cmF0b3Ixe30yMDE4MDcyMDA0-MmE0N2E4M2UtNjFmNS00NmM2LWE0N2ItOGE0Njc2MTliOTI5-80.operatingconfig";
        let info = FileInfo::from_path(path).expect("Can't find isolated test data");
        let json = std::fs::read_to_string(&info.pathname)
            .expect("Can't read isolated data file");
        let oc1: OperatingConfig =
            serde_json::from_str(&json).expect("Can't parse isolated data");
        let oc2 = ManualOperatingConfig::from_license_file(&info)
            .expect("Can't manually extract config");
        assert_eq!(oc1.payload.npd_id, oc2.npd_id, "npdIds do not match");
        assert_eq!(oc1.payload.ngl_app_id, oc2.app_id, "appIds do not match");
        let decode = serde_json::to_string(&oc1).unwrap();
        let ref_path = "../rsrc/OperatingConfigs/ai-isolated.operatingconfig";
        let ref_decode =
            std::fs::read_to_string(ref_path).expect("Can't read reference JSON");
        assert_eq!(decode, ref_decode);
    }

    #[test]
    fn test_lan_oc() {
        let path = "../rsrc/OperatingConfigs/SWxsdXN0cmF0b3Ixe30yMDE4MDcyMDA0-OTUzZTViZWYtYWJmMy00NGUxLWFjYjUtZmZhN2MyMDY4YjQx-80.operatingconfig";
        let info = FileInfo::from_path(path).expect("Can't find LAN test data");
        let json =
            std::fs::read_to_string(&info.pathname).expect("Can't read LAN data file");
        let oc1: OperatingConfig =
            serde_json::from_str(&json).expect("Can't parse LAN data");
        let oc2 = ManualOperatingConfig::from_license_file(&info)
            .expect("Can't manually extract config");
        assert_eq!(oc1.payload.npd_id, oc2.npd_id, "npdIds do not match");
        assert_eq!(oc1.payload.ngl_app_id, oc2.app_id, "appIds do not match");
        let decode = serde_json::to_string(&oc1).unwrap();
        let ref_path = "../rsrc/OperatingConfigs/ai-lan.operatingconfig";
        let ref_decode =
            std::fs::read_to_string(ref_path).expect("Can't read reference JSON");
        assert_eq!(decode, ref_decode);
    }

    #[test]
    fn test_sdl_oc() {
        let acro_path = "../rsrc/OperatingConfigs/QWNyb2JhdERDMXt9MjAxODA3MjAwNA-NDIzOTc1ZTItODQ2Ni00MDU0LTk2ZDEtNWQ4NzMwOWE4NGZk-90.operatingconfig";
        let id_path = "../rsrc/OperatingConfigs/SW5EZXNpZ24xe30yMDE4MDcyMDA0-NDIzOTc1ZTItODQ2Ni00MDU0LTk2ZDEtNWQ4NzMwOWE4NGZk-90.operatingconfig";
        let acro_info =
            FileInfo::from_path(acro_path).expect("Can't find Acrobat SDL test data");
        let id_info =
            FileInfo::from_path(id_path).expect("Can't find InDesign SDL test data");
        let acro_json = std::fs::read_to_string(&acro_info.pathname)
            .expect("Can't read Acrobat LAN data file");
        let id_json = std::fs::read_to_string(&id_info.pathname)
            .expect("Can't read InDesign LAN data file");
        let acro_oc1: OperatingConfig =
            serde_json::from_str(&acro_json).expect("Can't parse Acrobat LAN data");
        let id_oc1: OperatingConfig =
            serde_json::from_str(&id_json).expect("Can't parse InDesign LAN data");
        let acro_oc2 = ManualOperatingConfig::from_license_file(&acro_info)
            .expect("Can't manually extract Acrobat config");
        let id_oc2 = ManualOperatingConfig::from_license_file(&id_info)
            .expect("Can't manually extract Acrobat config");
        assert_eq!(
            acro_oc1.payload.npd_id, acro_oc2.npd_id,
            "Acrobat npdIds do not match"
        );
        assert_eq!(
            acro_oc1.payload.ngl_app_id, acro_oc2.app_id,
            "Acrobat appIds do not match"
        );
        assert_eq!(
            id_oc1.payload.npd_id, id_oc2.npd_id,
            "InDesign npdIds do not match"
        );
        assert_eq!(
            id_oc1.payload.ngl_app_id, id_oc2.app_id,
            "InDesign appIds do not match"
        );
        let acro_decode = serde_json::to_string(&acro_oc1).unwrap();
        let acro_ref_path = "../rsrc/OperatingConfigs/acrobat-sdl.operatingconfig";
        let acro_ref_decode =
            std::fs::read_to_string(acro_ref_path).expect("Can't read reference JSON");
        assert_eq!(acro_decode, acro_ref_decode);
        let id_decode = serde_json::to_string(&id_oc1).unwrap();
        let id_ref_path = "../rsrc/OperatingConfigs/indesign-sdl.operatingconfig";
        let id_ref_decode =
            std::fs::read_to_string(id_ref_path).expect("Can't read reference JSON");
        assert_eq!(id_decode, id_ref_decode);
    }

    #[test]
    fn test_online_package() {
        let path =
            "../rsrc/packages/mac/online-proxy-premiere/ngl-preconditioning-data.json";
        let info = FileInfo::from_path(path).expect("Can't find test data");
        let json =
            std::fs::read_to_string(&info.pathname).expect("Can't read package data");
        let pcd: PreconditioningData =
            serde_json::from_str(&json).expect("Can't parse package data");
        let ocs1 = pcd.operating_configs;
        let ocs2 = ManualOperatingConfig::from_preconditioning_file(&info)
            .expect("Can't manually extract config");
        assert_eq!(
            ocs1.len(),
            ocs2.len(),
            "Different number of OC specs and OCs?"
        );
        for i in 0..ocs1.len() {
            let oc1 = &ocs1[i].content;
            let oc2 = &ocs2[i];
            assert_eq!(oc1.payload.npd_id, oc2.npd_id, "npdIds do not match");
            assert_eq!(oc1.payload.ngl_app_id, oc2.app_id, "appIds do not match");
        }
    }
}

pub enum ActivationType {
    FrlOnline(String),
    FrlOffline,
    FrlIsolated(Vec<String>),
    FrlLan(String),
    Sdl,
    Unknown(String),
}

impl std::fmt::Display for ActivationType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ActivationType::FrlOnline(server) => {
                format!("FRL Online (server: {})", server).fmt(f)
            }
            ActivationType::FrlOffline => "FRL Offline".fmt(f),
            ActivationType::FrlIsolated(codes) => match codes.len() {
                1 => "FRL Isolated (1 census code)".fmt(f),
                n => format!("FRL Isolated ({} census codes)", n).fmt(f),
            },
            ActivationType::FrlLan(server) => {
                format!("FRL LAN (server: {})", server).fmt(f)
            }
            ActivationType::Sdl => "SDL".fmt(f),
            ActivationType::Unknown(s) => s.fmt(f),
        }
    }
}

pub enum Precedence {
    AcrobatStandard = 70,
    AcrobatPro = 100,
    CcSingleApp = 80,
    CcAllApps = 90,
}

impl std::fmt::Display for Precedence {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Precedence::AcrobatStandard => "70 (Acrobat Standard)".fmt(f),
            Precedence::CcSingleApp => "80 (CC Single App)".fmt(f),
            Precedence::CcAllApps => "90 (CC All Apps)".fmt(f),
            Precedence::AcrobatPro => "100 (Acrobat Pro)".fmt(f),
        }
    }
}
