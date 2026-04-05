local T = require("helper")
T.init("test_server.lua")

local Server = require("ws.server")
local WebSocket = require("ws.websocket")

local valid_key = "dGhlIHNhbXBsZSBub25jZQ=="

local function make_socket()
  return {
    sent = {},
    closed = false,
    timeout = nil,
    send = function(self, data)
      self.sent[#self.sent + 1] = data
      return true
    end,
    close = function(self)
      self.closed = true
    end,
    settimeout = function(self, value)
      self.timeout = value
    end,
  }
end

local function make_ws(sock)
  return {
    _socket = sock,
    ready_state = "OPEN",
    poll_calls = 0,
    close_calls = 0,
    on = function(self, event, fn)
      if event == "close" then
        self._on_close = fn
      end
    end,
    poll = function(self)
      self.poll_calls = self.poll_calls + 1
    end,
    close = function(self)
      self.close_calls = self.close_calls + 1
      if self._on_close then
        self._on_close()
      end
    end,
  }
end

-- missing Connection header is rejected
local server1 = Server.new({ no_server = true })
local sock1 = make_socket()
server1:_handle_upgrade(sock1, "GET", "/", {
  upgrade = "websocket",
  ["sec-websocket-key"] = valid_key,
  ["sec-websocket-version"] = "13",
})
T.check("missing connection status", sock1.sent[1] and sock1.sent[1]:find("400 Bad Request", 1, true) ~= nil)
T.check("missing connection body", sock1.sent[1] and sock1.sent[1]:find("Invalid Connection header", 1, true) ~= nil)
T.check("missing connection closed", sock1.closed)

-- client sockets are still polled when client_tracking is disabled
local server2 = Server.new({ no_server = true, client_tracking = false })
local sock2 = make_socket()
local ws2 = make_ws(sock2)
server2._socket_lib = {
  select = function(sockets)
    return { sock2 }
  end,
}
server2:_register_connection(ws2)
server2:poll(0)
T.check_equal("poll without client tracking", ws2.poll_calls, 1)
T.check("clients table omitted", server2.clients == nil)

server2:close()
T.check_equal("close without client tracking", ws2.close_calls, 1)

-- invalid subprotocol selected by callback fails upgrade
local original_create = WebSocket._create_from_server
WebSocket._create_from_server = function(socket)
  return {
    _socket = socket,
    protocol = "",
    on = function() end,
    _setup_socket = function() end,
  }
end

local server3 = Server.new({
  no_server = true,
  handle_protocols = function()
    return "bogus"
  end,
})
local sock3 = make_socket()
server3:_complete_upgrade(sock3, valid_key, { "chat" }, {}, "/", {})
T.check("invalid subprotocol status", sock3.sent[1] and sock3.sent[1]:find("500 Internal Server Error", 1, true) ~= nil)
T.check("invalid subprotocol closed", sock3.closed)

WebSocket._create_from_server = original_create

T.finish()
