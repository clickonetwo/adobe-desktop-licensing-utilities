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
use sha2::{Digest, Sha256};

extern "C" {
    // Puts the preimage of the device ID (an ASCII C string) into a buffer passed by the caller.
    //
    // Returns the length of the returned value; this size does not include
    // the terminator byte, so it will always be less than the buffer size.
    // To obtain the Adobe device ID, caller must compute the SHA256 hash of the
    // returned string value.
    //
    // If a preimage value cannot be obtained, returns 0.  Caller should _not_
    // compute the SHA256 of the returned value.  Instead, caller should use
    // `get_adobe_device_fallback_id` to obtain the already-hashed device ID.
    //
    // If the buffer is not large enough for the return value, returns -1.
    fn get_adobe_device_preimage_id(buf: *mut u8, len: i32) -> i32;

    // Puts the fallback ID (an ASCII C string) into a buffer passed by the caller.
    //
    // Returns the length of the fallback ID (64); this size does not include
    // the terminator byte, so it will always be less than the buffer size.
    // The returned value is the (already hashed) Adobe fallback device ID.
    //
    // If the buffer is not large enough to hold the fallback ID, returns -1.
    fn get_adobe_device_fallback_id(buf: *mut u8, len: i32) -> i32;
}

pub fn get_adobe_device_id() -> String {
    let mut buf: [u8; 513] = [0; 513];
    let plen = unsafe { get_adobe_device_preimage_id(buf.as_mut_ptr(), 513) };
    if plen > 0 {
        let plen = plen as usize;
        let digest = Sha256::digest(&buf[0..plen]);
        format!("{:x}", digest)
    } else {
        let id_len = unsafe { get_adobe_device_fallback_id(buf.as_mut_ptr(), 513) };
        if id_len < 0 {
            panic!("Adobe device IDs must fit in a buffer of 512 characters!")
        }
        let id_len = id_len as usize;
        String::from_utf8_lossy(&buf[0..id_len]).to_string()
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_get_device_id() {
        let id = super::get_adobe_device_id();
        assert_eq!(id.len(), 64, "Adobe device ID is not 64 characters");
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "Adobe device ID is not a SHA256 hash!"
        );
        assert!(
            id.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()),
            "Adobe device ID is not all lowercase!"
        );
        println!("The test machine's Adobe device ID is '{}'", id);
    }
}
