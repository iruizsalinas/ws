local H = require("tests.helper")
local ws = require("ws")
local loop = require("ws.loop")
local core = require("ws.core")

-- Lua 5.1, 5.2 and LuaJIT run the GC before copying a pushed string, so a
-- finalizer can call ws while ws.core is copying an event. Its strings must
-- survive that: this used to read freed memory and crash.
H.test("finalizers calling ws while an event is copied", function()
  local big = string.rep("x", 1048576)
  local server = H.server({ protocols = { "chat" } })
  local peer
  server:on("connection", function(c) peer = c end)
  local client = ws.connect(H.url(server), { protocols = "chat" })
  local deadline = core.now() + 10
  while not (peer and client.state == "open") do
    assert(core.now() < deadline, "no connection")
    ws.poll(0.05)
  end
  for _ = 1, 30 do
    peer:send(big)
    local t = core.now()
    while core.now() - t < 0.05 do end
    local ran = 0
    -- Each replaces ws's previous result strings.
    local function fin()
      ran = ran + 1
      local _ = client.protocol
    end
    for _ = 1, 20000 do
      if newproxy then
        getmetatable(newproxy(true)).__gc = fin
      else
        setmetatable({}, { __gc = fin })
      end
    end
    -- Mid-finalization, with debt the next GC check pays: the one in wait.
    while ran == 0 do collectgarbage("step", 0) end
    local junk = big:sub(2, 65536)
    local kind, id, data, code, from = core.wait(loop.hub, 1)
    while not (loop.kinds[kind] == "message" and id == client.id) do
      assert(kind, "no message")
      loop.handlers[loop.kinds[kind]](id, data, code, from)
      kind, id, data, code, from = core.wait(loop.hub, 1)
    end
    H.eq(data == big, true, "message intact")
    H.eq(#junk, 65535)
    collectgarbage("collect")
  end
  client:close()
  server:close()
  H.run()
end)
