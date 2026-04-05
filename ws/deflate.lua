local limiter_mod = require("ws.limiter")

local has_zlib, zlib = pcall(require, "zlib")

local TRAILER = string.char(0x00, 0x00, 0xFF, 0xFF)
local DEFAULT_WINDOW_BITS = 15

local zlib_limiter

local M = {}
M.__index = M
M.extension_name = "permessage-deflate"

function M.available()
  return has_zlib
end

function M.new(options)
  if not has_zlib then
    error("lua-zlib is required for permessage-deflate", 2)
  end

  options = options or {}
  local self = setmetatable({}, M)
  self._is_server = options.is_server or false
  self._threshold = options.threshold or 1024
  self._max_payload = options.max_payload or 0
  self._server_no_context_takeover = options.server_no_context_takeover or false
  self._client_no_context_takeover = options.client_no_context_takeover or false
  self._server_max_window_bits = options.server_max_window_bits
  self._client_max_window_bits = options.client_max_window_bits

  self._deflate = nil
  self._inflate = nil
  self.params = nil

  if not zlib_limiter then
    local concurrency = options.concurrency_limit or 10
    zlib_limiter = limiter_mod.new(concurrency)
  end

  return self
end

function M:offer()
  local params = {}
  if self._server_no_context_takeover then
    params.server_no_context_takeover = true
  end
  if self._client_no_context_takeover then
    params.client_no_context_takeover = true
  end
  if self._server_max_window_bits then
    params.server_max_window_bits = self._server_max_window_bits
  end
  if self._client_max_window_bits then
    params.client_max_window_bits = self._client_max_window_bits
  elseif self._client_max_window_bits == nil then
    params.client_max_window_bits = true
  end
  return params
end

function M:accept(configurations)
  configurations = self:_normalize_params(configurations)

  if self._is_server then
    self.params = self:_accept_as_server(configurations)
  else
    self.params = self:_accept_as_client(configurations)
  end

  return self.params
end

function M:_accept_as_server(offers)
  local accepted
  for _, params in ipairs(offers) do
    local dominated = false
    if self._server_no_context_takeover == false and
       params.server_no_context_takeover then
      dominated = true
    end
    if params.server_max_window_bits and
       (self._server_max_window_bits == false or
        (type(self._server_max_window_bits) == "number" and
         self._server_max_window_bits > params.server_max_window_bits)) then
      dominated = true
    end
    if type(self._client_max_window_bits) == "number" and
       not params.client_max_window_bits then
      dominated = true
    end
    if not dominated then
      accepted = params
      break
    end
  end

  if not accepted then
    error("none of the extension offers can be accepted")
  end

  if self._server_no_context_takeover then
    accepted.server_no_context_takeover = true
  end
  if self._client_no_context_takeover then
    accepted.client_no_context_takeover = true
  end
  if type(self._server_max_window_bits) == "number" then
    accepted.server_max_window_bits = self._server_max_window_bits
  end
  if type(self._client_max_window_bits) == "number" then
    accepted.client_max_window_bits = self._client_max_window_bits
  elseif accepted.client_max_window_bits == true or
         self._client_max_window_bits == false then
    accepted.client_max_window_bits = nil
  end

  return accepted
end

function M:_accept_as_client(response)
  local params = response[1]

  if self._client_no_context_takeover == false and
     params.client_no_context_takeover then
    error("unexpected parameter: client_no_context_takeover")
  end

  if not params.client_max_window_bits then
    if type(self._client_max_window_bits) == "number" then
      params.client_max_window_bits = self._client_max_window_bits
    end
  elseif self._client_max_window_bits == false or
         (type(self._client_max_window_bits) == "number" and
          params.client_max_window_bits > self._client_max_window_bits) then
    error("unexpected or invalid parameter: client_max_window_bits")
  end

  return params
end

function M:_normalize_params(configurations)
  for _, params in ipairs(configurations) do
    for key, value in pairs(params) do
      if type(value) == "table" then
        if #value > 1 then
          error("parameter \"" .. key .. "\" must have only a single value")
        end
        value = value[1]
      end

      if key == "client_max_window_bits" then
        if value ~= true then
          local num = tonumber(value)
          if not num or num ~= math.floor(num) or num < 8 or num > 15 then
            error("invalid value for parameter \"" .. key .. "\": " .. tostring(value))
          end
          value = num
        elseif not self._is_server then
          error("invalid value for parameter \"" .. key .. "\": " .. tostring(value))
        end
      elseif key == "server_max_window_bits" then
        local num = tonumber(value)
        if not num or num ~= math.floor(num) or num < 8 or num > 15 then
          error("invalid value for parameter \"" .. key .. "\": " .. tostring(value))
        end
        value = num
      elseif key == "client_no_context_takeover" or
             key == "server_no_context_takeover" then
        if value ~= true then
          error("invalid value for parameter \"" .. key .. "\": " .. tostring(value))
        end
      else
        error("unknown parameter \"" .. key .. "\"")
      end

      params[key] = value
    end
  end
  return configurations
end

function M:decompress(data, fin, callback)
  zlib_limiter:add(function(done)
    self:_decompress(data, fin, function(err, result)
      done()
      callback(err, result)
    end)
  end)
end

function M:compress(data, fin, callback)
  zlib_limiter:add(function(done)
    self:_compress(data, fin, function(err, result)
      done()
      callback(err, result)
    end)
  end)
end

function M:_decompress(data, fin, callback)
  local endpoint = self._is_server and "client" or "server"

  if not self._inflate then
    local key = endpoint .. "_max_window_bits"
    local window_bits = type(self.params[key]) == "number"
                        and self.params[key] or DEFAULT_WINDOW_BITS
    self._inflate = zlib.inflate(window_bits)
  end

  local input = data
  if fin then input = data .. TRAILER end

  local ok, result = pcall(self._inflate, input)
  if not ok then
    self._inflate = nil
    callback(result)
    return
  end

  if self._max_payload > 0 and #result > self._max_payload then
    self._inflate = nil
    callback("max payload size exceeded")
    return
  end

  if fin and self.params[endpoint .. "_no_context_takeover"] then
    self._inflate = nil
  end

  callback(nil, result)
end

function M:_compress(data, fin, callback)
  local endpoint = self._is_server and "server" or "client"

  if not self._deflate then
    local key = endpoint .. "_max_window_bits"
    local window_bits = type(self.params[key]) == "number"
                        and self.params[key] or DEFAULT_WINDOW_BITS
    self._deflate = zlib.deflate(nil, nil, window_bits)
  end

  local ok, result = pcall(self._deflate, data, "sync")
  if not ok then
    self._deflate = nil
    callback(result)
    return
  end

  -- strip the 4-byte trailer on final fragment
  if fin and #result >= 4 then
    local tail = result:sub(-4)
    if tail == TRAILER then
      result = result:sub(1, -5)
    end
  end

  if fin and self.params[endpoint .. "_no_context_takeover"] then
    self._deflate = nil
  end

  callback(nil, result)
end

function M:cleanup()
  self._inflate = nil
  self._deflate = nil
end

return M
