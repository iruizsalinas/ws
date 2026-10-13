#!/bin/sh
# Builds the native module once for Linux or macOS, tests it with every Lua
# version, stores it under bundles/, then installs the packaged rock into
# each Lua without any compiler and tests those copies.
#
#   sh .github/scripts/bundle.sh
set -eu
versions="5.1 5.2 5.3 5.4 5.5"
for v in $versions; do
  python .github/scripts/lua.py "$v"
done
.build/lua/5.4/bin/lua scripts/build.lua
for v in $versions; do
  echo "== Lua $v"
  ".build/lua/$v/bin/lua" tests/run.lua
  python tests/native/strings.py --lua ".build/lua/$v/bin/lua"
done
os=$(uname -s | sed 's/Linux/linux/; s/Darwin/macosx/')
python tests/native/abi.py
arch=$(uname -m | sed 's/aarch64/arm64/')
mkdir -p "bundles/$os-$arch"
mv ws/core.so "bundles/$os-$arch/core.so"
python .github/scripts/luarocks.py 5.4
python .github/scripts/package.py --partial
for v in $versions; do
  echo "== Installed into Lua $v"
  python .github/scripts/install.py --lua ".build/lua/$v/bin/lua" --luarocks .build/luarocks/bin/luarocks
done
