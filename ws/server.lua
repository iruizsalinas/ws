-- Servers: `ws.listen`, upgrade decisions and accepted connections.
local core = require("ws.core")
local emitter = require("ws.emitter")
local loop = require("ws.loop")
local opts = require("ws.opts")
local Socket = require("ws.socket")

local EVENTS = { connection = true }
local OPTIONS = {
  port = "port", host = "string", protocols = "list", accept = "function", keepalive = "keepalive",
  tls = { cert = "string", key = "string" },
  limits = { message = "size", buffer = "size", connections = "size" },
}

local Server = {}
Server.__index = Server
Server.__tostring = function(s) return "ws.server (port " .. s.port .. ")" end

-- `ws.listen(port)` or `ws.listen{...}`; raises if the port cannot be bound.
function Server.listen(arg)
  local o = opts.check(type(arg) == "table" and arg or { port = arg }, OPTIONS, 2)
  if o.port == nil then
    error("option 'port' is required", 2)
  end
  local limits, tls = o.limits or {}, o.tls or {}
  if o.tls and not (tls.cert and tls.key) then
    error("options 'tls.cert' and 'tls.key' go together", 2)
  end
  local id, err = core.listen(loop.hub, o.host or "0.0.0.0", o.port, tls.cert, tls.key,
    limits.message, limits.buffer, limits.connections, o.keepalive)
  if not id then
    error(err, 2)
  end
  local protocols = {}
  for _, p in ipairs(o.protocols or {}) do
    protocols[p] = true
  end
  local s = setmetatable({ id = id, port = core.port(id), accept = o.accept, protocols = protocols, _on = {} }, Server)
  loop.servers[id] = s
  return s
end

function Server:on(name, fn)
  emitter.on(self, EVENTS, name, fn)
  return self
end

function Server:once(name, fn)
  emitter.once(self, EVENTS, name, fn)
  return self
end

function Server:off(name, fn)
  emitter.off(self, EVENTS, name, fn)
  return self
end

-- This server's sockets, oldest first, from `connection` until `close`.
function Server:clients()
  local list = {}
  for _, s in pairs(loop.sockets) do
    if s.server == self then list[#list + 1] = s end
  end
  table.sort(list, function(a, b) return a.id < b.id end)
  return list
end

-- Stops listening and closes this server's sockets with 1001 (going away).
function Server:close()
  if loop.servers[self.id] ~= self then return end
  loop.servers[self.id] = nil
  self._on = {}
  core.unlisten(self.id)
  for id, s in pairs(loop.sockets) do
    if s.server == self then core.close(id, 1001) end
  end
end

-- path, address, port, protocols, then "name:value" header lines.
local function request(data)
  local lines = {}
  for line in data:gmatch("([^\n]*)\n") do
    lines[#lines + 1] = line
  end
  local req = { path = lines[1], address = lines[2], port = tonumber(lines[3]), protocols = {}, headers = {} }
  for p in lines[4]:gmatch("[^,]+") do
    req.protocols[#req.protocols + 1] = p
  end
  for i = 5, #lines do
    local name, value = lines[i]:match("^([^:]+):(.*)$")
    local prev = req.headers[name]
    req.headers[name] = prev and (prev .. ", " .. value) or value
  end
  return req
end

-- Decides an upgrade: `accept(req)` returns true, or false and a status.
function loop.handlers.request(token, data, _, server)
  local s = loop.servers[server]
  if not s then return core.reply(token, 503) end
  local req = request(data)
  if s.accept then
    local ok, accepted, status = pcall(s.accept, req)
    if not ok then
      core.reply(token, 500)
      error(accepted, 0)
    end
    if not accepted then
      status = status or 403
      if type(status) ~= "number" or status % 1 ~= 0 or status < 400 or status > 599 then
        core.reply(token, 500)
        error("accept: status must be an integer from 400 to 599", 0)
      end
      return core.reply(token, status)
    end
  end
  -- The application may have closed the listener from its accept callback.
  if loop.servers[server] ~= s then return core.reply(token, 503) end
  for _, p in ipairs(req.protocols) do
    if s.protocols[p] then return core.reply(token, 0, p) end
  end
  core.reply(token, 0)
end

function loop.handlers.connection(id, data, _, server)
  local s = loop.servers[server]
  if not s then return core.close(id, 1001) end
  emitter.emit(s, "connection", Socket.new(id, nil, s), request(data))
end

return Server
