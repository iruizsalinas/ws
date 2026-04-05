local T = require("helper")
T.init("test_utf8.lua")

local utf8 = require("ws.utf8")

-- valid sequences
T.check("empty string", utf8.is_valid(""))
T.check("ascii", utf8.is_valid("hello world"))
T.check("all ascii printable", utf8.is_valid(" !\"#$%&'()*+,-./0123456789"))
T.check("2-byte: U+00E9 (e-acute)", utf8.is_valid("\xc3\xa9"))
T.check("2-byte: U+0080 (min)", utf8.is_valid("\xc2\x80"))
T.check("2-byte: U+07FF (max)", utf8.is_valid("\xdf\xbf"))
T.check("3-byte: U+4E2D (CJK)", utf8.is_valid("\xe4\xb8\xad"))
T.check("3-byte: U+0800 (min)", utf8.is_valid("\xe0\xa0\x80"))
T.check("3-byte: U+FFFD", utf8.is_valid("\xef\xbf\xbd"))
T.check("4-byte: U+1F600 (emoji)", utf8.is_valid("\xf0\x9f\x98\x80"))
T.check("4-byte: U+10000 (min)", utf8.is_valid("\xf0\x90\x80\x80"))
T.check("4-byte: U+10FFFF (max)", utf8.is_valid("\xf4\x8f\xbf\xbf"))
T.check("mixed valid", utf8.is_valid("hello \xc3\xa9 \xe4\xb8\xad \xf0\x9f\x98\x80"))

-- overlong 2-byte
T.check("reject overlong 2-byte 0xC0 0xAF", not utf8.is_valid("\xc0\xaf"))
T.check("reject overlong 2-byte 0xC1 0xBF", not utf8.is_valid("\xc1\xbf"))
T.check("reject overlong 2-byte 0xC0 0x80", not utf8.is_valid("\xc0\x80"))

-- overlong 3-byte
T.check("reject overlong 3-byte 0xE0 0x80 0xAF", not utf8.is_valid("\xe0\x80\xaf"))
T.check("reject overlong 3-byte 0xE0 0x9F 0xBF", not utf8.is_valid("\xe0\x9f\xbf"))

-- surrogates (U+D800 - U+DFFF)
T.check("reject U+D800", not utf8.is_valid("\xed\xa0\x80"))
T.check("reject U+DB7F", not utf8.is_valid("\xed\xad\xbf"))
T.check("reject U+DC00", not utf8.is_valid("\xed\xb0\x80"))
T.check("reject U+DFFF", not utf8.is_valid("\xed\xbf\xbf"))

-- > U+10FFFF
T.check("reject U+110000", not utf8.is_valid("\xf4\x90\x80\x80"))
T.check("reject 0xF5", not utf8.is_valid("\xf5\x80\x80\x80"))
T.check("reject 0xF8", not utf8.is_valid("\xf8\x80\x80\x80\x80"))

-- overlong 4-byte
T.check("reject overlong 4-byte 0xF0 0x80 0x80 0xAF", not utf8.is_valid("\xf0\x80\x80\xaf"))
T.check("reject overlong 4-byte 0xF0 0x8F 0xBF 0xBF", not utf8.is_valid("\xf0\x8f\xbf\xbf"))

-- truncated sequences
T.check("reject truncated 2-byte", not utf8.is_valid("\xc3"))
T.check("reject truncated 3-byte (1)", not utf8.is_valid("\xe4"))
T.check("reject truncated 3-byte (2)", not utf8.is_valid("\xe4\xb8"))
T.check("reject truncated 4-byte (1)", not utf8.is_valid("\xf0"))
T.check("reject truncated 4-byte (2)", not utf8.is_valid("\xf0\x9f"))
T.check("reject truncated 4-byte (3)", not utf8.is_valid("\xf0\x9f\x98"))

-- lone continuation bytes
T.check("reject lone 0x80", not utf8.is_valid("\x80"))
T.check("reject lone 0xBF", not utf8.is_valid("\xbf"))

-- invalid leading bytes
T.check("reject 0xFE", not utf8.is_valid("\xfe"))
T.check("reject 0xFF", not utf8.is_valid("\xff"))

-- bad continuation in multi-byte
T.check("reject bad cont in 2-byte", not utf8.is_valid("\xc3\x00"))
T.check("reject bad cont in 3-byte", not utf8.is_valid("\xe4\xb8\x00"))
T.check("reject bad cont in 4-byte", not utf8.is_valid("\xf0\x9f\x98\x00"))

T.finish()
