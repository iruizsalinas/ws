-- Exercise selection and failure handling without real binaries or tools.
-- Run from the repository root: lua tests/native/install.lua
local install = assert(loadfile("scripts/native.lua"))
local real_io, real_package, real_arg = io, package, arg

local function check(case)
  local attempted, initialized, written, closed = {}, {}, nil, 0
  local filename = case.windows and "core.dll" or "core.so"
  local output = "ws/" .. filename
  io = {
    popen = function(cmd)
      assert(not case.windows, "Windows must not run a subprocess")
      assert(cmd == "uname -s", "installer must not detect the CPU or run build tools")
      if case.popen_error then error("popen is unavailable") end
      if case.no_pipe then return nil end
      return { read = function() return case.system or "Linux" end, close = function() end }
    end,
    open = function(path, mode)
      if mode == "rb" then
        local target = assert(path:match("^%./bundles/([^/]+)/" .. filename:gsub("%.", "%%.") .. "$"))
        if not case.bundles[target] then return nil end
        return {
          read = function() assert(initialized[#initialized] == target); return target end,
          close = function() closed = closed + 1 end,
        }
      end
      assert(path == output and mode == "wb")
      return { write = function(_, bytes) written = bytes; return true end, close = function() return true end }
    end,
  }
  package = {
    config = case.windows and "\\\n" or "/\n",
    loadlib = function(path, symbol)
      local target = assert(path:match("^%./bundles/([^/]+)/"))
      assert(symbol == "luaopen_ws_core" and not written)
      attempted[#attempted + 1] = target
      if target ~= case.accept then return nil, case.load_error or "wrong architecture" end
      return function(name)
        assert(name == "ws.core")
        initialized[#initialized + 1] = target
        if case.runtime_error then error(case.runtime_error, 0) end
        if case.bad_module then return false end
        return {}
      end
    end,
  }
  -- Selection must not depend on interpreter paths, options, or an arg table.
  arg = nil
  local ok, err = pcall(install)
  io, package, arg = real_io, real_package, real_arg
  if case.error then
    assert(not ok and tostring(err):find(case.error, 1, true), tostring(err))
    assert(not written, "a failed install must not write the destination")
  else
    assert(ok, tostring(err))
    assert(written == case.accept)
  end
  assert(table.concat(attempted, ",") == (case.attempted or ""))
  assert(closed == #attempted, "all input files must be closed")
end

local linux = { ["linux-x86_64"] = true, ["linux-arm64"] = true }
local macos = { ["macosx-x86_64"] = true, ["macosx-arm64"] = true }
local windows = { ["win32-x86_64"] = true }
check { bundles = {}, system = "FreeBSD", error = "no prebuilt module for FreeBSD" }
check { bundles = {}, popen_error = true, error = "no prebuilt module for this system" }
check { bundles = {}, no_pipe = true, error = "no prebuilt module for this system" }
check { bundles = {}, error = "Install the release from LuaRocks instead: luarocks install ws" }
check { bundles = linux, accept = "linux-x86_64", attempted = "linux-x86_64" }
check { bundles = linux, accept = "linux-arm64", attempted = "linux-x86_64,linux-arm64" }
check { bundles = { ["linux-arm64"] = true }, accept = "linux-arm64", attempted = "linux-arm64" }
check { bundles = macos, system = "Darwin", accept = "macosx-x86_64", attempted = "macosx-x86_64" }
check { bundles = macos, system = "Darwin", accept = "macosx-arm64", attempted = "macosx-x86_64,macosx-arm64" }
check { bundles = windows, windows = true, accept = "win32-x86_64", attempted = "win32-x86_64" }
check { bundles = windows, windows = true, attempted = "win32-x86_64", error = "It needs an x86_64 Lua" }
check { bundles = linux, attempted = "linux-x86_64,linux-arm64", error = "wrong architecture" }
check { bundles = linux, load_error = "GLIBC_2.28 not found", attempted = "linux-x86_64,linux-arm64",
  error = "GLIBC_2.28 not found" }
check { bundles = linux, load_error = "libc.so.6 not found", attempted = "linux-x86_64,linux-arm64",
  error = "musl is not supported" }
check { bundles = macos, system = "Darwin", attempted = "macosx-x86_64,macosx-arm64",
  error = "macOS 11 or newer" }
check { bundles = linux, accept = "linux-x86_64", attempted = "linux-x86_64",
  runtime_error = "ws.core needs a Lua with 64-bit integers", error = "ws.core needs a Lua with 64-bit integers" }
check { bundles = linux, accept = "linux-x86_64", attempted = "linux-x86_64",
  runtime_error = "ws.core needs a Lua with double-precision numbers", error = "double-precision numbers" }
check { bundles = linux, accept = "linux-x86_64", attempted = "linux-x86_64",
  bad_module = true, error = "the module did not return a table" }
print("installer tests passed")
