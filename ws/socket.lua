-- Sockets: `ws.connect` and the connections a server accepts.
local core = require("ws.core")
local emitter = require("ws.emitter")
local loop = require("ws.loop")
local opts = require("ws.opts")

local EVENTS = {
  open = true, message = true, ping = true, pong = true, drain = true, close = true, error = true,
}
local OPTIONS = {
  protocols = "list", headers = "headers", timeout = "seconds", keepalive = "keepalive",
  tls = { ca = "string" },
  limits = { message = "size", buffer = "size" },
}

local Socket = {}

-- Network threads move the native state on; this one follows the events
-- delivered so far, as in browsers ("open" and "closed" with their events).
local function state(s)
  if loop.sockets[s.id] ~= s then return "closed" end
  local native = core.state(s.id)
  if native == "closed" then return "closing" end
  if native == "open" and not s._opened then return "connecting" end
  return native
end

local getters = {
  state = state,
  protocol = function(s) return core.protocol(s.id) end,
  buffered = function(s) return core.buffered(s.id) end,
}

local meta = {
  __index = function(s, key)
    local get = getters[key]
    if get then return get(s) end
    return Socket[key]
  end,
  __tostring = function(s) return "ws.socket (" .. state(s) .. ")" end,
}

-- `server` is the accepting server, or nil for clients.
function Socket.new(id, url, server)
  -- Accepted sockets are open by their connection event.
  local s = setmetatable({ id = id, url = url, server = server, _opened = server ~= nil, _on = {} }, meta)
  loop.sockets[id] = s
  return s
end

function Socket.connect(url, options)
  if type(url) ~= "string" then
    error("url must be a string", 2)
  end
  local o = opts.check(options, OPTIONS, 2)
  local tls = o.tls or {}
  if o.tls and not url:lower():find("^wss://") then
    error("option 'tls' needs a wss:// URL", 2)
  end
  local protocols = o.protocols and table.concat(o.protocols, "\n")
  local limits = o.limits or {}
  local id, err = core.connect(loop.hub, url, protocols, o.headers, tls.ca, o.timeout,
    limits.message, limits.buffer, o.keepalive)
  if not id then
    error(err, 2)
  end
  return Socket.new(id, url)
end

function Socket:on(name, fn)
  emitter.on(self, EVENTS, name, fn)
  return self
end

function Socket:once(name, fn)
  emitter.once(self, EVENTS, name, fn)
  return self
end

function Socket:off(name, fn)
  emitter.off(self, EVENTS, name, fn)
  return self
end

-- Text by default; pass `true` to send binary.
function Socket:send(data, binary)
  if type(data) ~= "string" then
    error("data must be a string", 2)
  end
  return loop.check(core.send(self.id, data, binary))
end

function Socket:ping(data)
  if data ~= nil and type(data) ~= "string" then
    error("data must be a string", 2)
  end
  return loop.check(core.ping(self.id, data))
end

function Socket:close(code, reason)
  if code ~= nil and (type(code) ~= "number" or code % 1 ~= 0 or code < 0 or code > 65535) then
    error("invalid close code", 2)
  end
  if reason ~= nil and type(reason) ~= "string" then
    error("reason must be a string", 2)
  end
  loop.check(core.close(self.id, code, reason))
end

function Socket:terminate()
  core.terminate(self.id)
end

local function emit(name)
  return function(id, ...)
    local s = loop.sockets[id]
    if s then emitter.emit(s, name, ...) end
  end
end

local h = loop.handlers
function h.open(id)
  local s = loop.sockets[id]
  if s then s._opened = true; emitter.emit(s, "open") end
end
h.ping = emit("ping")
h.pong = emit("pong")
h.drain = emit("drain")

function h.message(id, data, binary)
  local s = loop.sockets[id]
  if s then emitter.emit(s, "message", data, binary == 1) end
end

-- Unhandled client errors are raised from ws.run(), as in Node.js; errors
-- on accepted sockets are not, so one bad peer cannot stop a server.
function h.error(id, message)
  local s = loop.sockets[id]
  if s and not emitter.emit(s, "error", message) and not s.server then
    error(message, 0)
  end
end

function h.close(id, reason, code)
  local s = loop.sockets[id]
  if not s then return end
  loop.sockets[id] = nil
  local listeners = s._on
  s._on = {}
  emitter.emit({ _on = listeners }, "close", code, reason)
end

return Socket
