-- Autobahn echo client: `lua .github/autobahn/client.lua <url>`.
package.path = "./?.lua;./?/init.lua;" .. package.path
package.cpath = "./?.so;./?.dll;" .. package.cpath
local ws = require("ws")

local base, agent = arg[1], "ws"

local function open(path, on_message, on_close)
  local c = ws.connect(base .. path, { limit = 64 * 1024 * 1024 })
  -- Many cases fail the connection on purpose.
  c:on("error", function() end)
  c:on("message", function(data, binary) on_message(c, data, binary) end)
  c:on("close", on_close)
end

local count, case = 0, 1

local function run()
  if case > count then
    return open("/updateReports?agent=" .. agent, function() end, function() end)
  end
  if case % 50 == 0 then
    print(("case %d/%d"):format(case, count))
  end
  local echo = function(c, data, binary) c:send(data, binary) end
  open(("/runCase?case=%d&agent=%s"):format(case, agent), echo, function()
    case = case + 1
    run()
  end)
end

open("/getCaseCount", function(_, data) count = tonumber(data) end, run)
ws.run()
