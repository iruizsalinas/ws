local H = require("tests.helper")
local ws = require("ws")

-- Connects and collects the client's error and close code.
local function attempt(server, options)
  local c = ws.connect(H.url(server), options)
  local result = {}
  c:on("error", function(err) result.error = err end)
  c:on("close", function(code)
    result.code = code
    server:close()
  end)
  return result
end

H.test("accept can reject with a status", function()
  local server = H.server({ accept = function() return false, 401 end })
  local r = attempt(server)
  H.run()
  H.eq(r.error, "unexpected server response: 401")
  H.eq(r.code, 1006)
end)

H.test("accept that returns nothing rejects with 403", function()
  local server = H.server({ accept = function() end })
  local r = attempt(server)
  H.run()
  H.eq(r.error, "unexpected server response: 403")
end)

H.test("accept errors reach ws.run and the client gets 500", function()
  local server = H.server({ accept = function() error("boom") end })
  local r = attempt(server)
  H.raises(function() H.run() end, "boom")
  H.run()
  H.eq(r.error, "unexpected server response: 500")
end)

H.test("accept with an invalid status", function()
  local server = H.server({ accept = function() return false, 200 end })
  local r = attempt(server)
  H.raises(function() H.run() end, "status must be an integer from 400 to 599")
  H.run()
  H.eq(r.error, "unexpected server response: 500")
end)

H.test("listen accepts a bare port", function()
  local server = ws.listen(0)
  H.eq(type(server.port), "number", "port")
  assert(server.port > 0, "bound port")
  H.eq(tostring(server), "ws.server (port " .. server.port .. ")")
  server:close()
  server:close()
  H.run()
end)

H.test("binding a busy port raises", function()
  local first = H.server()
  local ok, err = pcall(ws.listen, { port = first.port, host = "127.0.0.1" })
  H.eq(ok, false, "listen succeeded")
  H.eq(type(err), "string", "error")
  first:close()
end)

H.test("max_payload closes with 1009", function()
  local server = H.server({ max_payload = 1000 })
  local server_error
  server:on("connection", function(c)
    c:on("error", function(err) server_error = err end)
  end)
  local c = ws.connect(H.url(server))
  local code
  c:on("open", function() c:send(string.rep("x", 1001)) end)
  c:on("close", function(c2) code = c2; server:close() end)
  H.run()
  H.eq(server_error, "message too big")
  H.eq(code, 1009)
end)

H.test("server sockets have no url", function()
  local server = H.server()
  local url
  server:on("connection", function(c)
    url = c.url
    c:close()
    server:close()
  end)
  ws.connect(H.url(server))
  H.run()
  H.eq(url, nil)
end)
