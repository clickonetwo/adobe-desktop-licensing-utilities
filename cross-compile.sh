#!/bin/bash
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
if [ "$1" == "ubuntu" ]; then
  tgt=x86_64-unknown-linux-musl
  cc=/opt/homebrew/bin/x86_64-unknown-linux-musl-cc
  ar=/opt/homebrew/bin/x86_64-unknown-linux-musl-ar
  env TARGET_CC=$cc TARGET_AR=$ar cargo build --workspace --target=$tgt --features cross-compile
elif [ "$1" == "windows" ]; then
  tgt=x86_64-pc-windows-gnu
  cc=/opt/homebrew/bin/x86_64-w64-mingw32-gcc
  ar=/opt/homebrew/bin/x86_64-w64-mingw32-ar
  env cargo build --workspace --target=$tgt --features cross-compile
else
  echo "Don't know how to cross-compile for platform $1"
fi
