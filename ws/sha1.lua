local compat = require("ws.compat")
local band, bor, bxor, bnot = compat.band, compat.bor, compat.bxor, compat.bnot
local lshift, rshift = compat.lshift, compat.rshift
local add32, rotl32 = compat.add32, compat.rotl32

local byte, char, sub = string.byte, string.char, string.sub
local concat = table.concat
local floor = math.floor

local function preprocess(msg)
  local len = #msg
  local bit_len = len * 8

  -- append 0x80
  msg = msg .. char(0x80)

  -- pad to 56 mod 64
  local pad = (56 - (#msg % 64)) % 64
  msg = msg .. string.rep(char(0), pad)

  -- append 64-bit big-endian length
  local len_hi = floor(bit_len / 0x100000000)
  local len_lo = bit_len % 0x100000000
  msg = msg .. char(
    band(rshift(len_hi, 24), 0xFF), band(rshift(len_hi, 16), 0xFF),
    band(rshift(len_hi, 8), 0xFF), band(len_hi, 0xFF),
    band(rshift(len_lo, 24), 0xFF), band(rshift(len_lo, 16), 0xFF),
    band(rshift(len_lo, 8), 0xFF), band(len_lo, 0xFF)
  )

  return msg
end

local function sha1(msg)
  msg = preprocess(msg)

  local h0 = 0x67452301
  local h1 = 0xEFCDAB89
  local h2 = 0x98BADCFE
  local h3 = 0x10325476
  local h4 = 0xC3D2E1F0

  local w = {}

  for chunk_start = 1, #msg, 64 do
    for i = 0, 15 do
      local j = chunk_start + i * 4
      local b1, b2, b3, b4 = byte(msg, j, j + 3)
      w[i] = b1 * 0x1000000 + b2 * 0x10000 + b3 * 0x100 + b4
    end

    for i = 16, 79 do
      w[i] = rotl32(bxor(bxor(w[i-3], w[i-8]), bxor(w[i-14], w[i-16])), 1)
    end

    local a, b, c, d, e = h0, h1, h2, h3, h4

    for i = 0, 79 do
      local f, k
      if i <= 19 then
        f = bor(band(b, c), band(bnot(b), d))
        k = 0x5A827999
      elseif i <= 39 then
        f = bxor(bxor(b, c), d)
        k = 0x6ED9EBA1
      elseif i <= 59 then
        f = bor(bor(band(b, c), band(b, d)), band(c, d))
        k = 0x8F1BBCDC
      else
        f = bxor(bxor(b, c), d)
        k = 0xCA62C1D6
      end

      local temp = add32(add32(add32(add32(rotl32(a, 5), f), e), k), w[i])
      e = d
      d = c
      c = rotl32(b, 30)
      b = a
      a = temp
    end

    h0 = add32(h0, a)
    h1 = add32(h1, b)
    h2 = add32(h2, c)
    h3 = add32(h3, d)
    h4 = add32(h4, e)
  end

  local function w32_to_bytes(w)
    return char(
      band(rshift(w, 24), 0xFF), band(rshift(w, 16), 0xFF),
      band(rshift(w, 8), 0xFF), band(w, 0xFF)
    )
  end

  return w32_to_bytes(h0) .. w32_to_bytes(h1) .. w32_to_bytes(h2) ..
         w32_to_bytes(h3) .. w32_to_bytes(h4)
end

local hex_chars = {}
for i = 0, 255 do
  hex_chars[i] = string.format("%02x", i)
end

local function sha1_hex(msg)
  local hash = sha1(msg)
  local parts = {}
  for i = 1, 20 do
    parts[i] = hex_chars[byte(hash, i)]
  end
  return concat(parts)
end

return {
  sha1 = sha1,
  sha1_hex = sha1_hex,
}
