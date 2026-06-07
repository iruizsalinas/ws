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
  host = "example.com",
  upgrade = "websocket",
  ["sec-websocket-key"] = valid_key,
  ["sec-websocket-version"] = "13",
})
T.check("missing connection status", sock1.sent[1] and sock1.sent[1]:find("400 Bad Request", 1, true) ~= nil)
T.check("missing connection empty body", sock1.sent[1] and sock1.sent[1]:find("Content-Length: 0", 1, true) ~= nil)
T.check("missing connection no debug body", sock1.sent[1] and sock1.sent[1]:find("Invalid Connection header", 1, true) == nil)
T.check("missing connection closed", sock1.closed)

-- non-canonical Sec-WebSocket-Key padding is rejected
local server1b = Server.new({ no_server = true })
local sock1b = make_socket()
server1b:_handle_upgrade(sock1b, "GET", "/", {
  host = "example.com",
  upgrade = "websocket",
  connection = "Upgrade",
  ["sec-websocket-key"] = "AAAAAAAAAAAAAAAAAAAA====",
  ["sec-websocket-version"] = "13",
})
T.check("bad key padding status", sock1b.sent[1] and sock1b.sent[1]:find("400 Bad Request", 1, true) ~= nil)
T.check("bad key padding closed", sock1b.closed)

-- public handle_upgrade enforces GET like the built-in parser
local server1c = Server.new({ no_server = true })
local sock1c = make_socket()
server1c:handle_upgrade(sock1c, "POST", "/", {
  Host = "example.com",
  upgrade = "websocket",
  connection = "Upgrade",
  ["sec-websocket-key"] = valid_key,
  ["sec-websocket-version"] = "13",
})
T.check("public non-GET status", sock1c.sent[1] and sock1c.sent[1]:find("405 Method Not Allowed", 1, true) ~= nil)
T.check("public non-GET closed", sock1c.closed)

-- Host is required by the opening handshake
local server1d = Server.new({ no_server = true })
local sock1d = make_socket()
server1d:handle_upgrade(sock1d, "GET", "/", {
  upgrade = "websocket",
  connection = "Upgrade",
  ["sec-websocket-key"] = valid_key,
  ["sec-websocket-version"] = "13",
})
T.check("missing host status", sock1d.sent[1] and sock1d.sent[1]:find("400 Bad Request", 1, true) ~= nil)

local server1d1 = Server.new({ no_server = true })
local sock1d1 = make_socket()
server1d1:handle_upgrade(sock1d1, "GET", "/", {
  Host = "example.com:8080",
  Upgrade = "websocket",
  Connection = "Upgrade",
  ["Sec-WebSocket-Key"] = valid_key,
  ["Sec-WebSocket-Version"] = "13",
})
T.check("host with port accepted", sock1d1.sent[1] and sock1d1.sent[1]:find("101 Switching Protocols", 1, true) ~= nil)

local server1d2 = Server.new({ no_server = true })
local sock1d2 = make_socket()
server1d2:handle_upgrade(sock1d2, "GET", "/", {
  Host = "example.com, other.example",
  Upgrade = "websocket",
  Connection = "Upgrade",
  ["Sec-WebSocket-Key"] = valid_key,
  ["Sec-WebSocket-Version"] = "13",
})
T.check("duplicate host rejected", sock1d2.sent[1] and sock1d2.sent[1]:find("400 Bad Request", 1, true) ~= nil)

-- RFC 6455 requires version 13; older draft versions are rejected
local server1e = Server.new({ no_server = true })
local sock1e = make_socket()
server1e:handle_upgrade(sock1e, "GET", "/", {
  Host = "example.com",
  Upgrade = "websocket",
  Connection = "Upgrade",
  ["Sec-WebSocket-Key"] = valid_key,
  ["Sec-WebSocket-Version"] = "8",
})
T.check("draft version status", sock1e.sent[1] and sock1e.sent[1]:find("426 Upgrade Required", 1, true) ~= nil)
T.check("draft version advertises 13", sock1e.sent[1] and sock1e.sent[1]:find("Sec-WebSocket-Version: 13", 1, true) ~= nil)

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

local function make_recv_socket(lines)
  return {
    sent = {},
    closed = false,
    timeout = nil,
    receive_calls = 0,
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
    receive = function(self)
      self.receive_calls = self.receive_calls + 1
      local item = table.remove(lines, 1)
      if item == nil then return nil, "timeout", "" end
      if type(item) == "table" then return nil, item[1], item[2] end
      return item
    end,
  }
