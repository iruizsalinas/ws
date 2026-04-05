local test_files = {
  "test_compat.lua",
  "test_sha1.lua",
  "test_base64.lua",
  "test_utf8.lua",
  "test_validation.lua",
  "test_buffer.lua",
  "test_emitter.lua",
  "test_limiter.lua",
  "test_url.lua",
  "test_extension.lua",
  "test_subprotocol.lua",
  "test_frame.lua",
  "test_receiver.lua",
  "test_handshake.lua",
  "test_websocket.lua",
  "test_stress.lua",
}

local lua = arg[-1] or "lua"
local failed = {}
local total_pass = 0

for _, file in ipairs(test_files) do
  local cmd = lua .. " " .. file .. " 2>&1"
  local handle = io.popen(cmd)
  local output = handle:read("*a")
  local ok = handle:close()
  io.write(output)
  if not ok or output:find("^FAIL") then
    failed[#failed + 1] = file
  else
    total_pass = total_pass + 1
  end
end

print("")
print(string.rep("-", 40))
if #failed == 0 then
  print("ALL " .. total_pass .. " TEST FILES PASSED")
else
  print(total_pass .. " passed, " .. #failed .. " failed:")
  for _, f in ipairs(failed) do
    print("  " .. f)
  end
  os.exit(1)
end
