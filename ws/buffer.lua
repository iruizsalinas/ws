local compat = require("ws.compat")
local bxor = compat.bxor
local byte, char, sub = string.byte, string.char, string.sub
local concat = table.concat

local M = {}

local has_ffi, ffi = pcall(require, "ffi")
has_ffi = has_ffi and compat.is_luajit

if has_ffi then
  function M.mask(data, mask_key)
    local len = #data
    if len == 0 then return data end
    local m1, m2, m3, m4 = byte(mask_key, 1, 4)
    if m1 == 0 and m2 == 0 and m3 == 0 and m4 == 0 then return data end

    local buf = ffi.new("uint8_t[?]", len)
    ffi.copy(buf, data, len)
    local m = ffi.new("uint8_t[4]", m1, m2, m3, m4)
    for i = 0, len - 1 do
      buf[i] = bxor(buf[i], m[i % 4])
    end
    return ffi.string(buf, len)
  end
else
  function M.mask(data, mask_key)
    local len = #data
    if len == 0 then return data end
    local m1, m2, m3, m4 = byte(mask_key, 1, 4)
    if m1 == 0 and m2 == 0 and m3 == 0 and m4 == 0 then return data end

    local mask = { m1, m2, m3, m4 }
    local parts = {}
    local chunk_size = 256
    local unpack = compat.unpack

    for pos = 1, len, chunk_size do
      local end_pos = pos + chunk_size - 1
      if end_pos > len then end_pos = len end
      local bytes = { byte(data, pos, end_pos) }
      for i = 1, #bytes do
        bytes[i] = bxor(bytes[i], mask[((pos + i - 2) % 4) + 1])
      end
      parts[#parts + 1] = char(unpack(bytes))
    end
    return concat(parts)
  end
end

M.unmask = M.mask

function M.concat(list)
  if #list == 0 then return "" end
  if #list == 1 then return list[1] end
  return concat(list)
end

function M.read_uint16be(data, offset)
  offset = offset or 1
  local b1, b2 = byte(data, offset, offset + 1)
  return b1 * 256 + b2
end

function M.read_uint32be(data, offset)
  offset = offset or 1
  local b1, b2, b3, b4 = byte(data, offset, offset + 3)
  return b1 * 16777216 + b2 * 65536 + b3 * 256 + b4
end

function M.write_uint16be(value)
  return char(
    math.floor(value / 256) % 256,
    value % 256
  )
end

function M.write_uint32be(value)
  return char(
    math.floor(value / 16777216) % 256,
    math.floor(value / 65536) % 256,
    math.floor(value / 256) % 256,
    value % 256
  )
end

return M
