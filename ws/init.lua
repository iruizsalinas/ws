local WebSocket = require("ws.websocket")
local Server = require("ws.server")
local frame = require("ws.frame")

local M = {}

M.WebSocket = WebSocket
M.Server = Server

M.CONNECTING = WebSocket.CONNECTING
M.OPEN = WebSocket.OPEN
M.CLOSING = WebSocket.CLOSING
M.CLOSED = WebSocket.CLOSED

M.TEXT = frame.TEXT
M.BINARY = frame.BINARY

M._VERSION = "0.1.0"

function M.client(address, options)
  return WebSocket.new(address, options)
end

function M.server(options)
  return Server.new(options)
end

function M.connect(address, options)
  local ws = WebSocket.new(address, options)
  local ok, err = ws:connect()
  if not ok then
    return nil, err
  end
  return ws
end

return M
