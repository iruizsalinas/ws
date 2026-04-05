local compat = require("ws.compat")
local buffer = require("ws.buffer")
local validation = require("ws.validation")
local utf8_mod = require("ws.utf8")
local emitter = require("ws.emitter")
local band = compat.band
local byte, sub = string.byte, string.sub
local concat = table.concat

local GET_INFO = 0
local GET_PAYLOAD_LENGTH_16 = 1
local GET_PAYLOAD_LENGTH_64 = 2
local GET_MASK = 3
local GET_DATA = 4
local INFLATING = 5

local M = {}
M.__index = M
emitter.mixin(M)

function M.new(options)
  options = options or {}
  local self = setmetatable({}, M)
  emitter.init(self)

  self._is_server = options.is_server or false
  self._max_payload = options.max_payload or 0
  self._skip_utf8_validation = options.skip_utf8_validation or false
  self._extensions = options.extensions or {}

  self._buffered_bytes = 0
  self._buffers = {}
  self._buf_offset = 0

  self._compressed = false
  self._payload_length = 0
  self._mask = nil
  self._fragmented = 0
  self._masked = false
  self._fin = false
  self._opcode = 0

  self._total_payload_length = 0
  self._message_length = 0
  self._fragments = {}

  self._errored = false
  self._loop = false
  self._state = GET_INFO

  return self
end

