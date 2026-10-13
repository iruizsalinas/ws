-- Keepalive between two of our own endpoints. Dropping silent peers needs a
-- peer that never answers, which only the Rust tests can build.
local H = require("tests.helper")
local ws = require("ws")

H.test("keepalive pings a quiet peer and hides the answers", function()
  local server = H.server({ keepalive = false })
  local pings, pongs, state = 0, 0, nil
  server:on("connection", function(s)
    s:on("ping", function() pings = pings + 1 end)
  end)
  local c = ws.connect(H.url(server), { keepalive = 0.1 })
  c:on("pong", function() pongs = pongs + 1 end)
  c:on("open", function()
    ws.after(0.8, function()
      state = c.state
      c:close()
    end)
  end)
  c:on("close", function() server:close() end)
  H.run()
  assert(pings >= 3, "keepalive pings seen by the server: " .. pings)
  H.eq(pongs, 0, "keepalive pongs reported to the client")
  H.eq(state, "open", "state after several keepalive rounds")
end)
