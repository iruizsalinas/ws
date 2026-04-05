local emitter = require("ws.emitter")
local sha1_mod = require("ws.sha1")
local base64 = require("ws.base64")
local frame_mod = require("ws.frame")
local extension = require("ws.extension")
local subprotocol_mod = require("ws.subprotocol")
local deflate_mod = require("ws.deflate")
local validation = require("ws.validation")
local WebSocket = require("ws.websocket")

local RUNNING = 0
local CLOSING = 1
local CLOSED = 2

local function is_valid_key(key)
  return #key == 24 and key:match("^[A-Za-z0-9+/]+=*$") ~= nil
end

local M = {}
M.__index = M
emitter.mixin(M)

function M.new(options)
  local self = setmetatable({}, M)
  emitter.init(self)

  options = options or {}
  self._host = options.host or "0.0.0.0"
  self._port = options.port
  self._backlog = options.backlog or 511
  self._max_payload = options.max_payload or (100 * 1024 * 1024)
  self._auto_pong = options.auto_pong ~= false
  self._close_timeout = options.close_timeout or frame_mod.CLOSE_TIMEOUT
  self._skip_utf8_validation = options.skip_utf8_validation or false
  self._per_message_deflate = options.per_message_deflate or false
  self._verify_client = options.verify_client
  self._handle_protocols = options.handle_protocols
  self._path = options.path
  self._no_server = options.no_server or false
  self._client_tracking = options.client_tracking ~= false

  self._server = nil
  self._state = RUNNING
  self._socket_lib = nil
  self._connections = {}
  self._socket_map = {}

  if self._client_tracking then
    self.clients = {}
  end

  if self._per_message_deflate == true then
    self._per_message_deflate = {}
  end

  return self
end

function M:_register_connection(ws)
  self._connections[ws] = true
  self._socket_map[ws._socket] = ws

  if self._client_tracking then
    self.clients[ws] = true
  end

  ws:on("close", function()
    self:_unregister_connection(ws)
  end)
end

function M:_unregister_connection(ws)
  self._connections[ws] = nil
  if ws._socket then
    self._socket_map[ws._socket] = nil
  end

  if self._client_tracking then
    self.clients[ws] = nil
  end

  if self._state == CLOSING and not next(self._connections) then
    self._state = CLOSED
    self:emit("close")
  end
end

function M:listen(callback)
  local ok, socket_lib = pcall(require, "socket")
  if not ok then
    error("luasocket is required", 2)
  end
  self._socket_lib = socket_lib

  if self._no_server then
    if callback then callback() end
    self:emit("listening")
    return true
  end

  local server, err = socket_lib.bind(self._host, self._port, self._backlog)
  if not server then
    return nil, "failed to bind: " .. tostring(err)
  end

  server:settimeout(0)
  self._server = server

  local addr, port = server:getsockname()
  self._bound_port = port
  self._bound_address = addr

  if callback then callback() end
  self:emit("listening")
  return true
end

function M:address()
  if self._no_server then
    error("server is operating in noServer mode", 2)
  end
  if not self._server then return nil end
  return {
    address = self._bound_address,
    port = self._bound_port,
  }
end

