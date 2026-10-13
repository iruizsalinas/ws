-- Builds ws/core.so (ws/core.dll on Windows), which serves every Lua version.
--
--   lua scripts/build.lua
--
-- Needs cargo and a C compiler; on Windows, the x86_64-pc-windows-gnu target
-- and MinGW gcc.
local windows = package.config:sub(1, 1) == "\\"
local cmd = (windows and 'set "WS_GLUE=1" && ' or "WS_GLUE=1 ")
  .. "cargo build --release --locked -p ws-native --message-format=json-render-diagnostics"
  .. (windows and " --target x86_64-pc-windows-gnu" or "")
print("+ " .. cmd)

-- Cargo reports where it put the library, wherever its target dir is.
local pipe = assert(io.popen(cmd))
local built
for line in pipe:lines() do
  if line:find('"reason":"compiler-artifact"', 1, true) and line:find('"name":"ws_native"', 1, true) then
    for name in (line:match('"filenames":%[(.-)%]') or ""):gmatch('"(.-)"') do
      local path = name:gsub("\\\\", "\\")
      if path:match("%.dll$") or path:match("%.so$") or path:match("%.dylib$") then built = path end
    end
  end
end
-- Lua 5.1 and LuaJIT do not report the exit status; a failed build still
-- reports no library.
assert(pipe:close() and built, "cargo build failed")

local output = windows and "ws/core.dll" or "ws/core.so"
local input = assert(io.open(built, "rb"))
local bytes = input:read("*a")
input:close()
local out = assert(io.open(output, "wb"))
assert(out:write(bytes))
assert(out:close())
print("built " .. output .. " from " .. built)
