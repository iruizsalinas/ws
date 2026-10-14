-- LuaRocks build step: select a bundled module with the system loader, then
-- validate its Lua ABI before copying it. No compiler or CPU detection.
local windows = package.config:sub(1, 1) == "\\"
local filename = windows and "core.dll" or "core.so"
local system = "Windows"
if not windows then
  local ok, pipe = pcall(io.popen, "uname -s")
  system = ok and pipe and pipe:read("*l") or nil
  if ok and pipe then pipe:close() end
end

local targets = ({
  Windows = { "win32-x86_64" },
  Linux = { "linux-x86_64", "linux-arm64" },
  Darwin = { "macosx-x86_64", "macosx-arm64" },
})[system]
if not targets then
  error("ws: no prebuilt module for " .. (system or "this system") .. ". Supported systems: Linux, macOS and Windows.", 0)
end

local errors = {}
for _, target in ipairs(targets) do
  local path = "./bundles/" .. target .. "/" .. filename
  local input = io.open(path, "rb")
  if input then
    -- The loader checks the running process's architecture, not the host CPU.
    -- Probe each original path: a loaded DLL cannot be overwritten on Windows.
    local loader, err = package.loadlib(path, "luaopen_ws_core")
    if loader then
      local ok, core = pcall(loader, "ws.core")
      if not ok or type(core) ~= "table" then
        input:close()
        error("ws: the prebuilt module for " .. target .. " is not compatible with this Lua runtime: "
          .. (ok and "the module did not return a table" or tostring(core)), 0)
      end
      local bytes = input:read("*a")
      input:close()
      assert(bytes, "ws: cannot read the prebuilt module for " .. target)
      local out = assert(io.open("ws/" .. filename, "wb"))
      assert(out:write(bytes))
      assert(out:close())
      return
    end
    input:close()
    errors[#errors + 1] = target .. ": " .. tostring(err):gsub("[%s%.]+$", "")
  end
end

if #errors == 0 then
  error("ws: the prebuilt modules are missing from this package. "
    .. "Install the release from LuaRocks instead: luarocks install ws", 0)
end
local needs = ({ Linux = "an x86_64 or ARM64 Lua and glibc 2.28 or newer (musl is not supported)",
  Darwin = "an Intel or Apple Silicon Lua and macOS 11 or newer",
  Windows = "an x86_64 Lua" })[system]
error("ws: no prebuilt module could be loaded. It needs " .. needs .. ".\n"
  .. table.concat(errors, "\n"), 0)
