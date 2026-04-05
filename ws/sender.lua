local compat = require("ws.compat")
local frame_mod = require("ws.frame")
local buffer = require("ws.buffer")
local validation = require("ws.validation")

local M = {}
M.__index = M

local DEFAULT = 0
local DEFLATING = 1

function M.new(socket, extensions, is_client)
  local self = setmetatable({}, M)
  self._socket = socket
  self._extensions = extensions or {}
  self._is_client = is_client or false
  self._first_fragment = true
  self._compress = false
  self._queue = {}
  self._state = DEFAULT
  self.onerror = function() end
  return self
end

function M:_generate_mask()
  if self._is_client then
    return compat.random_bytes(4)
  end
  return nil
end

function M:send(data, options, cb)
  local deflate = self._extensions["permessage-deflate"]
  local opcode = options.binary and 0x02 or 0x01
  local rsv1 = options.compress or false
  local fin = options.fin ~= false

  if self._first_fragment then
    self._first_fragment = false
    if rsv1 and deflate and deflate.params then
      local key = deflate._is_server and "server_no_context_takeover"
                                      or "client_no_context_takeover"
      if deflate.params[key] then
        rsv1 = #data >= deflate._threshold
      end
    end
    self._compress = rsv1
  else
    rsv1 = false
    opcode = 0x00
  end

  if fin then self._first_fragment = true end

  if self._compress and deflate and self._state == DEFAULT then
    self:_deflate_and_send(data, fin, opcode, rsv1, cb)
  elseif self._state ~= DEFAULT then
    self._queue[#self._queue + 1] = {
      "send", data, fin, opcode, rsv1, self._compress, cb
    }
  else
    self:_send_frame(data, fin, opcode, rsv1, cb)
  end
end

function M:_deflate_and_send(data, fin, opcode, rsv1, cb)
  local deflate = self._extensions["permessage-deflate"]
  self._state = DEFLATING

  deflate:compress(data, fin, function(err, compressed)
    if err then
      self._state = DEFAULT
      if cb then cb(err) end
      self.onerror(err)
      return
    end

    self._state = DEFAULT
    self:_send_frame(compressed, fin, opcode, rsv1, cb)
    self:_dequeue()
  end)
end

function M:_send_frame(data, fin, opcode, rsv1, cb)
  local mask_key = self:_generate_mask()
  local encoded = frame_mod.encode(data, {
    fin = fin,
    opcode = opcode,
    mask = mask_key ~= nil,
    mask_key = mask_key,
    rsv1 = rsv1,
  })

  local ok, err = self._socket:send(encoded)
  if not ok then
    if cb then cb(err) end
    self.onerror(err)
    return
  end
  if cb then cb() end
end

function M:ping(data, cb)
  data = data or ""
  if #data > 125 then
    error("ping data must not exceed 125 bytes", 2)
  end

  local mask_key = self:_generate_mask()
  local encoded = frame_mod.encode(data, {
    fin = true,
    opcode = frame_mod.PING,
    mask = mask_key ~= nil,
    mask_key = mask_key,
    rsv1 = false,
  })

  if self._state ~= DEFAULT then
    self._queue[#self._queue + 1] = { "raw", encoded, cb }
    return
  end

  local ok, err = self._socket:send(encoded)
  if not ok then
    if cb then cb(err) end
    self.onerror(err)
    return
  end
  if cb then cb() end
end

function M:pong(data, cb)
  data = data or ""
  if #data > 125 then
    error("pong data must not exceed 125 bytes", 2)
  end

  local mask_key = self:_generate_mask()
  local encoded = frame_mod.encode(data, {
    fin = true,
    opcode = frame_mod.PONG,
    mask = mask_key ~= nil,
    mask_key = mask_key,
    rsv1 = false,
  })

  if self._state ~= DEFAULT then
    self._queue[#self._queue + 1] = { "raw", encoded, cb }
    return
  end

  local ok, err = self._socket:send(encoded)
  if not ok then
    if cb then cb(err) end
    self.onerror(err)
    return
  end
  if cb then cb() end
end

function M:close(code, reason, cb)
  local data
  if not code then
    data = ""
  elseif type(code) ~= "number" or not validation.is_valid_status_code(code) then
    error("invalid close code", 2)
  elseif not reason or reason == "" then
    data = buffer.write_uint16be(code)
  else
    if #reason > 123 then
      error("close reason must not exceed 123 bytes", 2)
    end
    data = buffer.write_uint16be(code) .. reason
  end

  local mask_key = self:_generate_mask()
  local encoded = frame_mod.encode(data, {
    fin = true,
    opcode = frame_mod.CLOSE,
    mask = mask_key ~= nil,
    mask_key = mask_key,
    rsv1 = false,
  })

  if self._state ~= DEFAULT then
    self._queue[#self._queue + 1] = { "raw", encoded, cb }
    return
  end

  local ok, err = self._socket:send(encoded)
  if not ok then
    if cb then cb(err) end
    self.onerror(err)
    return
  end
  if cb then cb() end
end

function M:_dequeue()
  while self._state == DEFAULT and #self._queue > 0 do
    local item = table.remove(self._queue, 1)
    local kind = item[1]

    if kind == "raw" then
      local ok, err = self._socket:send(item[2])
      local cb = item[3]
      if not ok then
        if cb then cb(err) end
        self.onerror(err)
      elseif cb then cb() end
    elseif kind == "send" then
      local data, fin, opcode, rsv1, do_compress, cb =
        item[2], item[3], item[4], item[5], item[6], item[7]
      if do_compress then
        self:_deflate_and_send(data, fin, opcode, rsv1, cb)
      else
        self:_send_frame(data, fin, opcode, rsv1, cb)
      end
    end
  end
end

return M