end

local function make_chunk_socket(chunks)
  local buffer = table.concat(chunks)
  local offset = 1
  return {
    sent = {},
    closed = false,
    timeout = nil,
    receive_calls = 0,
    receive_sizes = {},
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
    receive = function(self, size)
      self.receive_calls = self.receive_calls + 1
      self.receive_sizes[#self.receive_sizes + 1] = size
      if offset > #buffer then return nil, "timeout", "" end
      local chunk = buffer:sub(offset, offset + size - 1)
      offset = offset + #chunk
      return chunk
    end,
  }
end

local slow_sock = make_recv_socket({})
local server4 = Server.new({ no_server = true })
server4._server = {
  accept = function() return slow_sock end
}
server4:_accept_connection()
T.check_equal("accepted handshake is nonblocking", slow_sock.timeout, 0)
T.check("slow socket remains pending", server4._handshakes[slow_sock] ~= nil)
T.check("slow socket not closed immediately", not slow_sock.closed)

local big_sock = make_chunk_socket({ string.rep("x", 9) })
local server5 = Server.new({ no_server = true, max_header_size = 8 })
server5._handshakes[big_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server5:_read_handshake(big_sock)
T.check("oversized chunk header closed", big_sock.closed)
T.check("oversized chunk removed", server5._handshakes[big_sock] == nil)
T.check("oversized chunk status", big_sock.sent[1] and big_sock.sent[1]:find("431 Request Header Fields Too Large", 1, true) ~= nil)
T.check_equal("oversized chunk bounded read", big_sock.receive_sizes[1], 8)

local http10_sock = make_chunk_socket({
  "GET /chat HTTP/1.0\r\nHost: example.com\r\nUpgrade: websocket\r\n",
  "Connection: Upgrade\r\nSec-WebSocket-Key: " .. valid_key .. "\r\n",
  "Sec-WebSocket-Version: 13\r\n\r\n",
})
local server5b = Server.new({ no_server = true })
server5b._handshakes[http10_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server5b:_read_handshake(http10_sock)
T.check("HTTP/1.0 rejected", http10_sock.sent[1] and http10_sock.sent[1]:find("400 Bad Request", 1, true) ~= nil)

local original_create2 = WebSocket._create_from_server
local folded_connection
WebSocket._create_from_server = function(socket)
  return {
    _socket = socket,
    protocol = "",
    on = function() end,
    _setup_socket = function() end,
  }
end
local folded_sock = make_chunk_socket({
  "GET http://example.com/chat?x=1 HTTP/1.1\r\nHost: example.com\r\n",
  "Upgrade: websocket\r\nConnection: keep-alive\r\nConnection:\r\n",
  " Upgrade\r\nSec-WebSocket-Key: " .. valid_key .. "\r\n",
  "Sec-WebSocket-Version: 13\r\n\r\n",
})
local server5c = Server.new({ no_server = true, path = "/chat" })
server5c:on("connection", function(_, req)
  folded_connection = req
end)
server5c._handshakes[folded_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server5c:_read_handshake(folded_sock)
T.check("absolute target upgraded", folded_sock.sent[1] and folded_sock.sent[1]:find("101 Switching Protocols", 1, true) ~= nil)
T.check_equal("absolute target normalized", folded_connection and folded_connection.path, "/chat?x=1")

WebSocket._create_from_server = original_create2

local mismatch_sock = make_chunk_socket({
  "GET http://other.example/chat HTTP/1.1\r\nHost: example.com\r\n",
  "Upgrade: websocket\r\nConnection: Upgrade\r\n",
  "Sec-WebSocket-Key: " .. valid_key .. "\r\nSec-WebSocket-Version: 13\r\n\r\n",
})
local server5d = Server.new({ no_server = true })
server5d._handshakes[mismatch_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server5d:_read_handshake(mismatch_sock)
T.check("absolute target host mismatch rejected", mismatch_sock.sent[1] and mismatch_sock.sent[1]:find("400 Bad Request", 1, true) ~= nil)

local ipv6_host_sock = make_chunk_socket({
  "GET /chat HTTP/1.1\r\nHost: [::1]:8080\r\n",
  "Upgrade: websocket\r\nConnection: Upgrade\r\n",
  "Sec-WebSocket-Key: " .. valid_key .. "\r\nSec-WebSocket-Version: 13\r\n\r\n",
})
local server5e = Server.new({ no_server = true })
server5e._handshakes[ipv6_host_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server5e:_read_handshake(ipv6_host_sock)
T.check("IPv6 host with port accepted", ipv6_host_sock.sent[1] and ipv6_host_sock.sent[1]:find("101 Switching Protocols", 1, true) ~= nil)

local folded_host_sock = make_chunk_socket({
  "GET /chat HTTP/1.1\r\nHost: example.com\r\n",
  " Upgrade\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n",
  "Sec-WebSocket-Key: " .. valid_key .. "\r\nSec-WebSocket-Version: 13\r\n\r\n",
})
local server5f = Server.new({ no_server = true })
server5f._handshakes[folded_host_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server5f:_read_handshake(folded_host_sock)
T.check("folded host rejected", folded_host_sock.sent[1] and folded_host_sock.sent[1]:find("400 Bad Request", 1, true) ~= nil)

WebSocket._create_from_server = function(socket)
  return {
    _socket = socket,
    protocol = "",
    on = function() end,
    _setup_socket = function() end,
  }
end

local complete_sock = make_chunk_socket({
  "GET /chat HTTP/1.1\r\nHost: example.com\r\n",
  "Upgrade: websocket\r\nConnection: Upgrade\r\n",
  "Sec-WebSocket-Key: " .. valid_key .. "\r\nSec-WebSocket-Version: 13\r\n\r\n",
})
local server6 = Server.new({ no_server = true })
server6._handshakes[complete_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server6:_read_handshake(complete_sock)
T.check("complete handshake upgraded", complete_sock.sent[1] and complete_sock.sent[1]:find("101 Switching Protocols", 1, true) ~= nil)
T.check("complete handshake removed", server6._handshakes[complete_sock] == nil)

WebSocket._create_from_server = original_create2

local original_create3 = WebSocket._create_from_server
local pipelined_leftover
local pipelined_after_connection = false
WebSocket._create_from_server = function(socket)
  return {
    _socket = socket,
    protocol = "",
    on = function() end,
    _setup_socket = function(self)
      self._receiver = {
        write = function(_, data)
          pipelined_leftover = data
          pipelined_after_connection = self.connection_handler_ran == true
        end,
      }
    end,
  }
end

local pipelined_sock = make_chunk_socket({
  "GET /chat HTTP/1.1\r\nHost: example.com\r\n",
  "Upgrade: websocket\r\nConnection: Upgrade\r\n",
  "Sec-WebSocket-Key: " .. valid_key .. "\r\nSec-WebSocket-Version: 13\r\n\r\n",
  "FIRST-FRAME-BYTES",
})
local server7 = Server.new({ no_server = true })
server7:on("connection", function(client)
  client.connection_handler_ran = true
end)
server7._handshakes[pipelined_sock] = {
  buffer = "",
  headers = {},
  header_count = 0,
  size = 0,
  deadline = os.time() + 5,
}
server7:_read_handshake(pipelined_sock)
T.check_equal("pipelined bytes preserved", pipelined_leftover, "FIRST-FRAME-BYTES")
T.check("pipelined bytes after connection event", pipelined_after_connection)

WebSocket._create_from_server = original_create3

local original_create4 = WebSocket._create_from_server
local public_leftover
local public_after_connection = false
WebSocket._create_from_server = function(socket)
  return {
    _socket = socket,
    protocol = "",
    on = function() end,
    _setup_socket = function(self)
      self._receiver = {
        write = function(_, data)
          public_leftover = data
          public_after_connection = self.connection_handler_ran == true
        end,
      }
    end,
  }
end

local public_sock = make_socket()
local server8 = Server.new({ no_server = true })
server8:on("connection", function(client)
  client.connection_handler_ran = true
end)
server8:handle_upgrade(public_sock, "GET", "/", {
  Host = "example.com",
  upgrade = "websocket",
  connection = "Upgrade",
  ["sec-websocket-key"] = valid_key,
  ["sec-websocket-version"] = "13",
}, "PUBLIC-FIRST-FRAME")
T.check_equal("public upgrade preserves leftover", public_leftover, "PUBLIC-FIRST-FRAME")
T.check("public upgrade leftover after connection event", public_after_connection)

WebSocket._create_from_server = original_create4

T.finish()
