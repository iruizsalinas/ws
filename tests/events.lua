local H = require("tests.helper")
local ws = require("ws")

-- An echo server and a client that sends `count` messages once open.
local function chatter(count)
  local server = H.echo()
  local c = ws.connect(H.url(server))
  c:on("open", function()
    for i = 1, count do c:send("m" .. i) end
  end)
  return c
end

H.test("once listeners fire for the next event only", function()
  local c = chatter(3)
  local every, first = {}, {}
  c:once("message", function(data) first[#first + 1] = data end)
  c:on("message", function(data)
    every[#every + 1] = data
    if #every == 3 then c:close() end
  end)
  H.run()
  H.eq(#every, 3, "messages")
  H.eq(#first, 1, "once calls")
  H.eq(first[1], "m1")
end)

H.test("off removes the most recent matching listener", function()
  local c = chatter(2)
  local calls, gone, dropped = 0, 0, 0
  local function count() calls = calls + 1 end
  local function never() gone = gone + 1 end
  local function skipped() dropped = dropped + 1 end
  c:on("message", count):on("message", count):on("message", never):once("message", skipped)
  c:off("message", count):off("message", never):off("message", skipped)
  c:off("message", never):off("close", count)
  local seen = 0
  c:on("message", function()
    seen = seen + 1
    if seen == 2 then c:close() end
  end)
  H.run()
  H.eq(calls, 2, "one count listener left")
  H.eq(gone, 0, "removed listener")
  H.eq(dropped, 0, "once listener removed by its function")
end)

H.test("off keeps the order of the listeners left", function()
  local c = chatter(1)
  local log = {}
  local function a() log[#log + 1] = "a" end
  local function b() log[#log + 1] = "b" end
  c:on("message", a):on("message", b):on("message", a):off("message", a)
  c:on("message", function() c:close() end)
  H.run()
  H.eq(table.concat(log, ","), "a,b")
end)

H.test("server clients lists open connections and allows broadcast", function()
  local server = H.server()
  local accepted, received = {}, {}
  server:on("connection", function(peer)
    accepted[#accepted + 1] = peer
    local list = server:clients()
    H.eq(#list, #accepted, "listed before the connection event")
    for i, s in ipairs(list) do
      H.eq(s, accepted[i], "client " .. i)
    end
    if #accepted < 3 then return end
    accepted[1]:on("close", function()
      local left = server:clients()
      H.eq(#left, 2, "clients after one closed")
      H.eq(left[1], accepted[2])
      H.eq(left[2], accepted[3])
      server:close()
    end)
    for _, s in ipairs(list) do s:send("hi") end
  end)

  local clients = {}
  local function open(i)
    local c = ws.connect(H.url(server))
    clients[i] = c
    c:on("open", function()
      if i < 3 then open(i + 1) end
    end)
    c:on("message", function(data)
      received[#received + 1] = data
      if #received == 3 then clients[1]:close() end
    end)
  end
  open(1)
  H.run()
  H.eq(#accepted, 3, "connections")
  H.eq(#received, 3, "broadcast messages")
  H.eq(#server:clients(), 0, "clients after close")
  local fresh = H.server()
  H.eq(#fresh:clients(), 0, "fresh server")
  fresh:close()
end)
