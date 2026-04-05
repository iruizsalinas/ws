local T = require("helper")
T.init("test_receiver.lua")

local Receiver = require("ws.receiver")
local buffer = require("ws.buffer")
local frame = require("ws.frame")

local function make_frame(opcode, data, fin, mask_key)
  return frame.encode(data or "", {
    fin = fin ~= false,
    opcode = opcode,
    mask = mask_key ~= nil,
    mask_key = mask_key,
    rsv1 = false,
  })
end

-- simple text message
local rx = Receiver.new({ is_server = false })
local msg, is_bin
rx:on("message", function(d, b) msg = d; is_bin = b end)
rx:write(make_frame(0x01, "hello"))
T.check_equal("text message", msg, "hello")
T.check_equal("text is_binary", is_bin, false)

-- binary message
local rx2 = Receiver.new({ is_server = false })
local bmsg, bbin
rx2:on("message", function(d, b) bmsg = d; bbin = b end)
rx2:write(make_frame(0x02, "\x00\x01\x02\x03"))
T.check_equal("binary message", bmsg, "\x00\x01\x02\x03")
T.check_equal("binary is_binary", bbin, true)

-- close frame with code
local rx3 = Receiver.new({ is_server = false })
local close_code, close_reason
rx3:on("conclude", function(c, r) close_code = c; close_reason = r end)
rx3:write(make_frame(0x08, buffer.write_uint16be(1000) .. "bye"))
T.check_equal("close code", close_code, 1000)
T.check_equal("close reason", close_reason, "bye")

-- close frame empty (no status)
local rx4 = Receiver.new({ is_server = false })
local c4
rx4:on("conclude", function(c) c4 = c end)
rx4:write(make_frame(0x08, ""))
T.check_equal("close no status", c4, 1005)

-- close frame with just code (no reason)
local rx5 = Receiver.new({ is_server = false })
local c5, r5
rx5:on("conclude", function(c, r) c5 = c; r5 = r end)
rx5:write(make_frame(0x08, buffer.write_uint16be(1001)))
T.check_equal("close code only", c5, 1001)
T.check_equal("close empty reason", r5, "")

