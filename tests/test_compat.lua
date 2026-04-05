local T = require("helper")
T.init("test_compat.lua")

local compat = require("ws.compat")

-- bxor basics
T.check_equal("xor 0,0", compat.bxor(0, 0), 0)
T.check_equal("xor FF,00", compat.bxor(0xFF, 0x00), 0xFF)
T.check_equal("xor FF,FF", compat.bxor(0xFF, 0xFF), 0)
T.check_equal("xor AA,55", compat.bxor(0xAA, 0x55), 0xFF)
T.check_equal("xor 12345678,87654321",
  compat.bxor(0x12345678, 0x87654321), 0x95511559)

-- band
T.check_equal("and FF,0F", compat.band(0xFF, 0x0F), 0x0F)
T.check_equal("and 80,80", compat.band(0x80, 0x80), 0x80)
T.check_equal("and FF,00", compat.band(0xFF, 0x00), 0x00)

-- bor
T.check_equal("or F0,0F", compat.bor(0xF0, 0x0F), 0xFF)
T.check_equal("or 00,00", compat.bor(0x00, 0x00), 0x00)

-- rshift/lshift
T.check_equal("rshift 0x80,1", compat.rshift(0x80, 1), 0x40)
T.check_equal("rshift 0xFF,4", compat.rshift(0xFF, 4), 0x0F)
T.check_equal("lshift 1,7", compat.lshift(1, 7), 128)
T.check_equal("lshift 0xFF,8", compat.lshift(0xFF, 8), 0xFF00)

-- bnot: on Lua 5.3+ native ~ returns signed, so check via double-negation
T.check_equal("not double neg", compat.bnot(compat.bnot(0)), 0)
T.check_equal("not double neg FF", compat.bnot(compat.bnot(0xFF)), 0xFF)
-- bnot(x) AND 0xFF should give complement of low byte
T.check_equal("not 0 low byte", compat.band(compat.bnot(0), 0xFF), 0xFF)
T.check_equal("not FF low byte", compat.band(compat.bnot(0xFF), 0xFF), 0x00)

-- add32
T.check_equal("add32 simple", compat.add32(1, 2), 3)
T.check_equal("add32 overflow", compat.add32(0xFFFFFFFF, 1), 0)
T.check_equal("add32 wrap", compat.add32(0x80000000, 0x80000000), 0)

-- rotl32
T.check_equal("rotl 1,1", compat.rotl32(1, 1), 2)
T.check_equal("rotl 0x80000000,1", compat.rotl32(0x80000000, 1), 1)

-- random_bytes
local rb = compat.random_bytes(16)
T.check_equal("random len", #rb, 16)
local rb2 = compat.random_bytes(16)
T.check("random different", rb ~= rb2) -- extremely unlikely to be equal
T.check_equal("random 0 len", #compat.random_bytes(0), 0)

-- unpack
local a, b, c = compat.unpack({10, 20, 30})
T.check_equal("unpack 1", a, 10)
T.check_equal("unpack 2", b, 20)
T.check_equal("unpack 3", c, 30)

-- xor all byte values with known mask
local mask_byte = 0x37
local all_ok = true
for i = 0, 255 do
  local result = compat.bxor(i, mask_byte)
  -- verify by properties: a XOR b XOR b == a
  if compat.bxor(result, mask_byte) ~= i then
    all_ok = false
    break
  end
end
T.check("xor all bytes roundtrip", all_ok)

T.finish()
