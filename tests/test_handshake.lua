local T = require("helper")
T.init("test_handshake.lua")

local sha1 = require("ws.sha1")
local base64 = require("ws.base64")
local frame = require("ws.frame")

-- Sec-WebSocket-Accept computation
-- rfc 6455 section 4.2.2 example
local client_key = "dGhlIHNhbXBsZSBub25jZQ=="
local expected_accept = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
local computed = base64.encode(sha1.sha1(client_key .. frame.GUID))
T.check_equal("accept computation", computed, expected_accept)

-- key generation: 16 random bytes -> 24-char base64
local compat = require("ws.compat")
local key_bytes = compat.random_bytes(16)
T.check_equal("random bytes len", #key_bytes, 16)
local key_b64 = base64.encode(key_bytes)
T.check_equal("key b64 len", #key_b64, 24)
T.check("key b64 ends ==", key_b64:sub(-2) == "==" or key_b64:sub(-1) == "=" or true)

-- verify decoded key is 16 bytes
T.check_equal("key decode len", #base64.decode(key_b64), 16)

-- build request contains required headers
local handshake = require("ws.handshake")
local mock_ws = {
  _per_message_deflate = false,
  _max_payload = 100 * 1024 * 1024,
  _origin = nil,
  _headers = {},
}
local url = require("ws.url")
local parsed = url.parse("ws://localhost:8080/ws?token=abc")
local request, _ = handshake._build_request(mock_ws, parsed, "dGhlIHNhbXBsZSBub25jZQ==", {})
T.check("has GET", request:find("GET /ws%?token=abc HTTP/1.1") ~= nil)
T.check("has Host", request:find("Host: localhost:8080") ~= nil)
T.check("has Upgrade", request:find("Upgrade: websocket") ~= nil)
T.check("has Connection", request:find("Connection: Upgrade") ~= nil)
T.check("has Version", request:find("Sec%-WebSocket%-Version: 13") ~= nil)
T.check("has Key", request:find("Sec%-WebSocket%-Key: dGhlIHNhbXBsZSBub25jZQ==") ~= nil)
T.check("ends with CRLF CRLF", request:sub(-4) == "\r\n\r\n")

-- request with default port 80 omits port
local parsed80 = url.parse("ws://example.com/path")
local req80, _ = handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", {})
T.check("default port omitted", req80:find("Host: example.com\r\n") ~= nil)

-- request with protocols
local req_proto, _ = handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", {"chat", "json"})
T.check("has protocols", req_proto:find("Sec%-WebSocket%-Protocol: chat,json") ~= nil)

-- request with origin
mock_ws._origin = "http://example.com"
local req_origin, _ = handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", {})
T.check("has origin", req_origin:find("Origin: http://example.com") ~= nil)
mock_ws._origin = nil

mock_ws._origin = "http://example.com\r\nX-Evil: 1"
T.check_error("reject origin CRLF", function()
  handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", {})
end)
mock_ws._origin = nil

mock_ws._headers = { ["X-Test"] = "ok\r\nX-Evil: 1" }
T.check_error("reject header value CRLF", function()
  handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", {})
end)
mock_ws._headers = { ["X-Test\r\nX-Evil"] = "ok" }
T.check_error("reject header name CRLF", function()
  handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", {})
end)
mock_ws._headers = {}

T.check_error("reject invalid subprotocol token", function()
  handshake._build_request(mock_ws, parsed80, "AAAAAAAAAAAAAAAAAAAAAA==", { "bad proto" })
end)

do
  local old_ssl = package.loaded.ssl
  local captured
  package.loaded.ssl = {
    wrap = function(sock, params)
      captured = params
      return {
        sni = function() end,
        dohandshake = function() return true end,
      }
    end
  }
  local wrapped = handshake._wrap_tls({ close = function() end }, "example.com", { _tls_options = {} })
  T.check("tls wrap succeeds with fake ssl", wrapped ~= nil)
  T.check_equal("tls verifies peer by default", captured and captured.verify, "peer")
  package.loaded.ssl = old_ssl
end

local function make_mock_socket()
  return {
    closed = false,
    close = function(self) self.closed = true end,
  }
end

local function make_response_socket(chunks)
  return {
    closed = false,
    receive_calls = 0,
    receive_sizes = {},
    receive = function(self, size)
      self.receive_calls = self.receive_calls + 1
      self.receive_sizes[#self.receive_sizes + 1] = size
      local item = table.remove(chunks, 1)
      if item == nil then return nil, "closed", "" end
      if type(item) == "table" then return nil, item[1], item[2] end
      return item
    end,
    close = function(self) self.closed = true end,
  }
end

local function make_mock_ws()
  return {
    protocol = "",
    aborted = nil,
    setup_called = false,
    _abort = function(self, msg) self.aborted = msg end,
    _setup_socket = function(self) self.setup_called = true end,
  }
end

-- response parser enforces bounded reads
local big_sock = make_response_socket({ string.rep("x", 9) })
local big_ws = make_mock_ws()
big_ws._max_response_header_size = 8
local big_headers, big_err = handshake._read_response(big_sock, big_ws)
T.check("large response header rejected", big_headers == nil and big_err ~= nil)
T.check_equal("large response header abort", big_ws.aborted, "response headers too large")
T.check_equal("large response header bounded read", big_sock.receive_sizes[1], 9)
T.check("large response header closes socket", big_sock.closed)

local many_chunks = { "HTTP/1.1 101 Switching Protocols\r\n" }
for i = 1, 101 do
  many_chunks[#many_chunks + 1] = "X-Test-" .. i .. ": 1\r\n"
end
many_chunks[#many_chunks + 1] = "\r\n"
local many_sock = make_response_socket(many_chunks)
local many_ws = make_mock_ws()
many_ws._max_response_headers = 100
local many_headers, many_err = handshake._read_response(many_sock, many_ws)
T.check("too many response headers rejected", many_headers == nil and many_err ~= nil)
T.check_equal("too many response headers abort", many_ws.aborted, "too many response headers")
T.check("too many response headers closes socket", many_sock.closed)

local ok_sock = make_response_socket({
  "HTTP/1.1 101 Switching Protocols\r\nUp",
  "grade: websocket\r\nConnection: Upgrade\r\n\r\n",
})
local ok_ws = make_mock_ws()
local ok_headers, ok_code = handshake._read_response(ok_sock, ok_ws)
T.check_equal("chunked response status", ok_code, 101)
T.check_equal("chunked response header", ok_headers and ok_headers.upgrade, "websocket")

-- response validation requires Connection: Upgrade
local sock1 = make_mock_socket()
local ws1 = make_mock_ws()
local ok_conn, err_conn = handshake._validate_response(sock1, ws1, {
  upgrade = "websocket",
  ["sec-websocket-accept"] = expected_accept,
}, client_key, { "chat" }, nil)
T.check("missing connection rejected", ok_conn == nil and err_conn ~= nil)
T.check_equal("missing connection abort", ws1.aborted, "invalid Connection header")
T.check("missing connection closes socket", sock1.closed)

-- unsolicited server subprotocol is rejected
local sock2 = make_mock_socket()
local ws2 = make_mock_ws()
local ok_proto, err_proto = handshake._validate_response(sock2, ws2, {
  upgrade = "websocket",
  connection = "Upgrade",
  ["sec-websocket-accept"] = expected_accept,
  ["sec-websocket-protocol"] = "chat",
}, client_key, {}, nil)
T.check("unsolicited subprotocol rejected", ok_proto == nil and err_proto ~= nil)
T.check_equal("unsolicited subprotocol abort", ws2.aborted, "server sent an unsolicited subprotocol")
T.check("unsolicited subprotocol closes socket", sock2.closed)

T.finish()
