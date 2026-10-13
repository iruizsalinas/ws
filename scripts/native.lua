-- LuaRocks build step: puts the native module at ws/core.so (ws/core.dll on
-- Windows). Uses the prebuilt binary for this platform when it loads in the
-- running Lua; otherwise builds from source, which needs Rust (rustup.rs).
--
--   lua scripts/native.lua <LUA_INCDIR> <LUA_LIBDIR> <LUA_BINDIR> <LUALIB>
local incdir, libdir, bindir, lualib = arg[1], arg[2], arg[3], arg[4]
local windows = package.config:sub(1, 1) == "\\"
local output = windows and "ws/core.dll" or "ws/core.so"
-- LuaJIT speaks the Lua 5.1 ABI.
local version = assert(_VERSION:match("Lua (%d+%.%d+)"))

local function platform()
  if windows then
    -- The running interpreter's PE header says whether it is x86_64.
    local exe = io.open(arg[-1] or "", "rb")
    if not exe then return nil end
    exe:seek("set", 60)
    local b = exe:read(4) or ""
    local offset = (b:byte(1) or 0) + (b:byte(2) or 0) * 256 + (b:byte(3) or 0) * 65536
    exe:seek("set", offset + 4)
    local machine = exe:read(2)
    exe:close()
    return machine == "\100\134" and "win32-x86_64" or nil
  end
  local pipe = io.popen("uname -sm")
  local os_name, arch = (pipe and pipe:read("*l") or ""):match("^(%S+)%s+(%S+)")
  if pipe then pipe:close() end
  arch = ({ aarch64 = "arm64", amd64 = "x86_64" })[arch] or arch
  local name = ({ Linux = "linux", Darwin = "macosx" })[os_name]
  return name and (name .. "-" .. arch) or nil
end

local function copy(from, to)
  local input = io.open(from, "rb")
  if not input then return false end
  local bytes = input:read("*a")
  input:close()
  local out = assert(io.open(to, "wb"))
  assert(out:write(bytes))
  assert(out:close())
  return true
end

local target = platform()
if target and copy("bundles/" .. target .. "/" .. version .. "/" .. (windows and "core.dll" or "core.so"), output) then
  -- Loading verifies the Lua ABI and, on Windows, the Lua DLL name.
  local loader, err = package.loadlib("./" .. output, "luaopen_ws_core")
  if loader then
    print("ws: using the prebuilt module for " .. target .. " / Lua " .. version)
    return
  end
  print("ws: the prebuilt module does not load here (" .. tostring(err) .. "); building from source")
  os.remove(output)
else
  print("ws: no prebuilt module for " .. tostring(target) .. " / Lua " .. version .. "; building from source")
end

local cargo = io.popen("cargo --version 2>&1")
local found = cargo and (cargo:read("*a") or ""):find("^cargo") ~= nil
if cargo then cargo:close() end
assert(found, "ws: building from source needs Rust; install it from https://rustup.rs and retry")

local library
if windows then
  for _, dir in ipairs({ bindir, libdir }) do
    local f = dir and lualib and io.open(dir .. "\\" .. lualib, "rb")
    if f then
      f:close()
      library = dir .. "\\" .. lualib
      break
    end
  end
  assert(library, "ws: cannot find the Lua library " .. tostring(lualib))
end
local lua = arg[-1] or "lua"
local q = windows and '"' or "'"
local cmd = q .. lua .. q .. " scripts/build.lua " .. q .. incdir .. q
if library then
  cmd = cmd .. " " .. q .. library .. q
end
-- cmd.exe strips one pair of outer quotes from a command starting with one.
local ok = os.execute(windows and ('"' .. cmd .. '"') or cmd)
assert(ok == true or ok == 0, "ws: building the native module failed")
