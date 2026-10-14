-- LuaRocks build step: puts the native module at ws/core.so (ws/core.dll on
-- Windows). Installs and validates this platform's prebuilt binary.
local windows = package.config:sub(1, 1) == "\\"
local output = windows and "ws/core.dll" or "ws/core.so"

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
  if name and (arch == "x86_64" or arch == "arm64") then return name .. "-" .. arch end
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

-- Opens the module as require would, including its runtime checks. On
-- failure, also returns whether the system loader refused the file.
local function open()
  local loader, err = package.loadlib("./" .. output, "luaopen_ws_core")
  if not loader then return false, err, true end
  local ok, core = pcall(loader, "ws.core")
  if ok and type(core) == "table" then return true end
  return false, ok and "it is not compatible with this Lua runtime" or tostring(core)
end

-- What the system needs when it refuses a prebuilt module.
local NEEDS = { linux = "glibc 2.28 or newer", macosx = "macOS 11 or newer" }

local target = platform()
if not target then
  error("ws: no prebuilt module for this platform. Supported platforms: "
    .. "linux-x86_64, linux-arm64, macosx-x86_64, macosx-arm64, win32-x86_64.", 0)
end
if not copy("bundles/" .. target .. "/" .. (windows and "core.dll" or "core.so"), output) then
  error("ws: the prebuilt module for " .. target .. " is missing from this package. "
    .. "Install the release from LuaRocks instead: luarocks install ws", 0)
end
local ok, err, system = open()
if not ok then
  os.remove(output)
  -- Windows system messages end in a period and a line break.
  local reason = tostring(err):gsub("[%s%.]+$", "")
  local needs = system and NEEDS[target:match("^[^-]+")]
  error("ws: the prebuilt module for " .. target .. " failed to load: " .. reason
    .. (needs and (". It needs " .. needs .. ".") or ""), 0)
end
