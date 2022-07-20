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
use openssl::{pkey::PKey, pkey::Private, x509::X509};

#[derive(Debug, Clone)]
pub struct CertificateData {
    key: PKey<Private>,
    cert: X509,
}

impl CertificateData {
    pub fn from_key_cert_pair(key: PKey<Private>, cert: X509) -> Result<Self> {
        let data = CertificateData { key, cert };
        data.validate()?;
        Ok(data)
    }

    pub fn key_pem(&self) -> Vec<u8> {
        self.key.private_key_to_pem_pkcs8().expect("Can't encode key in PEM format")
    }

    pub fn cert_pem(&self) -> Vec<u8> {
        self.cert.to_pem().expect("Can't encode certificate in PEM format")
    }

    pub fn validate(&self) -> Result<&Self> {
        let key_pubkey = self
            .key
            .public_key_to_pem()
            .wrap_err("Can't get public key from private key")?;
        let cert_pubkey = self
            .cert
            .public_key()
            .wrap_err("Can't get public key from certificate")?
            .public_key_to_pem()
            .wrap_err("Can't encode cert pubkey in PEM form")?;
        if key_pubkey == cert_pubkey {
            Ok(self)
        } else {
            Err(eyre!("Private key and certificate do not match"))
        }
    }
}

pub fn load_pfx_file(path: &str, password: &str) -> Result<CertificateData> {
    let file = std::fs::read(path).wrap_err(format!("Can't load PFX file '{}'", path))?;
    let pkcs12 =
        openssl::pkcs12::Pkcs12::from_der(&file).wrap_err("Can't read PFX file")?;
    let parsed = pkcs12.parse(password).wrap_err("Cant parse PFX file")?;
    CertificateData::from_key_cert_pair(parsed.pkey, parsed.cert)
}

pub fn load_pem_files(
    key_path: &str,
    cert_path: &str,
    key_pass: Option<&str>,
) -> Result<CertificateData> {
    let key_data = std::fs::read(key_path)
        .wrap_err(format!("Can't load key file '{}'", key_path))?;
    let key = if let Some(pass) = key_pass {
        PKey::private_key_from_pem_passphrase(&key_data, pass.as_bytes())
            .wrap_err("Can't decrypt key data using password")?
    } else {
        PKey::private_key_from_pem(&key_data).wrap_err("Can't read key data")?
    };
    let cert_data = std::fs::read(cert_path)
        .wrap_err(format!("Can't load certificate file '{}'", cert_path))?;
    let cert = X509::from_pem(&cert_data).wrap_err("Can't parse certificate data")?;
    CertificateData::from_key_cert_pair(key, cert)
}

#[cfg(test)]
mod test {
    use openssl::pkey::PKey;

    #[test]
    fn try_load_and_compare_certificates() {
        let password = "pfx-testing";
        let key_path = "../rsrc/certificates/pfx-testing.key";
        let clear_key_path = "../rsrc/certificates/pfx-testing-clear.key";
        let cert_path = "../rsrc/certificates/pfx-testing.cert";
        let pfx_path = "../rsrc/certificates/pfx-testing.pfx";
        let key_data = std::fs::read(key_path).unwrap();
        let key = PKey::private_key_from_pem_passphrase(&key_data, password.as_bytes())
            .unwrap();
        let key_str = String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap();
        let cert_str = std::fs::read_to_string(cert_path).unwrap();
        let pfx_data = super::load_pfx_file(pfx_path, password).unwrap();
        let pfx_key_str = String::from_utf8(pfx_data.key_pem()).unwrap();
        let pfx_cert_str = String::from_utf8(pfx_data.cert_pem()).unwrap();
        assert_eq!(remove_ascii_whitespace(&pfx_key_str), remove_ascii_whitespace(&key_str));
        assert_eq!(remove_ascii_whitespace(&pfx_cert_str), remove_ascii_whitespace(&cert_str));
        let pem_data =
            super::load_pem_files(key_path, cert_path, Some(password)).unwrap();
        let pem_key_str = String::from_utf8(pem_data.key_pem()).unwrap();
        let pem_cert_str = String::from_utf8(pem_data.cert_pem()).unwrap();
        assert_eq!(remove_ascii_whitespace(&pem_key_str), remove_ascii_whitespace(&key_str));
        assert_eq!(remove_ascii_whitespace(&pem_cert_str), remove_ascii_whitespace(&cert_str));
        let pem_data = super::load_pem_files(clear_key_path, cert_path, None).unwrap();
        let pem_key_str = String::from_utf8(pem_data.key_pem()).unwrap();
        let pem_cert_str = String::from_utf8(pem_data.cert_pem()).unwrap();
        assert_eq!(remove_ascii_whitespace(&pem_key_str), remove_ascii_whitespace(&key_str));
        assert_eq!(remove_ascii_whitespace(&pem_cert_str), remove_ascii_whitespace(&cert_str));
    }
    
    fn remove_ascii_whitespace(s: &str) -> String {
        s.split_ascii_whitespace().collect::<Vec<&str>>().join("")
    }
}
