local T = require("helper")
T.init("test_base64.lua")

local base64 = require("ws.base64")

-- rfc 4648 test vectors
T.check_equal("encode empty", base64.encode(""), "")
T.check_equal("encode f", base64.encode("f"), "Zg==")
T.check_equal("encode fo", base64.encode("fo"), "Zm8=")
T.check_equal("encode foo", base64.encode("foo"), "Zm9v")
T.check_equal("encode foob", base64.encode("foob"), "Zm9vYg==")
T.check_equal("encode fooba", base64.encode("fooba"), "Zm9vYmE=")
T.check_equal("encode foobar", base64.encode("foobar"), "Zm9vYmFy")

T.check_equal("decode empty", base64.decode(""), "")
T.check_equal("decode Zg==", base64.decode("Zg=="), "f")
T.check_equal("decode Zm8=", base64.decode("Zm8="), "fo")
T.check_equal("decode Zm9v", base64.decode("Zm9v"), "foo")
T.check_equal("decode Zm9vYg==", base64.decode("Zm9vYg=="), "foob")
T.check_equal("decode Zm9vYmE=", base64.decode("Zm9vYmE="), "fooba")
T.check_equal("decode Zm9vYmFy", base64.decode("Zm9vYmFy"), "foobar")

-- roundtrip with all byte values
local all_bytes = {}
for i = 0, 255 do all_bytes[i + 1] = string.char(i) end
local all_data = table.concat(all_bytes)
T.check_equal("roundtrip 0x00-0xFF",
  base64.decode(base64.encode(all_data)), all_data)

-- 16-byte random-like data (websocket key size)
local key_data = "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"
local encoded = base64.encode(key_data)
T.check_equal("16-byte roundtrip", base64.decode(encoded), key_data)
T.check_equal("16-byte encoded length", #encoded, 24)

T.finish()
