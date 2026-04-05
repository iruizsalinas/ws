local T = require("helper")
T.init("test_stress.lua")

local frame = require("ws.frame")
local Receiver = require("ws.receiver")
local buffer = require("ws.buffer")

local function make_frame(opcode, data, fin)
  return frame.encode(data or "", {
    fin = fin ~= false,
    opcode = opcode,
    mask = false,
    rsv1 = false,
  })
end

-- 0-byte payload frame
local rx1 = Receiver.new({ is_server = false })
local msg1
rx1:on("message", function(d) msg1 = d end)
rx1:write(make_frame(0x01, ""))
T.check_equal("0-byte msg", msg1, "")

-- 1MB binary frame
local big = {}
for i = 1, 1024 do big[i] = string.rep(string.char(i % 256), 1024) end
local big_data = table.concat(big)
T.check_equal("1MB data len", #big_data, 1024 * 1024)
local rx2 = Receiver.new({ is_server = false })
local big_msg
rx2:on("message", function(d) big_msg = d end)
rx2:write(make_frame(0x02, big_data))
T.check_equal("1MB roundtrip len", #big_msg, 1024 * 1024)
T.check_equal("1MB roundtrip data", big_msg, big_data)

-- 10000 small frames
local rx3 = Receiver.new({ is_server = false })
local count3 = 0
rx3:on("message", function() count3 = count3 + 1 end)
local batch = {}
for i = 1, 10000 do
  batch[i] = make_frame(0x01, "msg" .. i)
end
rx3:write(table.concat(batch))
T.check_equal("10k frames", count3, 10000)

-- masking roundtrip stress: many different keys
local all_ok = true
local test_data = "The quick brown fox jumps over the lazy dog"
for i = 0, 255 do
  local key = string.char(i, (i * 7) % 256, (i * 13) % 256, (i * 31) % 256)
  local masked = buffer.mask(test_data, key)
  local unmasked = buffer.unmask(masked, key)
  if unmasked ~= test_data then
    all_ok = false
    break
  end
end
T.check("256 mask keys roundtrip", all_ok)

-- frame encode/decode roundtrip for every opcode
local opcodes = { 0x01, 0x02, 0x08, 0x09, 0x0A }
for _, op in ipairs(opcodes) do
  local data = op < 0x08 and "test_data" or (op == 0x08 and buffer.write_uint16be(1000) or "ping")
  local encoded = frame.encode(data, {
    fin = true, opcode = op, mask = false, rsv1 = false
  })
  local b1 = string.byte(encoded, 1)
  local decoded_opcode = b1 % 16
  T.check_equal("opcode " .. op .. " roundtrip", decoded_opcode, op)
end

-- fragmented binary message
local rx4 = Receiver.new({ is_server = false })
local frag4
rx4:on("message", function(d, b) frag4 = { d, b } end)
rx4:write(make_frame(0x02, "\x00\x01", false))
rx4:write(make_frame(0x00, "\x02\x03", false))
rx4:write(make_frame(0x00, "\x04\x05", true))
T.check_equal("binary frag data", frag4[1], "\x00\x01\x02\x03\x04\x05")
T.check_equal("binary frag is_binary", frag4[2], true)

-- close frame with max-length reason (123 bytes)
local max_reason = string.rep("a", 123)
local close_data = buffer.write_uint16be(1000) .. max_reason
local close_frame = make_frame(0x08, close_data)
local rx5 = Receiver.new({ is_server = false })
local c5, r5
rx5:on("conclude", function(c, r) c5 = c; r5 = r end)
rx5:write(close_frame)
T.check_equal("max reason code", c5, 1000)
T.check_equal("max reason len", #r5, 123)

-- large frame with 16-bit length encoded via receiver
local data_300 = string.rep("x", 300)
local rx6 = Receiver.new({ is_server = false })
local msg6
rx6:on("message", function(d) msg6 = d end)
rx6:write(make_frame(0x01, data_300))
T.check_equal("300-byte msg", msg6, data_300)

T.finish()
