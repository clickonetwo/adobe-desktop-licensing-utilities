---
layout: default
title: Desktop Glossary
nav_order: 3
---

# A Glossary of the Adobe Desktop

This document contains definitions of the terms and descriptions of the components used by Adobe when doing application licensing on Mac and Windows desktops.  It’s organized alphabetically.  See the [licensing primer](./primer.md) for an introduction that uses many of these terms in context.

##### Activation Count

In [named-user licensing](#named-user-licensing), the [Adobe License Server](#adobe-license-server) keeps track, for each user, of the number of different [device IDs](#device-ids) to which it issues a valid [license profile](#license-profile): this number is called the _activation count_ for that user.

##### Adobe Admin Console

The [_Adobe Admin Console_](https://adminconsole.adobe.com) is a web application hosted by Adobe. It is used by administrators to deploy and manage their users, software and services. (It has other uses as well, but they are not relevant to the ADLU.)

##### Adobe License Server

This is the customer-visible server cluster responsible for receiving licensing requests from client applications, determining the appropriate license profile for that application at that time, signing the profile to prove it comes from Adobe, and returning it to the client.

The license server is reachable via HTTPS at two different DNS addresses: `lcs-cops.adobe.io` and `lcs-cops-proxy.adobe.com`.  The first of those addresses is always used by [named-user licensing](#named-user-licensing) clients, and it provides endpoints in different regions based on the geography of the client so as to minimize latency. The latter address refers to a fixed-IP load balancer that passes traffic to one of the US clusters. It is meant for use only by feature-restricted licensing customers whose outbound proxies require fixed IPs for allow-listing.

##### Adobe Licensing Tooklit

The _Adobe Licensing Toolkit_ is a command line application—`adobe-licensing-toolkit` on Mac and `adobe-licensing-toolkit.exe` on Windows—that is used to manage the installation and uninstallation of [feature-restricted](#feature-restricted-licensing) [license-only packages](#license-only-packages).  The latest version of the toolkit can be downloaded directly from the [Adobe Admin Console](#adobe-admin-console), and it’s also contained in every license-only package.

##### Adobe Log Server

This is the customer-visible server cluster that receives [licensing log](#licensing-logs) uploads from desktop applications. It allows Adobe to notice and debug licensing issues that happen across the customer base.

The log server is reachable via HTTPS at one DNS address: `lcs-ulecs.adobe.io`.

##### ADLU Decoder

The _ADLU decoder_ is a command-line utility that runs on Mac and Windows.  It’s part of the [Adobe Desktop Licensing Utilities](https://github.com/clickonetwo/adobe-desktop-licensing-utilities).  See the [ADLU documentation](./index.md) for more details.

##### ADLU Proxy

The _ADLU proxy_ is a web service that runs on Mac, Windows, and Linux.  It’s part of the [Adobe Desktop Licensing Utilities](https://github.com/clickonetwo/adobe-desktop-licensing-utilities).  See the [ADLU documentation](./index.md) for more details.

##### Application-Only Package

An _application-only package_ is a [package](#package) for a desktop product that only contains installers for the desktop applications in that product.  Application-only packages for [named-user licensing](#named-user-licensing) deployments additionally contain the Creative Cloud Desktop application; packages for [feature-restricted licensing](#feature-restricted licensing) deployments do not.  In all other respects there is no difference between the two.

##### Combined Package

A _combined_ package is a [feature-restricted licensing](#feature-restricted licensing) [package](#package) for a desktop product that contains both installers for the desktop applications in that product and installers for the [operating configurations](#operating-configuration) for the desktop applications in that product.  Combined packages contain a top-level installer that both installs the operating configurations and invokes the application installers.  They can’t be used to install just one or the other, and this often leads to problems when customers use them to install updates to product (because they also install a second set of operating configurations).  In general, the use of combined packages is a bad idea: Adobe recommends that customers create separate application-only and license-only packages.  (This still allows for simultaneous installation of both by a script that does both installs, but it avoids the issues associated with having multiple operating configurations for the same application.)

##### Desktop Application

An application from Adobe that runs on a desktop computer operating system (typically Mac or Windows).  Obvious examples of applications are editing applications such as Photoshop and Premiere Pro.  Less obvious examples are companion applications such as Bridge or Media Encoder.  Even less obvious examples are utility applications such as the Creative Cloud Desktop and CCLibrary (which is a faceless application that connects desktops with the CC Cloud).

##### Desktop Product

A product offering from Adobe which includes the rights to use one or more [desktop applications](#desktop-application).  For example, the Photoshop product includes the right to use the Photoshop and Bridge applications, as well as the right to run the Photoshop-on-iPad program (which is _not_ a desktop application).  The most common desktop products purchased by Enterprise customers are Creative Cloud All Apps and Creative Cloud Single App.

Because many Adobe desktop products carry the name of their “flagship application,” many people (including Adobe employees as well as customers) confuse the application and the product.  To keep them clear, it may be helpful to remember this chain of events:

1. A customer purchases a _desktop product_ P.

2. That purchase gives the customer the right to use a _desktop application_ A.

3. The right to use A gives rise to a [license profile](#license-profile) for A.
4. When the customer launches A, it retrieves that license profile from Adobe.

So it is the purchase of the _desktop product_ which creates the license profile that is required by the _desktop application_ when it runs.

##### Device ID

Adobe applications assign a unique identifier, called a *device ID*, to each desktop computer that they run on.  This device ID is obtained by computing an non-reversible hash function of a variety of hardware characteristics of the device (such as serial numbers).

Whenever an application requests a [license profile](#license-profile) from the [Adobe License Sever](#adobe-license-server), it includes the device ID of the machine it’s running on.  The license profile returned from the server contains that device ID, which ensures it can only be used on the device from which it was requested. 

##### Device Token

Another name for an [identity token](#identity-token).  The term *identity token* is preferred, because the term *device token* is often confused with the term [device id](#device-id), but the two have very little to do with one another.

##### Feature-Restricted Licensing

In _feature-restricted licensing_, license requests sent to Adobe by applications include a [package ID](#package-id) that identifies a [license package](#license-package) previously installed on that machine by an administrator. The package ID is then used by the [Adobe License Server](#adobe-license-server) to look up the [desktop product](#desktop-product) associated with the license package, and the [desktop applications](#desktop-application) included in that product determine the [license profile](#license-profile) returned by the server.

##### Identity Token

Also called a _device token_, an identity token is a [JSON web token](https://jwt.io) issued by the Adobe Identity Server to a particular signed-in individual on a particular machine (either a desktop computer or a mobile device).  Identity tokens don’t actually contain personally identifiable information; they contain one or more identifiers that the Adobe Identity Server can use to look up an individual’s customer profile with Adobe.

##### License Count

In [feature-restricted licensing](#feature-restricted-licensing), the [Adobe License Server](#adobe-license-server) keeps track, for each [desktop product](#desktop-product), of the number of different [device IDs](#device-ids) to which it issues a valid [license profile](#license-profile): this number is called the _license count_ for that product.  In the desktop product overview card in the Adobe Admin Console, this license count is shown as the number of licenses _used_ for that product.

##### License-Only Package

A _license-only package_ is one that contains installers for desktop product license data but not the installer for any of the desktop applications the product licenses.  License-only packages, when unzipped, are a folder containing one executable—the [Adobe Licensing Toolkit](#adobe-licensing-toolkit)—and one data file—`ngl-configuration-data.json`.  The data file contains [operating configurations](#operating-configuration) for all the applications associated with the package’s desktop product. Those operating configurations are installed on target machines by invoking the licensing toolkit as an administrator and giving it the path to the data file in an installation command.

##### License Profile

This is an Adobe-signed JSON data structure that applications receive from the [Adobe License Server](#adobe-license-server).  The parameters in it control various aspects of application behavior, chief among them being whether or not the application will run, and if so what features it will make available for use.

The phrase _license profile_ is often shortened to _license_, but that’s misleading for two reasons:

1. License profiles control many aspects of application behavior that are not related to the details of a customer license, such as log uploads.
2. All applications require a license profile to function, even ones that customers don’t need a license for.

License profiles are really just behavioral profiles for the application; that is, they are a form of configuration sent by the server.  The term “license” remains from the days when the only control on behavior was “run” or “don’t run” and that was determined by whether the customer had a license for the app.

##### Licensing Logs

All Adobe desktop applications keep logs of their licensing activity.  These logs do not contain personally identifiable information about the user who was running the application.  But they do contain client-generated session and request IDs that can be matched up with those in Adobe server-side logs for debugging purposes. They also contain the OS profile of the machine and, in named-user scenarios, a non-reversible hash of data from the user’s [identity token](#identity-token) that can be used to separate logs made for different users.

Licensing log files are kept in an `NGL` folder in a user-specific log directory (`~/Library/Logs` on Mac, `???` on Windows).  The name of the log file starts with the word `NGLClient` followed by a form of the application name and internal version number.  The log file generated by the current (or most recent) launch end have a `.log` suffix immediately after the internal version number.  But when the app is launched again later, the file from the immediately prior launch is renamed to have a launch date after the version number before the `.log` suffix.

One of the behavioral controls that are sent from the [Adobe License Server](#adobe-license-server) to each application in its [license profile](#license-profile) is a set of parameters that control when (and if) applications try to upload their logs to the Adobe Log Server. 

##### Named-User Licensing

In named-user licensing, license requests by applications include an [identity token](#identity-token) that identifies the user to Adobe.  The users’s ID is then used by the [Adobe License Server](#adobe-license-server) to look up the [desktop products](#desktop-product) that user has purchased or been assigned to use, and the [desktop applications](#desktop-application) included in those products determine the [license profile](#license-profile) returned by the server.

##### Next Generation Licensing

The Next Generation Licensing project (known as NGL) was a development effort at Adobe that created the current generation of desktop application licensing technology.  Although NGL was an internal code name, it still persists in some of the licensing artifacts on the desktop, such as the ``NGLClient`` prefix in the names of all the [licensing logs](#licensing-logs) and the `ngl-configuration-data.json` file in [license-only packages](#license-only packages).

##### NGL

An acronym for [Next Generation Licensing](#next-generation-licensing).

##### Operating Configuration

An _operating configuration_ is a JSON data file with the `.operatingconfig` extension.  It contains an application-specific portion of a license package.  The data in an operating configuration is used by the application when it launches to make a feature-restricted licensing request to the Adobe License Server.

##### OS Credential Store

An _OS Credential Store_ is a secure database provided by the operating system that allows each OS user to store credential data that cannot be accessed by any other user.  On Mac systems, the OS credential store is the _Keychain Services_ subsystem and the _Keychain Application_ is its UI.  On Windows systems, the OS credential store is the _Credential Manager_ (which provides both API and UI).

##### Package

A _package_ is a zipped-up folder containing programs and data that administrators can generate on the [Adobe Admin Console](#adobe-admin-console).  Some packages, such as those used in [named-user licensing](#named-user-licensing) deployments, contain only installers for applications.  Other packages, such as those used in [feature-restricted licensing](#feature-restricted-licensing) deployments, can contain both application installers and installers for license data.

##### Package ID

A unique ID assigned to a [license-package](#license-package) by the [Adobe Admin Console](#adobe-admin-console).  Package IDs are actually [UUIDs](https://en.wikipedia.org/wiki/Universally_unique_identifier), but the Admin Console always shows them in a base-64 URL-encoded version of their canonical form, rather than in canonical form directly.  So they look like this:

​	```Y2M4ZmUzYTUtMjUwZC00NDNkLTliMjMtNmM0YTQ4MzgyYTg5```

rather than like this:

​	```cc8fe3a5-250d-443d-9b23-6c4a48382a89```

The [ADLU decoder](#adlu-decoder) also shows them in encoded form, to match the admin console.

