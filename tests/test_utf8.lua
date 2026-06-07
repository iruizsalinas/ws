local T = require("helper")
T.init("test_utf8.lua")

local utf8 = require("ws.utf8")

local function b(...)
  return string.char(...)
end

-- valid sequences
T.check("empty string", utf8.is_valid(""))
T.check("ascii", utf8.is_valid("hello world"))
T.check("all ascii printable", utf8.is_valid(" !\"#$%&'()*+,-./0123456789"))
T.check("2-byte: U+00E9 (e-acute)", utf8.is_valid(b(0xC3, 0xA9)))
T.check("2-byte: U+0080 (min)", utf8.is_valid(b(0xC2, 0x80)))
T.check("2-byte: U+07FF (max)", utf8.is_valid(b(0xDF, 0xBF)))
T.check("3-byte: U+4E2D (CJK)", utf8.is_valid(b(0xE4, 0xB8, 0xAD)))
T.check("3-byte: U+0800 (min)", utf8.is_valid(b(0xE0, 0xA0, 0x80)))
T.check("3-byte: U+FFFD", utf8.is_valid(b(0xEF, 0xBF, 0xBD)))
T.check("4-byte: U+1F600 (emoji)", utf8.is_valid(b(0xF0, 0x9F, 0x98, 0x80)))
T.check("4-byte: U+10000 (min)", utf8.is_valid(b(0xF0, 0x90, 0x80, 0x80)))
T.check("4-byte: U+10FFFF (max)", utf8.is_valid(b(0xF4, 0x8F, 0xBF, 0xBF)))
T.check("mixed valid", utf8.is_valid("hello " .. b(0xC3, 0xA9) .. " " .. b(0xE4, 0xB8, 0xAD) .. " " .. b(0xF0, 0x9F, 0x98, 0x80)))

-- overlong 2-byte
T.check("reject overlong 2-byte 0xC0 0xAF", not utf8.is_valid(b(0xC0, 0xAF)))
T.check("reject overlong 2-byte 0xC1 0xBF", not utf8.is_valid(b(0xC1, 0xBF)))
T.check("reject overlong 2-byte 0xC0 0x80", not utf8.is_valid(b(0xC0, 0x80)))

-- overlong 3-byte
T.check("reject overlong 3-byte 0xE0 0x80 0xAF", not utf8.is_valid(b(0xE0, 0x80, 0xAF)))
T.check("reject overlong 3-byte 0xE0 0x9F 0xBF", not utf8.is_valid(b(0xE0, 0x9F, 0xBF)))

-- surrogates (U+D800 - U+DFFF)
T.check("reject U+D800", not utf8.is_valid(b(0xED, 0xA0, 0x80)))
T.check("reject U+DB7F", not utf8.is_valid(b(0xED, 0xAD, 0xBF)))
T.check("reject U+DC00", not utf8.is_valid(b(0xED, 0xB0, 0x80)))
T.check("reject U+DFFF", not utf8.is_valid(b(0xED, 0xBF, 0xBF)))

-- > U+10FFFF
T.check("reject U+110000", not utf8.is_valid(b(0xF4, 0x90, 0x80, 0x80)))
T.check("reject 0xF5", not utf8.is_valid(b(0xF5, 0x80, 0x80, 0x80)))
T.check("reject 0xF8", not utf8.is_valid(b(0xF8, 0x80, 0x80, 0x80, 0x80)))

-- overlong 4-byte
T.check("reject overlong 4-byte 0xF0 0x80 0x80 0xAF", not utf8.is_valid(b(0xF0, 0x80, 0x80, 0xAF)))
T.check("reject overlong 4-byte 0xF0 0x8F 0xBF 0xBF", not utf8.is_valid(b(0xF0, 0x8F, 0xBF, 0xBF)))

-- truncated sequences
T.check("reject truncated 2-byte", not utf8.is_valid(b(0xC3)))
T.check("reject truncated 3-byte (1)", not utf8.is_valid(b(0xE4)))
T.check("reject truncated 3-byte (2)", not utf8.is_valid(b(0xE4, 0xB8)))
T.check("reject truncated 4-byte (1)", not utf8.is_valid(b(0xF0)))
T.check("reject truncated 4-byte (2)", not utf8.is_valid(b(0xF0, 0x9F)))
T.check("reject truncated 4-byte (3)", not utf8.is_valid(b(0xF0, 0x9F, 0x98)))

-- lone continuation bytes
T.check("reject lone 0x80", not utf8.is_valid(b(0x80)))
T.check("reject lone 0xBF", not utf8.is_valid(b(0xBF)))

-- invalid leading bytes
T.check("reject 0xFE", not utf8.is_valid(b(0xFE)))
T.check("reject 0xFF", not utf8.is_valid(b(0xFF)))

-- bad continuation in multi-byte
T.check("reject bad cont in 2-byte", not utf8.is_valid(b(0xC3, 0x00)))
T.check("reject bad cont in 3-byte", not utf8.is_valid(b(0xE4, 0xB8, 0x00)))
T.check("reject bad cont in 4-byte", not utf8.is_valid(b(0xF0, 0x9F, 0x98, 0x00)))

T.finish()