-- close frame max reason (123 bytes of valid UTF-8)
local rx5b = Receiver.new({ is_server = false })
local c5b, r5b
rx5b:on("conclude", function(c, r) c5b = c; r5b = r end)
local max_reason = string.rep("x", 123)
rx5b:write(make_frame(0x08, buffer.write_uint16be(1000) .. max_reason))
T.check_equal("close max reason code", c5b, 1000)
T.check_equal("close max reason len", #r5b, 123)

-- ping
local rx6 = Receiver.new({ is_server = false })
local ping_data
rx6:on("ping", function(d) ping_data = d end)
rx6:write(make_frame(0x09, "ping!"))
T.check_equal("ping data", ping_data, "ping!")

-- pong
local rx7 = Receiver.new({ is_server = false })
local pong_data
rx7:on("pong", function(d) pong_data = d end)
rx7:write(make_frame(0x0A, "pong!"))
T.check_equal("pong data", pong_data, "pong!")

-- masked frame in server mode
local rx8 = Receiver.new({ is_server = true })
local m8
rx8:on("message", function(d) m8 = d end)
rx8:write(make_frame(0x01, "secret", true, "\x00\x00\x00\x00"))
T.check_equal("masked server", m8, "secret")

-- reject unmasked in server mode
local rx9 = Receiver.new({ is_server = true })
local err9
rx9:on("error", function(m) err9 = m end)
rx9:write(make_frame(0x01, "bad"))
T.check("reject unmasked server", err9 and err9:find("MASK"))

-- reject masked in client mode
local rx10 = Receiver.new({ is_server = false })
local err10
rx10:on("error", function(m) err10 = m end)
rx10:write(make_frame(0x01, "bad", true, "\xAA\xBB\xCC\xDD"))
T.check("reject masked client", err10 and err10:find("MASK"))

-- RSV2 rejection
local rx11 = Receiver.new({ is_server = false })
local err11
rx11:on("error", function(m) err11 = m end)
rx11:write(string.char(0xB1, 0x02) .. "hi")
T.check("reject RSV2", err11 and err11:find("RSV"))

-- RSV1 without deflate extension
local rx12 = Receiver.new({ is_server = false })
local err12
rx12:on("error", function(m) err12 = m end)
rx12:write(string.char(0xC1, 0x02) .. "hi")
T.check("reject RSV1 no ext", err12 and err12:find("RSV1"))

-- invalid opcode
local rx13 = Receiver.new({ is_server = false })
local err13
rx13:on("error", function(m) err13 = m end)
rx13:write(string.char(0x83, 0x00))
T.check("reject invalid opcode", err13 and err13:find("opcode"))

-- control frame > 125 bytes
local rx14 = Receiver.new({ is_server = false })
local err14
rx14:on("error", function(m) err14 = m end)
rx14:write(string.char(0x89, 126) .. buffer.write_uint16be(126) .. string.rep("x", 126))
T.check("reject control > 125", err14 and err14:find("payload length"))

-- fragmented control frame
local rx15 = Receiver.new({ is_server = false })
local err15
rx15:on("error", function(m) err15 = m end)
rx15:write(string.char(0x09, 0x00))  -- ping with FIN=0
T.check("reject fragmented control", err15 and err15:find("FIN"))

-- invalid close code
local rx16 = Receiver.new({ is_server = false })
local err16
rx16:on("error", function(m) err16 = m end)
rx16:write(make_frame(0x08, buffer.write_uint16be(1004)))
T.check("reject invalid close code", err16 and err16:find("status code"))

-- close with 1 byte payload (invalid)
local rx17 = Receiver.new({ is_server = false })
local err17
rx17:on("error", function(m) err17 = m end)
rx17:write(string.char(0x88, 0x01, 0x00))
T.check("reject 1-byte close", err17 and err17:find("payload length"))

-- fragmented message
local rx18 = Receiver.new({ is_server = false })
local frag_msg
rx18:on("message", function(d) frag_msg = d end)
rx18:write(make_frame(0x01, "hel", false))
T.check("no msg yet", frag_msg == nil)
rx18:write(make_frame(0x00, "lo", true))
T.check_equal("fragmented result", frag_msg, "hello")

-- fragmented with 3 parts
local rx19 = Receiver.new({ is_server = false })
local frag3
rx19:on("message", function(d) frag3 = d end)
rx19:write(make_frame(0x02, "aa", false))
rx19:write(make_frame(0x00, "bb", false))
rx19:write(make_frame(0x00, "cc", true))
T.check_equal("3-part fragment", frag3, "aabbcc")

-- interleaved control during fragmentation
local rx20 = Receiver.new({ is_server = false })
local frag_result, got_ping = nil, false
rx20:on("message", function(d) frag_result = d end)
rx20:on("ping", function() got_ping = true end)
rx20:write(make_frame(0x01, "hel", false))
rx20:write(make_frame(0x09, ""))  -- ping
rx20:write(make_frame(0x00, "lo", true))
T.check("ping during frag", got_ping)
T.check_equal("frag with interleave", frag_result, "hello")

-- continuation without initial fragment (invalid)
local rx21 = Receiver.new({ is_server = false })
local err21
rx21:on("error", function(m) err21 = m end)
rx21:write(make_frame(0x00, "data", true))
T.check("reject orphan continuation", err21 and err21:find("opcode"))

-- new data frame during incomplete fragmented message
local rx22 = Receiver.new({ is_server = false })
local err22
rx22:on("error", function(m) err22 = m end)
rx22:write(make_frame(0x01, "part1", false))
rx22:write(make_frame(0x01, "part2", true))
T.check("reject new data during frag", err22 and err22:find("opcode"))

-- max_payload enforcement
local rx23 = Receiver.new({ is_server = false, max_payload = 10 })
local err23
rx23:on("error", function(m) err23 = m end)
rx23:write(make_frame(0x01, string.rep("x", 11)))
T.check("max_payload exceeded", err23 and err23:find("max payload"))

-- receiver refuses data after error
local rx24 = Receiver.new({ is_server = false })
local err24_count = 0
rx24:on("error", function() err24_count = err24_count + 1 end)
rx24:write(string.char(0x83, 0x00))  -- invalid opcode
T.check_equal("first error", err24_count, 1)
rx24:write(make_frame(0x01, "hello"))  -- should be ignored
T.check_equal("no second error", err24_count, 1)

-- multiple messages in one write
local rx25 = Receiver.new({ is_server = false })
local msgs = {}
rx25:on("message", function(d) msgs[#msgs + 1] = d end)
rx25:write(make_frame(0x01, "one") .. make_frame(0x01, "two") .. make_frame(0x01, "three"))
T.check_equal("multi message count", #msgs, 3)
T.check_equal("multi msg 1", msgs[1], "one")
T.check_equal("multi msg 2", msgs[2], "two")
T.check_equal("multi msg 3", msgs[3], "three")

-- split delivery (data arrives in small chunks)
local rx26 = Receiver.new({ is_server = false })
local split_msg
rx26:on("message", function(d) split_msg = d end)
local full_frame = make_frame(0x01, "split")
for i = 1, #full_frame do
  rx26:write(full_frame:sub(i, i))
end
T.check_equal("byte-at-a-time", split_msg, "split")

T.finish()
