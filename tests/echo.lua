local H = require("tests.helper")
local ws = require("ws")

H.test("text and binary echo", function()
  local server = H.echo()
  local c = ws.connect(H.url(server, "/chat?room=1"))
  H.eq(c.state, "connecting", "initial state")
  H.eq(c.url, H.url(server, "/chat?room=1"), "url")
  local got = {}
  c:on("open", function()
    H.eq(c.state, "open", "state")
    assert(c:send("h\195\169llo"))
    assert(c:send("\0\1\2\255", true))
  end)
  c:on("message", function(data, binary)
    got[#got + 1] = { data, binary }
    if #got == 2 then c:close() end
  end)
  H.run()
  H.eq(got[1][1], "h\195\169llo")
  H.eq(got[1][2], false, "text flag")
  H.eq(got[2][1], "\0\1\2\255")
  H.eq(got[2][2], true, "binary flag")
  H.eq(c.state, "closed", "final state")
end)

H.test("large message within a raised limit", function()
  local server = H.echo({ limits = { message = 4 * 1024 * 1024 } })
  local big = string.rep("0123456789", 200000)
  local c = ws.connect(H.url(server), { limits = { message = 4 * 1024 * 1024 } })
  local echoed
  c:on("open", function() c:send(big, true) end)
  c:on("message", function(data) echoed = data; c:close() end)
  H.run()
  H.eq(echoed == big, true, "2 MB roundtrip")
end)

H.test("a full send buffer refuses sends, then drain says when to resume", function()
  -- The server cannot read while the open handler keeps sending.
  local server = H.server({ limits = { message = 1024 * 1024 } })
  local chunk = string.rep("x", 256 * 1024)
  local c = ws.connect(H.url(server), { limits = { buffer = 1024 * 1024 } })
  local accepted, err, drains, resumed, last = 0, nil, 0, nil, nil
  server:on("connection", function(s)
    s:on("message", function(data) last = data end)
  end)
  c:on("open", function()
    for _ = 1, 400 do
      local ok, e = c:send(chunk, true)
      if not ok then
        err = e
        break
      end
      accepted = accepted + 1
    end
  end)
  c:on("drain", function()
    drains = drains + 1
    resumed = c:send("after")
    c:close()
  end)
  c:on("close", function() server:close() end)
  H.run()
  H.eq(err, "send buffer is full")
  assert(accepted >= 4, "sends below the limit are accepted")
  H.eq(drains, 1, "drain events")
  H.eq(resumed, true, "send after drain")
  H.eq(last, "after", "last message the server got")
end)

H.test("ping and pong", function()
  local server = H.server()
  local seen = {}
  server:on("connection", function(c)
    c:on("ping", function(data) seen[#seen + 1] = "server ping " .. data end)
    c:on("close", function() server:close() end)
  end)
  local c = ws.connect(H.url(server))
  c:on("open", function() assert(c:ping("hi")) end)
  c:on("pong", function(data) seen[#seen + 1] = "client pong " .. data; c:close() end)
  H.run()
  H.eq(table.concat(seen, ","), "server ping hi,client pong hi")
end)

H.test("subprotocol follows the client's preference", function()
  local server = H.server({ protocols = { "b", "a" } })
  local server_side
  server:on("connection", function(c)
    server_side = c.protocol
    c:close()
    server:close()
  end)
  local c = ws.connect(H.url(server), { protocols = { "a", "b" } })
  local client_side
  c:on("open", function() client_side = c.protocol end)
  H.run()
  H.eq(client_side, "a", "client protocol")
  H.eq(server_side, "a", "server protocol")
end)

H.test("no shared subprotocol leaves it unset", function()
  local server = H.server({ protocols = "x" })
  server:on("connection", function(c) c:close(); server:close() end)
  local c = ws.connect(H.url(server), { protocols = "y" })
  local opened
  c:on("open", function() opened = true; H.eq(c.protocol, nil, "protocol") end)
  H.run()
  H.eq(opened, true, "opened")
end)

H.test("request details", function()
  local seen, accepted
  local server = H.server({
    accept = function(req)
      accepted = req
      return true
    end,
  })
  server:on("connection", function(c, req)
    seen = req
    c:close()
    server:close()
  end)
  ws.connect(H.url(server, "/p?q=1"), { headers = { Origin = "https://app.example", ["X-Token"] = "abc" } })
  H.run()
  for _, req in ipairs({ accepted, seen }) do
    H.eq(req.path, "/p?q=1", "path")
    H.eq(req.address, "127.0.0.1", "address")
    H.eq(type(req.port), "number", "port")
    H.eq(req.headers.origin, "https://app.example", "origin")
    H.eq(req.headers["x-token"], "abc", "custom header")
    H.eq(req.headers.host, "127.0.0.1:" .. server.port, "host")
  end
end)
