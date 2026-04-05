local compat = require("ws.compat")
local buffer = require("ws.buffer")
local band, bor, rshift = compat.band, compat.bor, compat.rshift
local byte, char = string.byte, string.char
local floor = math.floor

local M = {}

M.CONTINUATION = 0x00
M.TEXT = 0x01
M.BINARY = 0x02
M.CLOSE = 0x08
M.PING = 0x09
M.PONG = 0x0A

M.GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
M.CLOSE_TIMEOUT = 30

function M.encode(data, options)
  local fin = options.fin
  local opcode = options.opcode
  local do_mask = options.mask
  local mask_key = options.mask_key
  local rsv1 = options.rsv1

  local byte1 = opcode
  if fin then byte1 = bor(byte1, 0x80) end
  if rsv1 then byte1 = bor(byte1, 0x40) end

  local len = #data
  local byte2 = 0
  local len_bytes = ""

  if len < 126 then
    byte2 = len
  elseif len < 65536 then
    byte2 = 126
    len_bytes = buffer.write_uint16be(len)
  else
    byte2 = 127
    local high = floor(len / 0x100000000)
    local low = len % 0x100000000
    len_bytes = buffer.write_uint32be(high) .. buffer.write_uint32be(low)
  end

  if do_mask then
    byte2 = bor(byte2, 0x80)
    local header = char(byte1, byte2) .. len_bytes .. mask_key
    local masked = buffer.mask(data, mask_key)
    return header .. masked
  end

  return char(byte1, byte2) .. len_bytes .. data
end

return M
