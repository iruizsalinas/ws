local T = require("helper")
T.init("test_frame.lua")

local frame = require("ws.frame")
local buffer = require("ws.buffer")
local compat = require("ws.compat")
local band = compat.band

-- unmasked text frame
local encoded = frame.encode("hi", {
  fin = true, opcode = frame.TEXT, mask = false, rsv1 = false
})
local b1, b2 = string.byte(encoded, 1, 2)
T.check_equal("text fin byte1", b1, 0x81)
T.check_equal("text len byte2", b2, 2)
T.check_equal("text payload", encoded:sub(3), "hi")

-- unmasked binary frame
local bin = frame.encode("\x00\x01\x02", {
  fin = true, opcode = frame.BINARY, mask = false, rsv1 = false
})
T.check_equal("binary byte1", string.byte(bin, 1), 0x82)
T.check_equal("binary len", string.byte(bin, 2), 3)

-- masked frame
local mask_key = "\x37\xfa\x21\x3d"
local masked = frame.encode("Hi", {
  fin = true, opcode = frame.TEXT, mask = true, mask_key = mask_key, rsv1 = false
})
local mb1, mb2 = string.byte(masked, 1, 2)
T.check_equal("masked byte1", mb1, 0x81)
T.check_equal("masked byte2 mask bit", band(mb2, 0x80), 0x80)
T.check_equal("masked byte2 len", band(mb2, 0x7F), 2)
T.check_equal("masked mask key", masked:sub(3, 6), mask_key)
local masked_payload = masked:sub(7)
T.check_equal("masked payload decodes", buffer.unmask(masked_payload, mask_key), "Hi")

-- 0-byte payload
local empty = frame.encode("", {
  fin = true, opcode = frame.TEXT, mask = false, rsv1 = false
})
T.check_equal("empty payload byte2", string.byte(empty, 2), 0)
T.check_equal("empty total len", #empty, 2)

-- 125-byte payload (max 7-bit)
local p125 = string.rep("a", 125)
local f125 = frame.encode(p125, {
  fin = true, opcode = frame.TEXT, mask = false, rsv1 = false
})
T.check_equal("125-byte len field", string.byte(f125, 2), 125)
T.check_equal("125-byte total", #f125, 2 + 125)

-- 126-byte payload (triggers 16-bit length)
local p126 = string.rep("b", 126)
local f126 = frame.encode(p126, {
  fin = true, opcode = frame.TEXT, mask = false, rsv1 = false
})
T.check_equal("126-byte len field", string.byte(f126, 2), 126)
local ext_len = buffer.read_uint16be(f126, 3)
T.check_equal("126-byte ext len", ext_len, 126)
T.check_equal("126-byte total", #f126, 2 + 2 + 126)

-- 65535-byte payload (max 16-bit)
local p65535 = string.rep("c", 65535)
local f65535 = frame.encode(p65535, {
  fin = true, opcode = frame.BINARY, mask = false, rsv1 = false
})
T.check_equal("65535 len field", string.byte(f65535, 2), 126)
T.check_equal("65535 ext len", buffer.read_uint16be(f65535, 3), 65535)

-- 65536-byte payload (triggers 64-bit length)
local p65536 = string.rep("d", 65536)
local f65536 = frame.encode(p65536, {
  fin = true, opcode = frame.BINARY, mask = false, rsv1 = false
})
T.check_equal("65536 len field", string.byte(f65536, 2), 127)
local high = buffer.read_uint32be(f65536, 3)
local low = buffer.read_uint32be(f65536, 7)
T.check_equal("65536 high", high, 0)
T.check_equal("65536 low", low, 65536)
T.check_equal("65536 total", #f65536, 2 + 8 + 65536)

-- ping frame
local ping = frame.encode("", {
  fin = true, opcode = frame.PING, mask = false, rsv1 = false
})
T.check_equal("ping byte1", string.byte(ping, 1), 0x89)

-- pong frame
local pong = frame.encode("", {
  fin = true, opcode = frame.PONG, mask = false, rsv1 = false
})
T.check_equal("pong byte1", string.byte(pong, 1), 0x8A)

-- close frame
local close = frame.encode("", {
  fin = true, opcode = frame.CLOSE, mask = false, rsv1 = false
})
T.check_equal("close byte1", string.byte(close, 1), 0x88)

-- continuation frame (FIN=0)
local cont = frame.encode("part1", {
  fin = false, opcode = frame.TEXT, mask = false, rsv1 = false
})
T.check_equal("continuation byte1", string.byte(cont, 1), 0x01)

-- RSV1 set
local rsv1 = frame.encode("data", {
  fin = true, opcode = frame.TEXT, mask = false, rsv1 = true
})
T.check_equal("rsv1 byte1", string.byte(rsv1, 1), 0xC1)

-- constants
T.check_equal("CONTINUATION", frame.CONTINUATION, 0x00)
T.check_equal("TEXT", frame.TEXT, 0x01)
T.check_equal("BINARY", frame.BINARY, 0x02)
T.check_equal("CLOSE", frame.CLOSE, 0x08)
T.check_equal("PING", frame.PING, 0x09)
T.check_equal("PONG", frame.PONG, 0x0A)
T.check_equal("GUID", frame.GUID, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11")

T.finish()
