-- Autobahn echo server: `lua .github/autobahn/server.lua <port>`.
package.path = "./?.lua;./?/init.lua;" .. package.path
package.cpath = "./?.so;./?.dll;" .. package.cpath
local ws = require("ws")

-- Autobahn sends messages of up to 16 MiB.
local server = ws.listen({ port = tonumber(arg[1]), limit = 64 * 1024 * 1024 })
server:on("connection", function(c)
  c:on("message", function(data, binary) c:send(data, binary) end)
end)
print("listening on " .. server.port)
io.stdout:flush()
ws.run()
