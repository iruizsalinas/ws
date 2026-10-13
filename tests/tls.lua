local H = require("tests.helper")
local ws = require("ws")

local dir = "tests/fixtures/tls/"

local function tls_server()
  return H.echo({ cert = dir .. "server.pem", key = dir .. "server-key.pem" })
end

H.test("wss with a private CA", function()
  local server = tls_server()
  local c = ws.connect("wss://localhost:" .. server.port .. "/secure", { ca = dir .. "ca.pem" })
  local echoed
  c:on("open", function() c:send("secret") end)
  c:on("message", function(data)
    echoed = data
    c:close()
  end)
  H.run()
  H.eq(echoed, "secret")
end)

H.test("untrusted certificates are refused", function()
  local server = tls_server()
  local c = ws.connect("wss://localhost:" .. server.port)
  local err
  c:on("error", function(e) err = e end)
  c:on("close", function() server:close() end)
  H.run()
  assert(err and err:find("certificate", 1, true), "error mentions the certificate: " .. tostring(err))
end)

H.test("certificates must match the host name", function()
  local server = tls_server()
  local c = ws.connect("wss://127.0.0.1:" .. server.port, { ca = dir .. "ca.pem" })
  local err
  c:on("error", function(e) err = e end)
  c:on("close", function() server:close() end)
  H.run()
  assert(err and err:find("certificate", 1, true), "error mentions the certificate: " .. tostring(err))
end)

H.test("plain clients cannot speak to a TLS server", function()
  local server = tls_server()
  local c = ws.connect("ws://127.0.0.1:" .. server.port)
  local code
  c:on("error", function() end)
  c:on("close", function(c2)
    code = c2
    server:close()
  end)
  H.run()
  H.eq(code, 1006)
end)
