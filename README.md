# Adobe Device License Utilities

[![Rust CI (stable)](https://github.com/adobe/adobe-device-licensing-utilities/workflows/Rust%20CI%20(stable)/badge.svg)](https://github.com/adobe/adobe-license-decoder.rs/actions?query=workflow%3A%22Rust+CI+%28stable%29%22)

Adobe provides two forms of device-based licensing: Feature Restricted Licensing (FRL) and Shared Device Licensing (SDL).  This project provides two utilities which can help administrators manage the deployment and use of their device-based licenses:

- The `license-decoder` is a command-line tool for discovering and managing the "operating configuration" files that control device licensing (both FRL and SDL).
- The `frl-proxy` is a webservice that facilitates the use of FRL licensing in LAN environments that are either intermittently connected to or fully isolated from the internet.

## Attribution



## Support

These utilities are developed and maintained by [Daniel Brotsky](maito:dan@clickonetwo.io).  If you encounter bugs, have questions, or have feature requests, please file an issue against this project.  If your support needs are more involved, or if you are looking for custom feature development, maintenance contracts are available from [ClickOneTwo Consulting LLC](clickonetwo.io).

## Contributing

Contributions are welcomed! Read the [Contributing Guide](./.github/CONTRIBUTING.md) for more information.

## License and Attribution

The material in this repository is licensed under the GNU Afero General Public License, which is reproduced in full in the [LICENSE-AGPL](LICENSE-AGPL) file.

Some source files in this repository are derived from files in two Adobe Open Source projects: the [Adobe License Decoder](https://github.com/adobe/adobe-license-decoder.rs) and the [FRL Online Proxy](https://github.com/adobe/frl-online-proxy). The use of those materials in this work is permitted by the MIT license under which they were released. That license is reproduced here in the LICENSE-MIT file.
