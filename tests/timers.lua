local H = require("tests.helper")
local ws = require("ws")

H.test("timers fire in order", function()
  local order = {}
  ws.after(0.05, function() order[#order + 1] = "b" end)
  ws.after(0.01, function() order[#order + 1] = "a" end)
  ws.after(0, function() order[#order + 1] = "now" end)
  H.run()
  H.eq(table.concat(order, ","), "now,a,b")
end)

H.test("intervals repeat until cancelled", function()
  local n = 0
  local t
  t = ws.every(0.01, function()
    n = n + 1
    if n == 3 then t:cancel() end
  end)
  H.run()
  H.eq(n, 3)
end)

H.test("cancelled timers never fire", function()
  local fired = false
  ws.after(0.01, function() fired = true end):cancel()
  H.run()
  H.eq(fired, false)
end)

H.test("timer arguments are checked", function()
  H.raises(function() ws.after(-1, print) end, "non-negative")
  H.raises(function() ws.every(0, print) end, "positive")
  H.raises(function() ws.after(1) end, "callback must be a function")
end)

H.test("poll reports activity", function()
  H.eq(ws.poll(), false, "idle")
  local t = ws.after(10, print)
  H.eq(ws.poll(0.01), true, "pending timer")
  t:cancel()
  H.eq(ws.poll(), false, "idle again")
  H.raises(function() ws.poll(-1) end, "non-negative")
end)

H.test("a timer drives a heartbeat", function()
  local server = H.server()
  server:on("connection", function(c) c:on("close", function() server:close() end) end)
  local c = ws.connect(H.url(server))
  local pongs = 0
  local beat
  c:on("open", function()
    beat = ws.every(0.02, function() c:ping() end)
  end)
  c:on("pong", function()
    pongs = pongs + 1
    if pongs == 3 then
      beat:cancel()
      c:close()
    end
  end)
  H.run()
  H.eq(pongs, 3)
end)
