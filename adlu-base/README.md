# adlu-base

This package contains all the utility functionality that the device licensing utilities require.  There are three basic categories:

## Encoding/Decoding

Since NGL traffics in signed JSON data, it uses a lot of base64-URL encoding for transmission of that data.  So there are a bunch of utilities here for base64 encoding and decoding of JSON from strings and files, as well as a codecs for serde that handle both encoded JSON structures and templated JSON data that includes base64-encoded segments.

Since NGL uses a lot of millisecond-accurate epoch timestamps, this module includes a timestamp converter.

## Platform Access

NGL uses the secure store on each platform, so there are utilities here for reading platform secure storage.

NGL assigns a platform-specific hardware ID which this module can provide (see the `ngl` sub-module).

The adlu-proxy server detects interrupts, so this module provides functionality for that (see the `signal` submodule).

## OpenSSL Functionality

The adlu-proxy server makes use of SSL certificates for secure connections, so this module provides functionality for that (see the `certificate` submodule).
