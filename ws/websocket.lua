local emitter = require("ws.emitter")
local frame_mod = require("ws.frame")
local Receiver = require("ws.receiver")
local Sender = require("ws.sender")
local handshake = require("ws.handshake")

local CONNECTING = "CONNECTING"
local OPEN = "OPEN"
local CLOSING = "CLOSING"
local CLOSED = "CLOSED"

local M = {}
M.__index = M
emitter.mixin(M)

M.CONNECTING = CONNECTING
M.OPEN = OPEN
M.CLOSING = CLOSING
M.CLOSED = CLOSED

local function init_fields(self, options)
  emitter.init(self)
  self.ready_state = CONNECTING
  self.protocol = ""
  self.extensions = ""
  self.url = ""
  self._socket = nil
  self._receiver = nil
  self._sender = nil
  self._close_code = 1006
  self._close_reason = ""
  self._close_frame_sent = false
  self._close_frame_received = false
  self._close_deadline = nil
  self._error_emitted = false
  self._extensions = {}
  self._auto_pong = options.auto_pong ~= false
  self._close_timeout = options.close_timeout or frame_mod.CLOSE_TIMEOUT
  self._max_payload = options.max_payload or (100 * 1024 * 1024)
  self._skip_utf8_validation = options.skip_utf8_validation or false
end

function M.new(address, options)
  local self = setmetatable({}, M)
  options = options or {}
  init_fields(self, options)

  self._is_server = false
  self._follow_redirects = options.follow_redirects or false
  self._max_redirects = options.max_redirects or 10
  self._redirects = 0
  self._handshake_timeout = options.handshake_timeout
  self._per_message_deflate = options.per_message_deflate
  self._protocols = options.protocols or {}
  self._headers = options.headers or {}
  self._tls_options = options.tls or {}
  self._origin = options.origin
  self._address = address

  if type(self._protocols) == "string" then
    self._protocols = { self._protocols }
  end

  return self
end

function M._create_from_server(socket, options)
  local self = setmetatable({}, M)
  init_fields(self, options)
  self._socket = socket
  self._is_server = true
  return self
end

