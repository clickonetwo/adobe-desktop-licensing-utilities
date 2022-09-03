#!/usr/bin/env bash
#
# Copyright 2022 Daniel Brotsky. All rights reserved.
#
# All of the copyrighted work in this repository is licensed under the
# GNU Affero General Public License, reproduced in the LICENSE-AGPL file.
#
# Attribution:
#
# Some source files in this repository are derived from files in two Adobe Open
# Source projects: the Adobe License Decoder repository found at this URL:
#     https://github.com/adobe/adobe-license-decoder.rs
# and the FRL Online Proxy repository found at this URL:
#     https://github.com/adobe/frl-online-proxy
#
# The files in those original works are copyright 2022 Adobe and the use of those
# materials in this work is permitted by the MIT license under which they were
# released.  That license is reproduced here in the LICENSE-MIT file.
#
# This is a simple bash installer script.  It expects that you are running
# as a standard user with sudo privileges, and it expects that the current
# directory should be the parent of a newly-created adlu-proxy directory
# that contains the running server.  The script makes the adlu-proxy directory
# downloads the ubuntu binary to that directory, gives it the ability to listen
# to privileged ports (in case you want to do that), and then runs the proxy
# for you to create your configuration file.
#
# You need to invoke the script with release tag you want to download;
# for example, v1.0.0-alpha.2 or v1.0.1
if [ "$1"x == ""x ]
then
  echo You must specify a release tag to download, such as v1.0.0
  exit 1
fi
echo "WARNING: This will forcibly remove any existing adlu-proxy directory."
read -p "Proceed? " -n 1 -r
echo    # move to a new line
if [[ $REPLY =~ ^[Yy]$ ]]
then
  rm -rf adlu-proxy
  mkdir adlu-proxy
  if cd adlu-proxy
  then
    echo "Downloading proxy..."
  else
    echo "Couldn't create and change to adlu-proxy directory"
    exit 1
  fi
  wget -o /tmp/wget.log https://github.com/clickonetwo/adobe-desktop-licensing-utilities/releases/download/$1/adlu-proxy.ubuntu_x86_64
  if [ -f adlu-proxy.ubuntu_x86_64 ]
  then
    echo "Download succeeded"
    mv -f adlu-proxy.ubuntu_x86_64 adlu-proxy
    chmod a+x adlu-proxy
  else
    echo "Download failed! See /tmp/wget.log for details."
    exit 1
  fi
  read -p "Do you want your proxy to listen on privileged ports (e.g., 443)? " -n 1 -r
  echo
  if [[ $REPLY =~ ^[Yy]$ ]]
  then
    echo About to use sudo to allow privileged ports - you may see a password prompt
    if sudo setcap cap_net_bind_service+eip adlu-proxy
    then
      echo Proxy can use privileged ports.
    else
      echo "setcap call failed, exiting..."
      exit 1
    fi
  fi
  echo "Proxy is installed.  Configuring..."
  exec ./adlu-proxy configure
fi