function M:poll(timeout)
  if self._state ~= RUNNING then return end

  local sockets = {}
  if self._server then
    sockets[#sockets + 1] = self._server
  end

  for sock, ws in pairs(self._socket_map) do
    if ws.ready_state == "OPEN" or ws.ready_state == "CLOSING" then
      sockets[#sockets + 1] = sock
    else
      self._socket_map[sock] = nil
    end
  end

  if #sockets == 0 then return end

  local readable = self._socket_lib.select(sockets, nil, timeout or 0)
  if not readable then return end

  for _, sock in ipairs(readable) do
    if sock == self._server then
      self:_accept_connection()
    else
      self:_read_client(sock)
    end
  end

  -- check close deadlines
  local now = os.time()
  for ws in pairs(self._connections) do
    if ws._close_deadline and now >= ws._close_deadline then
      ws:_destroy_socket()
    end
  end
end

local function contains_protocol(protocols, selected)
  for _, protocol in ipairs(protocols) do
    if protocol == selected then
      return true
    end
  end
  return false
end

function M:_accept_connection()
  local client, err = self._server:accept()
  if not client then return end

  client:settimeout(5)

  local line, lerr = client:receive("*l")
  if not line then
    client:close()
    return
  end

  local method, path = line:match("^(%u+)%s+(%S+)%s+HTTP/%d+%.%d+")
  if not method then
    self:_abort_handshake(client, 400, "Bad Request")
    return
  end

  if method ~= "GET" then
    self:_abort_handshake(client, 405, "Method Not Allowed")
    return
  end

  local headers = {}
  while true do
    local hline, herr = client:receive("*l")
    if not hline or hline == "" then break end
    local name, value = hline:match("^([^:]+):%s*(.*)")
    if name then
      headers[name:lower()] = value
    end
  end

  self:_handle_upgrade(client, method, path, headers)
end

function M:_handle_upgrade(socket, method, path, headers)
  local upgrade = headers["upgrade"]
  if not validation.header_has_token(upgrade, "websocket") then
    self:_abort_handshake(socket, 400, "Invalid Upgrade header")
    return
  end

  local connection = headers["connection"]
  if not validation.header_has_token(connection, "upgrade") then
    self:_abort_handshake(socket, 400, "Invalid Connection header")
    return
  end

  local key = headers["sec-websocket-key"]
  if not key or not is_valid_key(key) or #base64.decode(key) ~= 16 then
    self:_abort_handshake(socket, 400, "Missing or invalid Sec-WebSocket-Key header")
    return
  end

  local version = tonumber(headers["sec-websocket-version"])
  if version ~= 13 and version ~= 8 then
    self:_abort_handshake(socket, 400, "Missing or invalid Sec-WebSocket-Version header",
      { ["Sec-WebSocket-Version"] = "13, 8" })
    return
  end

  if self._path then
    local req_path = path:match("^([^?]*)") or path
    if req_path ~= self._path then
      self:_abort_handshake(socket, 400, "Bad Request")
      return
    end
  end

  -- parse subprotocols
  local protocols = {}
  local sec_protocol = headers["sec-websocket-protocol"]
  if sec_protocol then
    local pok, parsed = pcall(subprotocol_mod.parse, sec_protocol)
    if not pok then
      self:_abort_handshake(socket, 400, "Invalid Sec-WebSocket-Protocol header")
      return
    end
    protocols = parsed
  end

  -- negotiate extensions
  local exts = {}
  local ext_header = headers["sec-websocket-extensions"]
  if self._per_message_deflate and ext_header and deflate_mod.available() then
    local eok, offers = pcall(extension.parse, ext_header)
    if eok and offers[deflate_mod.extension_name] then
      local src_opts = type(self._per_message_deflate) == "table"
                       and self._per_message_deflate or {}
      local deflate_opts = {}
      for k, v in pairs(src_opts) do deflate_opts[k] = v end
      deflate_opts.is_server = true
      deflate_opts.max_payload = self._max_payload
      local deflate = deflate_mod.new(deflate_opts)
      local aok = pcall(deflate.accept, deflate, offers[deflate_mod.extension_name])
      if aok then
        exts[deflate_mod.extension_name] = deflate
      end
    end
  end

  -- verify client
  if self._verify_client then
    local req_info = {
      origin = headers["origin"] or headers["sec-websocket-origin"],
      secure = false,
      headers = headers,
      path = path,
    }
    local verified = self._verify_client(req_info)
    if not verified then
      self:_abort_handshake(socket, 401, "Unauthorized")
      return
    end
  end

  self:_complete_upgrade(socket, key, protocols, headers, path, exts)
end

function M:_complete_upgrade(socket, key, protocols, request_headers, path, exts)
  if self._state ~= RUNNING then
    self:_abort_handshake(socket, 503, "Service Unavailable")
    return
  end

  local digest = base64.encode(sha1_mod.sha1(key .. frame_mod.GUID))

  local response_headers = {
    "HTTP/1.1 101 Switching Protocols",
    "Upgrade: websocket",
    "Connection: Upgrade",
    "Sec-WebSocket-Accept: " .. digest,
  }

  local ws = WebSocket._create_from_server(socket, {
    auto_pong = self._auto_pong,
    close_timeout = self._close_timeout,
    max_payload = self._max_payload,
    skip_utf8_validation = self._skip_utf8_validation,
  })

  -- subprotocol selection
  if #protocols > 0 then
    local selected
    if self._handle_protocols then
      selected = self._handle_protocols(protocols, request_headers)
    else
      selected = protocols[1]
    end
    if selected then
      if not contains_protocol(protocols, selected) then
        self:_abort_handshake(socket, 500, "Invalid subprotocol selection")
        return
      end
      response_headers[#response_headers + 1] =
        "Sec-WebSocket-Protocol: " .. selected
      ws.protocol = selected
    end
  end

  -- extension response
  if exts[deflate_mod.extension_name] then
    local params = exts[deflate_mod.extension_name].params
    local value = extension.format({ [deflate_mod.extension_name] = { params } })
    response_headers[#response_headers + 1] =
      "Sec-WebSocket-Extensions: " .. value
  end

  self:emit("headers", response_headers, request_headers)

  response_headers[#response_headers + 1] = ""
  response_headers[#response_headers + 1] = ""
  local response = table.concat(response_headers, "\r\n")

  local ok, err = socket:send(response)
  if not ok then
    socket:close()
    return
  end

  socket:settimeout(0)
  ws:_setup_socket(socket, exts)
  self:_register_connection(ws)

  self:emit("connection", ws, { headers = request_headers, path = path })
end

function M:handle_upgrade(socket, method, path, headers)
  self:_handle_upgrade(socket, method, path, headers)
end

function M:_read_client(sock)
  local ws = self._socket_map[sock]
  if ws then
    ws:poll(0)
  end
end

function M:_abort_handshake(socket, code, message, extra_headers)
  local status_text = ({
    [400] = "Bad Request",
    [401] = "Unauthorized",
    [403] = "Forbidden",
    [405] = "Method Not Allowed",
    [426] = "Upgrade Required",
    [500] = "Internal Server Error",
    [503] = "Service Unavailable",
  })[code] or "Error"

  local body = message or status_text
  local headers = {
    "HTTP/1.1 " .. code .. " " .. status_text,
    "Connection: close",
    "Content-Type: text/plain",
    "Content-Length: " .. #body,
  }

  if extra_headers then
    for k, v in pairs(extra_headers) do
      headers[#headers + 1] = k .. ": " .. v
    end
  end

  headers[#headers + 1] = ""
  headers[#headers + 1] = body

  pcall(socket.send, socket, table.concat(headers, "\r\n"))
  pcall(socket.close, socket)
end

function M:close(cb)
  if self._state == CLOSED then
    if cb then cb() end
    return
  end

  if self._state == CLOSING then
    if cb then self:once("close", cb) end
    return
  end

  self._state = CLOSING

  if self._server then
    pcall(function() self._server:close() end)
    self._server = nil
  end

  if not next(self._connections) then
    self._state = CLOSED
    if cb then cb() end
    self:emit("close")
    return
  end

  if cb then self:once("close", cb) end

  for ws in pairs(self._connections) do
    ws:close(1001, "server shutting down")
  end
end

return M
