---
layout: home
title: Home
nav_order: 1
---

# Adobe Desktop Licensing Utilities

The [*Adobe Desktop Licensing Utilities*](https://github.com/clickonetwo/adobe-desktop-licensing-utilities) (ADLU) are a collection of command-line utilities and hosted services that make it easier for system administrators to deploy, manage, and track the use of their Adobe desktop software:

- The `adlu-decoder` is a command-line tool for discovering and reporting on the license packages used to administer Adobe feature-restricted and shared-device licensing.
- The `adlu-proxy` is a web service that:
  - facilitates the use of feature-restricted licensing in network environments that are either intermittently connected to or fully isolated from the internet, and
  - facilitates the monitoring of application usage by user and machine in all licensing environments.

All the programs in the ADLU are open-source and available for free under the terms of the [GNU Affero General Public License](https://www.gnu.org/licenses/agpl-3.0.html). Support contracts for the ADLU are available from [ClickOneTwo Consulting LLC](https://clickonetwo.io).

## Roadmap

[This site](https://clickonetwo.github.io/adobe-device-license-utilities) provides up-to-date documentation for planning and managing application deployments using the ADLU.  There are three groups of documents:

* Introductory. This group provides the background information necessary to make effective use of the programs in the ADLU.  It has two documents: a [primer on Adobe Licensing](./primer.md) and a [desktop glossary](./glossary.md). The primer explains how Adobe desktop apps communicate with the Adobe servers, and the glossary explains the various bits and pieces of technology on the desktop that apps require when licensing.  

* Illustrative. This group provides examples of how the ADLU programs might be used in various customer scenarios.   There is a [proxy overview](proxy-overview.md) and a [decoder overview](decoder-overview.md).

* Operational. This group provides detailed instructions for installing, configuring, and operating the programs in the ADLU.  There is a [proxy operations guide](./proxy-operation.md) and a [decoder operations guide](./decoder-operation.md).


## Copyright

Copyright 2022 Daniel Brotsky. All rights reserved.

This documentation, like all parts of the ADLU, is open-source and available for free under the terms of the [GNU Affero General Public License](https://www.gnu.org/licenses/agpl-3.0.html).
