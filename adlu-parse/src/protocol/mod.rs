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
pub use frl::{
    FrlActivationRequestBody, FrlActivationResponseBody, FrlAppDetails,
    FrlDeactivationQueryParams, FrlDeactivationResponseBody, FrlDeviceDetails,
};
pub use log::{LogSession, LogUploadResponse};
pub use named_user::{
    LicenseSession, NulAppDetails, NulDeviceDetails, NulLicenseRequestBody,
    NulLicenseResponseBody,
};
pub use request::{Request, RequestType};

mod frl;
mod log;
mod named_user;
mod request;
