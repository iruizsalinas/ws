-- WebSocket client and server for Lua. See README.md for the API.
local loop = require("ws.loop")
local server = require("ws.server")
local socket = require("ws.socket")
local timer = require("ws.timer")

return {
  _VERSION = "0.2.0",
  connect = socket.connect,
  listen = server.listen,
  run = loop.run,
  poll = loop.poll,
  after = timer.after,
  every = timer.every,
}
