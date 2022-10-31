#!/usr/bin/env bash

# This is a simple bash installer script.  It expects that the current
# directory should be the parent of a newly-created adlu-proxy directory
# that contains the running server.  The script makes the adlu-proxy directory,
# downloads the ubuntu binary to that directory, and then runs the proxy
# to update (or create via interview) the proxy configuration file.
#
# You need to invoke the script with release tag you want to download;
# for example, v1.0.0-alpha.2 or v1.0.1
if [ "$1"x == ""x ]
then
  echo You must specify a release tag to download, such as v1.0.0
  exit 1
fi
echo "WARNING: This will forcibly replace any existing adlu-proxy."
read -p "Proceed? " -n 1 -r
echo    # move to a new line
if [[ $REPLY =~ ^[Yy]$ ]]
then
  mkdir -p /home/adlu/adlu-proxy
  if cd /home/adlu/adlu-proxy
  then
    echo "Downloading proxy..."
  else
    echo "Couldn't create and change to adlu-proxy directory"
    exit 1
  fi
  rm -f adlu-proxy.ubuntu_x86_64
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
  echo "Proxy is installed.  Configuring..."
  exec ./adlu-proxy configure --repair
fi