function M:_setup_socket(socket, exts)
  self._socket = socket
  self._extensions = exts or {}

  local ext_names = {}
  for k in pairs(self._extensions) do
    ext_names[#ext_names + 1] = k
  end
  self.extensions = table.concat(ext_names, ",")

  self._receiver = Receiver.new({
    is_server = self._is_server,
    max_payload = self._max_payload,
    skip_utf8_validation = self._skip_utf8_validation,
    extensions = self._extensions,
  })

  self._sender = Sender.new(socket, self._extensions, not self._is_server)

  local ws = self
  self._receiver:on("message", function(data, is_binary)
    ws:emit("message", data, is_binary)
  end)

  self._receiver:on("ping", function(data)
    if ws._auto_pong and ws.ready_state == OPEN then
      ws._sender:pong(data)
    end
    ws:emit("ping", data)
  end)

  self._receiver:on("pong", function(data)
    ws:emit("pong", data)
  end)

  self._receiver:on("conclude", function(code, reason)
    ws._close_frame_received = true
    ws._close_code = code
    ws._close_reason = reason
    if code == 1005 then ws:close()
    else ws:close(code, reason) end
  end)

  self._receiver:on("error", function(message, status_code)
    if not ws._error_emitted then
      ws._error_emitted = true
      ws:emit("error", message)
    end
    if ws.ready_state == OPEN then
      ws:close(status_code or 1002)
    end
  end)

  self._sender.onerror = function(err)
    if ws.ready_state == CLOSED then return end
    if ws.ready_state == OPEN then
      ws.ready_state = CLOSING
      ws:_set_close_timer()
    end
    if not ws._error_emitted then
      ws._error_emitted = true
      ws:emit("error", err)
    end
  end

  self.ready_state = OPEN
  self:emit("open")
end

function M:connect()
  if self.ready_state ~= CONNECTING then
    return nil, "already connected or connecting"
  end

  local ok, socket_lib = pcall(require, "socket")
  if not ok then return nil, "luasocket is required" end

  local sock, err = handshake.perform(
    self, self._address, self._protocols, socket_lib)
  if not sock then return nil, err end
  return true
end

function M:send(data, options, cb)
  if type(options) == "function" then
    cb = options
    options = {}
  end
  options = options or {}

  if self.ready_state == CONNECTING then
    error("WebSocket is not open: readyState CONNECTING", 2)
  end
  if self.ready_state ~= OPEN then
    if cb then cb("WebSocket is not open: readyState " .. self.ready_state) end
    return nil, "WebSocket is not open"
  end

  if type(data) == "number" then data = tostring(data) end

  local opts = {
    binary = options.binary ~= nil and options.binary or type(data) ~= "string",
    compress = options.compress ~= false,
    fin = options.fin ~= false,
  }
  if not self._extensions["permessage-deflate"] then
    opts.compress = false
  end

  self._sender:send(data or "", opts, cb)
  return true
end

function M:ping(data, cb)
  if self.ready_state == CONNECTING then
    error("WebSocket is not open: readyState CONNECTING", 2)
  end
  if self.ready_state ~= OPEN then
    if cb then cb("WebSocket is not open") end
    return nil, "WebSocket is not open"
  end
  if type(data) == "number" then data = tostring(data) end
  self._sender:ping(data or "", cb)
  return true
end

function M:pong(data, cb)
  if self.ready_state == CONNECTING then
    error("WebSocket is not open: readyState CONNECTING", 2)
  end
  if self.ready_state ~= OPEN then
    if cb then cb("WebSocket is not open") end
    return nil, "WebSocket is not open"
  end
  if type(data) == "number" then data = tostring(data) end
  self._sender:pong(data or "", cb)
  return true
end

function M:close(code, reason)
  if self.ready_state == CLOSED then return end

  if self.ready_state == CONNECTING then
    self:_abort("WebSocket was closed before the connection was established")
    return
  end

  if self.ready_state == CLOSING then
    if self._close_frame_sent and self._close_frame_received then
      self:_destroy_socket()
    end
    return
  end

  self.ready_state = CLOSING
  self._sender:close(code, reason, function(err)
    if err then return end
    self._close_frame_sent = true
    if self._close_frame_received then
      self:_destroy_socket()
    end
  end)
  self:_set_close_timer()
end

function M:terminate()
  if self.ready_state == CLOSED then return end
  if self.ready_state == CONNECTING then
    self:_abort("WebSocket was closed before the connection was established")
    return
  end
  self.ready_state = CLOSING
  self:_destroy_socket()
end

function M:poll(timeout)
  if self.ready_state ~= OPEN and self.ready_state ~= CLOSING then return end
  if not self._socket then return end

  -- check close deadline
  if self._close_deadline and os.time() >= self._close_deadline then
    self:_destroy_socket()
    return
  end

  local ok, socket_lib = pcall(require, "socket")
  if not ok then return end

  local readable = socket_lib.select({ self._socket }, nil, timeout or 0)
  if not readable or #readable == 0 then return end

  self._socket:settimeout(0)
  while true do
    local data, err, partial = self._socket:receive(8192)
    local chunk = data or partial
    if chunk and #chunk > 0 then
      self._receiver:write(chunk)
    end
    if err then
      if err == "closed" then self:_handle_socket_close() end
      break
    end
  end
end

function M:_handle_socket_close()
  if self.ready_state == CLOSED then return end
  self.ready_state = CLOSING
  self._close_deadline = nil
  self:_emit_close()
end

function M:_destroy_socket()
  if self._socket then
    pcall(function() self._socket:close() end)
  end
  self._close_deadline = nil
  self:_emit_close()
end

function M:_emit_close()
  if self.ready_state == CLOSED then return end
  if self._extensions["permessage-deflate"] then
    self._extensions["permessage-deflate"]:cleanup()
  end
  if self._receiver then
    self._receiver:remove_all_listeners()
  end
  self.ready_state = CLOSED
  self:emit("close", self._close_code, self._close_reason)
end

function M:_abort(message)
  self.ready_state = CLOSING
  if not self._error_emitted then
    self._error_emitted = true
    self:emit("error", message)
  end
  self:_emit_close()
end

function M:_set_close_timer()
  self._close_deadline = os.time() + self._close_timeout
end

return M
