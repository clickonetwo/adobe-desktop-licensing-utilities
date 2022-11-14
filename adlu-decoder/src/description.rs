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
use adlu_base::Timestamp;
use adlu_parse::admin::{ActivationType, Configuration, OcFileSpec, PreconditioningData};

pub fn describe_configuration(config: &Configuration, verbose: i32) {
    match config {
        Configuration::Packaged(pcs) => {
            let mut pcs = pcs.clone();
            pcs.sort_by(|pc1, pc2| pc1.npd_id.cmp(&pc2.npd_id));
            for pc in pcs.iter() {
                describe_preconditioning_data(pc, verbose);
            }
        }
        Configuration::Installed(ocs) => {
            let mut ocs = ocs.clone();
            ocs.sort_by(|oc1, oc2| match oc1.npd_id().cmp(&oc2.npd_id()) {
                std::cmp::Ordering::Equal => oc1.app_id().cmp(&oc2.app_id()),
                otherwise => otherwise,
            });
            describe_operating_configs(&ocs, verbose);
        }
    }
}

fn describe_operating_configs(ocs: &[OcFileSpec], verbose: i32) {
    let mut current_npd_id = String::new();
    for (i, oc) in ocs.iter().enumerate() {
        if !current_npd_id.eq_ignore_ascii_case(&oc.npd_id()) {
            current_npd_id = oc.npd_id();
            println!("License files for npdId: {}:", current_npd_id);
            describe_package(oc, verbose);
            println!("Filenames (shown with '...' where the npdId appears):")
        }
        println!("{: >2}: {}", i + 1, shorten_oc_file_name(&oc.name));
        describe_app(-1, &oc.app_id(), &oc.cert_group_id(), verbose);
        if let Some(install_datetime) = oc.install_date() {
            println!("    Install date: {}", install_datetime);
        }
        // if -vv is given, check for locally cached licenses
        if verbose > 1 {
            if let Some(s) = oc.cached_expiry() {
                if let Ok(ts) = s.parse::<Timestamp>() {
                    println!(
                        "    Cached activation expires: {}",
                        ts.as_local_datetime().format("%Y-%m-%d")
                    )
                } else {
                    println!("    Invalid cached expiration")
                }
            } else {
                println!("    No cached activation")
            }
        }
    }
}

fn describe_preconditioning_data(pc_data: &PreconditioningData, verbose: i32) {
    let mut oc_data = pc_data.operating_configs.clone();
    oc_data.sort_by_key(|oc1| oc1.app_id());
    for (i, oc) in pc_data.operating_configs.iter().enumerate() {
        if i == 0 {
            println!("Preconditioning data for npdId: {}", &oc.npd_id());
            describe_package(oc, verbose);
            println!("Application Licenses:")
        }
        describe_app(i as i32, &oc.app_id(), &oc.cert_group_id(), verbose);
    }
}

fn describe_package(oc: &OcFileSpec, verbose: i32) {
    if verbose > 0 {
        println!("    Package License ID: {}", oc.npd_id());
    }
    println!("    License type: {}", oc.activation_type());
    if verbose > 0 {
        if let ActivationType::FrlIsolated(codes) = oc.activation_type() {
            if codes.len() == 1 {
                println!("    Census code: {}", codes[0]);
            } else {
                println!("    Census codes: {}", codes.join(", "));
            }
        }
    }
    println!("    License expiry date: {}", oc.expiry_date());
    println!("    Precedence: {}", oc.precedence());
}

fn describe_app(count: i32, app_id: &str, group_id: &str, verbose: i32) {
    println!(
        "{}App ID: {}{}",
        if count < 0 { String::from("    ") } else { format!("{: >2}: ", count + 1) },
        app_id,
        if verbose > 0 {
            format!(", Certificate Group: {}", group_id)
        } else {
            String::new()
        }
    );
}

pub fn shorten_oc_file_name(name: &str) -> String {
    let parts: Vec<&str> = name.split('-').collect();
    if parts.len() != 3 {
        name.to_string()
    } else {
        format!("{}-...-{}", parts[0], parts[2])
    }
}
