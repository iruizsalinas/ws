-- LuaRocks build step: puts the native module at ws/core.so (ws/core.dll on
-- Windows). Uses this platform's prebuilt binary when it loads; otherwise
-- builds from source, which needs Rust (rustup.rs) and a C compiler.
local windows = package.config:sub(1, 1) == "\\"
local output = windows and "ws/core.dll" or "ws/core.so"

-- As in ws/loop.lua; neither the prebuilt module nor a build from source
-- serves other number types, and running the module there may crash.
assert(2 ^ 53 + 1 == 2 ^ 53 and 2 ^ 24 + 1 ~= 2 ^ 24 and (math.maxinteger or 2 ^ 63) > 2 ^ 62,
  "ws needs a Lua with 64-bit integers and double numbers")

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

-- Opens the module as require would: true; nil and why it does not load;
-- or false and why it refused this Lua, which a rebuild cannot change.
local function open()
  local loader, err = package.loadlib("./" .. output, "luaopen_ws_core")
  if not loader then return nil, err end
  local ok, core = pcall(loader, "ws.core")
  if ok and type(core) == "table" then return true end
  return false, ok and "ws.core cannot load into this Lua runtime" or tostring(core)
end

local target = platform()
if target and copy("bundles/" .. target .. "/" .. (windows and "core.dll" or "core.so"), output) then
  local ok, err = open()
  if ok then
    print("ws: using the prebuilt module for " .. target)
    return
  end
  assert(ok == nil, "ws: " .. tostring(err))
  print("ws: the prebuilt module does not load here (" .. tostring(err) .. "); building from source")
  os.remove(output)
else
  print("ws: no prebuilt module for " .. tostring(target) .. "; building from source")
end

local cargo = io.popen("cargo --version 2>&1")
local found = cargo and (cargo:read("*a") or ""):find("^cargo") ~= nil
if cargo then cargo:close() end
assert(found, "ws: building from source needs Rust; install it from https://rustup.rs and retry")

local lua = arg[-1] or "lua"
local q = windows and '"' or "'"
local cmd = q .. lua .. q .. " scripts/build.lua"
-- cmd.exe strips one pair of outer quotes from a command starting with one.
local ok = os.execute(windows and ('"' .. cmd .. '"') or cmd)
assert(ok == true or ok == 0, "ws: building the native module failed")
local opened, err = open()
assert(opened, "ws: the module built from source does not load: " .. tostring(err))
