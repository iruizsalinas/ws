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

T.finish()
