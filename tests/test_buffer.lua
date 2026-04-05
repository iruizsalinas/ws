local T = require("helper")
T.init("test_buffer.lua")

local buffer = require("ws.buffer")
local compat = require("ws.compat")

-- mask/unmask roundtrip
local original = "Hello, World!"
local key = "ABCD"
local masked = buffer.mask(original, key)
T.check("mask changes data", masked ~= original)
T.check("mask roundtrip", buffer.unmask(masked, key) == original)

-- zero mask is identity
T.check("zero mask identity", buffer.mask("test", "\x00\x00\x00\x00") == "test")

-- empty data
T.check("mask empty", buffer.mask("", "ABCD") == "")

-- verify XOR correctness for every byte 0x00-0xFF
local all_bytes = {}
for i = 0, 255 do all_bytes[i + 1] = string.char(i) end
local all_data = table.concat(all_bytes)
local mask_key = "\x37\xfa\x21\x3d"
local masked_all = buffer.mask(all_data, mask_key)
T.check_equal("mask all bytes length", #masked_all, 256)

local m = { string.byte(mask_key, 1, 4) }
local all_correct = true
for i = 1, 256 do
  local expected = compat.bxor(i - 1, m[((i - 1) % 4) + 1])
  if string.byte(masked_all, i) ~= expected then
    all_correct = false
    break
  end
end
T.check("XOR correctness all bytes", all_correct)
T.check("mask all roundtrip", buffer.unmask(masked_all, mask_key) == all_data)

-- different keys produce different wire bytes but same decoded
local key2 = "\xAA\xBB\xCC\xDD"
local masked1 = buffer.mask("hello", "ABCD")
local masked2 = buffer.mask("hello", key2)
T.check("different keys differ", masked1 ~= masked2)
T.check("different keys decode same",
  buffer.unmask(masked1, "ABCD") == buffer.unmask(masked2, key2))

-- large data masking (> 256 byte chunk boundary)
local big_data = string.rep("X", 1000)
local big_masked = buffer.mask(big_data, "ABCD")
T.check("big data roundtrip", buffer.unmask(big_masked, "ABCD") == big_data)

-- concat
T.check_equal("concat empty", buffer.concat({}), "")
T.check_equal("concat single", buffer.concat({"hello"}), "hello")
T.check_equal("concat multi", buffer.concat({"he", "ll", "o"}), "hello")

-- uint read/write
T.check_equal("uint16 roundtrip 0", buffer.read_uint16be(buffer.write_uint16be(0)), 0)
T.check_equal("uint16 roundtrip 256", buffer.read_uint16be(buffer.write_uint16be(256)), 256)
T.check_equal("uint16 roundtrip 65535", buffer.read_uint16be(buffer.write_uint16be(65535)), 65535)
T.check_equal("uint32 roundtrip 0", buffer.read_uint32be(buffer.write_uint32be(0)), 0)
T.check_equal("uint32 roundtrip 1000000", buffer.read_uint32be(buffer.write_uint32be(1000000)), 1000000)

T.finish()
