# adlu-parse

This module knows how to parse, interpret, and generate all the device-license-related structures used by NGL.  There are three categories of these:

- Administrative configuration data (see the `admin` module).  This includes installation packge data and installed packages (aka "operating configs").
- User license data (see the `user` module).  This includes the cached forms of licenses (aka "Adobe Signed NGL Profiles" or ASNPs).
- On-the-wire request/response data (see the `protocol` module).  This includes requests for activation and deactivation of device licenses.

There are a lot of shared components among these various objects.  For example, almost all forms of NGL data are transmitted and store as doubly-signed base64 with custom signature block formats.  For another, the protocol and cached forms of ASNPs are pretty much the same.
