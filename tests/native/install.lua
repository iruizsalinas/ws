-- Exercise the LuaRocks installer without touching real binaries or tools.
-- Run from the repository root: lua tests/native/install.lua
local install = assert(loadfile("scripts/native.lua"))
local real_io, real_os, real_package, real_print = io, os, package, print

local function check(case)
  local copied, removed, loaded = false, false, false
  io = {
    popen = function(cmd)
      assert(cmd == "uname -sm", "installer tried to run a build tool: " .. cmd)
      return { read = function() return case.platform or "Linux x86_64" end, close = function() end }
    end,
    open = function(path, mode)
      if mode == "rb" then
        assert(path == "bundles/linux-x86_64/core.so")
        if case.missing then return nil end
        return { read = function() return "binary" end, close = function() end }
      end
      assert(path == "ws/core.so" and mode == "wb")
      return {
        write = function(_, bytes) assert(bytes == "binary"); copied = true; return true end,
        close = function() return true end,
      }
    end,
  }
  os = { remove = function(path) assert(path == "ws/core.so"); removed = true end }
  package = {
    config = "/\n",
    loadlib = function(path, symbol)
      assert(path == "./ws/core.so" and symbol == "luaopen_ws_core" and copied)
      loaded = true
      if case.load_error then return nil, case.load_error end
      return function(name)
        assert(name == "ws.core")
        if case.runtime_error then error(case.runtime_error, 0) end
        if case.bad_module then return false end
        return {}
      end
    end,
  }
  print = function() end
  local ok, err = pcall(install)
  io, os, package, print = real_io, real_os, real_package, real_print
  if case.error then
    assert(not ok and tostring(err):find(case.error, 1, true), tostring(err))
  else
    assert(ok, tostring(err))
  end
  assert(copied == (case.copied or false))
  assert(loaded == (case.copied or false))
  assert(removed == (case.removed or false))
end

check { platform = "FreeBSD amd64", error = "no prebuilt module for this platform" }
check { platform = "Linux riscv64", error = "no prebuilt module for this platform" }
check { missing = true, error = "Install the release from LuaRocks instead: luarocks install ws" }
check { copied = true, removed = true, load_error = "GLIBC_2.28 not found", error = "GLIBC_2.28 not found. It needs glibc 2.28 or newer." }
check { copied = true, removed = true, runtime_error = "ws.core needs a Lua with 64-bit integers", error = "failed to load: ws.core needs a Lua with 64-bit integers" }
check { copied = true, removed = true, bad_module = true, error = "it is not compatible with this Lua runtime" }
check { copied = true }
print("installer tests passed")
