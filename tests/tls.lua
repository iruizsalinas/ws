local H = require("tests.helper")
local ws = require("ws")

local dir = "tests/fixtures/tls/"

local function tls_server()
  return H.echo({ tls = { cert = dir .. "server.pem", key = dir .. "server-key.pem" } })
end

H.test("wss with a private CA", function()
  local server = tls_server()
  local c = ws.connect("wss://localhost:" .. server.port .. "/secure", { tls = { ca = dir .. "ca.pem" } })
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
  local c = ws.connect("wss://127.0.0.1:" .. server.port, { tls = { ca = dir .. "ca.pem" } })
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

local function read(name)
  local f = assert(io.open(dir .. name, "rb"))
  local text = f:read("*a")
  f:close()
  return text
end

H.test("certificates and keys can be given as PEM text", function()
  local server = H.echo({ tls = { cert = read("server.pem"), key = read("server-key.pem") } })
  local c = ws.connect("wss://localhost:" .. server.port, { tls = { ca = read("ca.pem") } })
  local echoed
  c:on("open", function() c:send("inline") end)
  c:on("message", function(data)
    echoed = data
    c:close()
  end)
  H.run()
  H.eq(echoed, "inline")
end)

H.test("errors about PEM text never echo it", function()
  -- Well framed, so the body decodes and only fails as a key.
  local label = read("server-key.pem"):match("^%-%-%-%-%-BEGIN ([^-]+)%-%-%-%-%-")
  local secret = "c2VjcmV0LWtleS1tYXRlcmlhbA"
  local key = "-----BEGIN " .. label .. "-----\n" .. secret .. "\n-----END " .. label .. "-----\n"
  local ok, err = pcall(ws.listen, { port = 0, tls = { cert = read("server.pem"), key = key } })
  H.eq(ok, false, "listen with a corrupt key")
  assert(not err:find("cannot read", 1, true), "PEM text was read as a path: " .. err)
  assert(not err:find(secret, 1, true) and not err:find("secret-key", 1, true), "error leaked key material: " .. err)
end)
