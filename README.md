# Adobe Desktop License Utilities

[![CI](https://github.com/clickonetwo/adobe-desktop-licensing-utilities/workflows/ci/badge.svg)](https://github.com/clickonetwo/adobe-desktop-licensing-utilities/actions?query=workflow%3Aci)

Adobe provides three forms of desktop application licensing: Named User Licensing (NUL), Feature Restricted Licensing (FRL), and Shared Device Licensing (SDL).  This project provides utilities which can help administrators manage the deployment and use of their Adobe desktop licenses:

- The `adlu-decoder` is a command-line tool for discovering and reporting on the "operating configuration" files that control both FRL and SDL.
- The `adlu-proxy` is a web service that both:
    - facilitates the use of FRL licensing in LAN environments that are either intermittently connected to or fully isolated from the internet, and
    - facilitates the collection and analysis of usage logs from NUL-licensed applications.

Each of these utilities has their own crate inside this workspace.  See the [adlu-decoder README file](adlu-decoder/README.md) and the [adlu-proxy README file](adlu-proxy/README.md) for more info on each.

## Support

These utilities are developed and maintained by [Daniel Brotsky](mailto:dan@clickonetwo.io).  If you encounter bugs, have questions, or have feature requests, please file an issue against this project.  If your support needs are more involved, or if you are looking for custom feature development, maintenance contracts are available from [ClickOneTwo Consulting LLC](https://clickonetwo.io).

## Contributing

Contributions are very welcome.  If you have a PR to submit, please be sure to open a bug or enhancement request first explaining the issue that your PR addresses.

ClickOneTwo observes the [Contributor Covenant Code of Conduct](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) and requires that all of our collaborators behave in accordance with it.

## License and Attribution

The material in this repository is licensed under the GNU Afero General Public License v3, which is reproduced in full in the [LICENSE-AGPL](LICENSE-AGPL) file.

Some source files in this repository are derived from files in two Adobe Open Source projects: the [Adobe License Decoder](https://github.com/adobe/adobe-license-decoder.rs) and the [FRL Online Proxy](https://github.com/adobe/frl-online-proxy). The use of those materials in this work is permitted by the MIT license under which they were released. That license is reproduced here in the [LICENSE-MIT](LICENSE-MIT) file and the required attribution notice is posted both in the [COPYRIGHT](COPYRIGHT) file and in the header of all source files.  Both the LICENSE-MIT and COPYRIGHT files must be retained in any derivative work, as required by the GNU Affero General Public License v3 under which this work is licensed.
