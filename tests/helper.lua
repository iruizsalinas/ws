package.path = './?.lua;../?.lua;../?/init.lua;../ws/?.lua;' .. package.path

local M = {}
M.count = 0
M.failed = 0
M.name = "unknown"

function M.init(name)
  M.name = name
  M.count = 0
  M.failed = 0
end

function M.check(description, condition)
  M.count = M.count + 1
  if not condition then
    M.failed = M.failed + 1
    io.stderr:write("  FAIL: " .. description .. "\n")
  end
end

function M.check_equal(description, got, expected)
  M.count = M.count + 1
  if got ~= expected then
    M.failed = M.failed + 1
    io.stderr:write("  FAIL: " .. description ..
      " (expected " .. tostring(expected) ..
      ", got " .. tostring(got) .. ")\n")
  end
end

function M.check_error(description, fn)
  M.count = M.count + 1
  local ok, err = pcall(fn)
  if ok then
    M.failed = M.failed + 1
    io.stderr:write("  FAIL: " .. description .. " (expected error, got none)\n")
  end
end

function M.finish()
  if M.failed > 0 then
    print("FAIL: " .. M.name .. " (" .. M.failed .. "/" .. M.count .. " failed)")
    os.exit(1)
  else
    print("PASS: " .. M.name .. " (" .. M.count .. " tests)")
  end
end

return M
