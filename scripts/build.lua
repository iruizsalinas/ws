-- Builds the native module for the Lua running this script.
--
--   lua scripts/build.lua <lua include dir> [lua library] [output]
--
-- The library (the Lua DLL) is required on Windows only. The output
-- defaults to ws/core.so, or ws/core.dll on Windows. Needs cargo and a C
-- compiler; on Windows, the x86_64-pc-windows-gnu target and MinGW gcc.
local windows = package.config:sub(1, 1) == "\\"
local include, library, output = arg[1], arg[2], arg[3]
assert(include, "usage: lua scripts/build.lua <lua include dir> [lua library] [output]")
assert(not windows or library, "on Windows, pass the Lua DLL to link against")

-- Cargo runs build scripts from the crate folder, so pass absolute paths.
local function absolute(path)
  if not path or path:find("^/") or path:find("^%a:[/\\]") or path:find("^\\\\") then
    return path
  end
  local cwd = windows and io.popen("cd"):read("*l") or os.getenv("PWD")
  return cwd .. (windows and "\\" or "/") .. path
end
include, library = absolute(include), absolute(library)

local function quote(s)
  if windows then
    assert(not s:find('["%%!]'), "unsupported characters in path: " .. s)
    return '"' .. s .. '"'
  end
  return "'" .. s:gsub("'", "'\\''") .. "'"
end

local function env(name, value)
  if windows then
    return 'set "' .. name .. "=" .. value .. '" && '
  end
  return name .. "=" .. quote(value) .. " "
end

local function run(cmd)
  print("+ " .. cmd)
  local ok, _, code = os.execute(cmd)
  assert(ok == true or ok == 0, "command failed (" .. tostring(code) .. ")")
end

local target = windows and "x86_64-pc-windows-gnu" or nil
local cmd = env("LUA_INCDIR", include) .. (library and env("LUA_LIB", library) or "")
cmd = cmd .. "cargo build --release --locked -p ws-native" .. (target and (" --target " .. target) or "")
run(cmd)

local dir = "target/" .. (target and (target .. "/") or "") .. "release/"
local built
for _, name in ipairs({ "ws_native.dll", "libws_native.so", "libws_native.dylib" }) do
  local f = io.open(dir .. name, "rb")
  if f then
    f:close()
    built = dir .. name
    break
  end
end
assert(built, "cargo produced no library in " .. dir)

output = output or (windows and "ws/core.dll" or "ws/core.so")
local input = assert(io.open(built, "rb"))
local bytes = input:read("*a")
input:close()
local out = assert(io.open(output, "wb"))
assert(out:write(bytes))
assert(out:close())
print("built " .. output)
