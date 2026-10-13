#!/bin/sh
# Builds and tests the native module for one Lua version on Linux or macOS,
# stores it under bundles/, then installs the packaged rock without any
# compiler and tests that copy.
#
#   sh .github/scripts/bundle.sh <lua version>
set -eu
v=$1
lua=".build/lua/$v/bin/lua"
python .github/scripts/lua.py "$v"
"$lua" scripts/build.lua ".build/lua/$v/include"
"$lua" tests/run.lua
os=$(uname -s | sed 's/Linux/linux/; s/Darwin/macosx/')
arch=$(uname -m | sed 's/aarch64/arm64/')
mkdir -p "bundles/$os-$arch/$v"
mv ws/core.so "bundles/$os-$arch/$v/core.so"
python .github/scripts/luarocks.py "$v"
python .github/scripts/package.py --partial
python .github/scripts/install.py --lua "$lua" --luarocks .build/luarocks/bin/luarocks