function M:write(data)
  if self._errored then return end
  if self._opcode == 0x08 and self._state == GET_INFO then return end
  self._buffered_bytes = self._buffered_bytes + #data
  self._buffers[#self._buffers + 1] = data
  self:_start_loop()
end

function M:_consume(n)
  self._buffered_bytes = self._buffered_bytes - n

  local first = self._buffers[1]
  local offset = self._buf_offset
  local available = #first - offset

  if available == n then
    table.remove(self._buffers, 1)
    self._buf_offset = 0
    return sub(first, offset + 1)
  end

  if available > n then
    self._buf_offset = offset + n
    return sub(first, offset + 1, offset + n)
  end

  local parts = {}
  local remaining = n
  parts[1] = sub(first, offset + 1)
  remaining = remaining - available
  table.remove(self._buffers, 1)
  self._buf_offset = 0

  while remaining > 0 do
    local buf = self._buffers[1]
    if remaining >= #buf then
      parts[#parts + 1] = buf
      remaining = remaining - #buf
      table.remove(self._buffers, 1)
    else
      parts[#parts + 1] = sub(buf, 1, remaining)
      self._buf_offset = remaining
      remaining = 0
    end
  end

  return concat(parts)
end

function M:_start_loop()
  self._loop = true

  while self._loop do
    if self._state == GET_INFO then
      self:_get_info()
    elseif self._state == GET_PAYLOAD_LENGTH_16 then
      self:_get_payload_length_16()
    elseif self._state == GET_PAYLOAD_LENGTH_64 then
      self:_get_payload_length_64()
    elseif self._state == GET_MASK then
      self:_get_mask()
    elseif self._state == GET_DATA then
      self:_get_data()
    elseif self._state == INFLATING then
      self._loop = false
      return
    end
  end
end

function M:_error(message, status_code)
  self._loop = false
  self._errored = true
  self:emit("error", message, status_code)
end

function M:_get_info()
  if self._buffered_bytes < 2 then
    self._loop = false
    return
  end

  local header = self:_consume(2)
  local b1, b2 = byte(header, 1, 2)

  if band(b1, 0x30) ~= 0 then
    self:_error("RSV2 and RSV3 must be clear", 1002)
    return
  end

  local compressed = band(b1, 0x40) == 0x40
  if compressed and not self._extensions["permessage-deflate"] then
    self:_error("RSV1 must be clear", 1002)
    return
  end

  self._fin = band(b1, 0x80) == 0x80
  self._opcode = band(b1, 0x0F)
  self._payload_length = band(b2, 0x7F)

  if self._opcode == 0x00 then
    if compressed then
      self:_error("RSV1 must be clear", 1002)
      return
    end
    if self._fragmented == 0 then
      self:_error("invalid opcode 0", 1002)
      return
    end
    self._opcode = self._fragmented
  elseif self._opcode == 0x01 or self._opcode == 0x02 then
    if self._fragmented ~= 0 then
      self:_error("invalid opcode " .. self._opcode, 1002)
      return
    end
    self._compressed = compressed
  elseif self._opcode >= 0x08 and self._opcode <= 0x0A then
    if not self._fin then
      self:_error("FIN must be set", 1002)
      return
    end
    if compressed then
      self:_error("RSV1 must be clear", 1002)
      return
    end
    if self._payload_length > 125 then
      self:_error("invalid payload length " .. self._payload_length, 1002)
      return
    end
    if self._opcode == 0x08 and self._payload_length == 1 then
      self:_error("invalid payload length 1", 1002)
      return
    end
  else
    self:_error("invalid opcode " .. self._opcode, 1002)
    return
  end

  if not self._fin and self._fragmented == 0 then
    self._fragmented = self._opcode
  end

  self._masked = band(b2, 0x80) == 0x80
  if self._is_server then
    if not self._masked then
      self:_error("MASK must be set", 1002)
      return
    end
  elseif self._masked then
    self:_error("MASK must be clear", 1002)
    return
  end

  if self._payload_length == 126 then
    self._state = GET_PAYLOAD_LENGTH_16
  elseif self._payload_length == 127 then
    self._state = GET_PAYLOAD_LENGTH_64
  else
    self:_have_length()
  end
end

function M:_get_payload_length_16()
  if self._buffered_bytes < 2 then
    self._loop = false
    return
  end
  local data = self:_consume(2)
  self._payload_length = buffer.read_uint16be(data)
  self:_have_length()
end

function M:_get_payload_length_64()
  if self._buffered_bytes < 8 then
    self._loop = false
    return
  end
  local data = self:_consume(8)
  local high = buffer.read_uint32be(data, 1)
  local low = buffer.read_uint32be(data, 5)

  -- reject > 2^53 - 1
  if high > 0x1FFFFF then
    self:_error("payload length > 2^53 - 1", 1009)
    return
  end

  self._payload_length = high * 0x100000000 + low
  self:_have_length()
end

function M:_have_length()
  if self._payload_length > 0 and self._opcode < 0x08 then
    self._total_payload_length = self._total_payload_length + self._payload_length
    if self._max_payload > 0 and self._total_payload_length > self._max_payload then
      self:_error("max payload size exceeded", 1009)
      return
    end
  end

  if self._masked then
    self._state = GET_MASK
  else
    self._state = GET_DATA
  end
end

function M:_get_mask()
  if self._buffered_bytes < 4 then
    self._loop = false
    return
  end
  self._mask = self:_consume(4)
  self._state = GET_DATA
end

function M:_get_data()
  local data = ""

  if self._payload_length > 0 then
    if self._buffered_bytes < self._payload_length then
      self._loop = false
      return
    end
    data = self:_consume(self._payload_length)
    if self._masked then
      data = buffer.unmask(data, self._mask)
    end
  end

  if self._opcode >= 0x08 then
    self:_control_message(data)
    return
  end

  if self._compressed then
    local deflate = self._extensions["permessage-deflate"]
    if deflate then
      self._state = INFLATING
      deflate:decompress(data, self._fin, function(err, result)
        if err then
          local code = (type(err) == "string" and err:find("max payload")) and 1009 or 1007
          self:_error(err, code)
          return
        end
        if #result > 0 then
          self._message_length = self._message_length + #result
          if self._max_payload > 0 and self._message_length > self._max_payload then
            self:_error("max payload size exceeded", 1009)
            return
          end
          self._fragments[#self._fragments + 1] = result
        end
        self:_data_message()
        if self._state == GET_INFO then
          self:_start_loop()
        end
      end)
      return
    end
  end

  if #data > 0 then
    self._message_length = self._total_payload_length
    self._fragments[#self._fragments + 1] = data
  end

  self:_data_message()
end

function M:_data_message()
  if not self._fin then
    self._state = GET_INFO
    return
  end

  local fragments = self._fragments

  self._total_payload_length = 0
  self._message_length = 0
  self._fragmented = 0
  self._fragments = {}

  local message = buffer.concat(fragments)
  local is_binary = self._opcode == 0x02

  if not is_binary then
    if not self._skip_utf8_validation and not utf8_mod.is_valid(message) then
      self:_error("invalid UTF-8 sequence", 1007)
      return
    end
  end

  self._state = GET_INFO
  self:emit("message", message, is_binary)
end

function M:_control_message(data)
  if self._opcode == 0x08 then
    self._loop = false
    if #data == 0 then
      self:emit("conclude", 1005, "")
    else
      local code = buffer.read_uint16be(data)
      if not validation.is_valid_status_code(code) then
        self:_error("invalid status code " .. code, 1002)
        return
      end
      local reason = sub(data, 3)
      if not self._skip_utf8_validation and #reason > 0 and
         not utf8_mod.is_valid(reason) then
        self:_error("invalid UTF-8 sequence", 1007)
        return
      end
      self:emit("conclude", code, reason)
    end
    self._state = GET_INFO
    return
  end

  if self._opcode == 0x09 then
    self:emit("ping", data)
  else
    self:emit("pong", data)
  end
  self._state = GET_INFO
end

return M
