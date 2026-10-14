-- The event loop: waits on the native hub and dispatches to handlers.

-- ws.core assumes double numbers and 64-bit integers, Lua's defaults. Checked
-- here first: native code can crash on other number types.
assert(2 ^ 53 + 1 == 2 ^ 53 and 2 ^ 24 + 1 ~= 2 ^ 24 and (math.maxinteger or 2 ^ 63) > 2 ^ 62,
  "ws needs a Lua with 64-bit integers and double-precision numbers")
local core = require("ws.core")
-- On Windows, a ws.core that cannot use this Lua runtime returns nothing.
assert(type(core) == "table", "ws.core is not compatible with this Lua runtime")
local timer = require("ws.timer")

local loop = {
  hub = false, -- reserved before allocating the native hub
  sockets = {}, -- id -> socket, until its close event
  servers = {}, -- id -> server, until closed
  handlers = {}, -- event kind -> function(id, data, code, server)
  -- Event kinds as ws.core numbers them.
  kinds = { "open", "message", "ping", "pong", "error", "close", "request", "connection", "drain" },
}

-- When this Lua state closes, its sockets and servers end with it instead
-- of running on in the host process.
local function shutdown()
  if loop.hub then core.shutdown(loop.hub) end
end
if newproxy then -- Lua 5.1 and LuaJIT finalize only userdata
  loop.guard = newproxy(true)
  getmetatable(loop.guard).__gc = shutdown
else
  loop.guard = setmetatable({}, { __gc = shutdown })
end

local running = false
-- Events handled per wait, so timers are never starved.
local BATCH = 256

local function active()
  return next(loop.sockets) ~= nil or next(loop.servers) ~= nil or timer.pending()
end

-- Handles events for up to `limit` seconds (until something is due if nil).
local function step(limit)
  timer.fire()
  if not active() then
    return false
  end
  local wait = timer.wait()
  if limit and (not wait or wait > limit) then
    wait = limit
  end
  for i = 1, BATCH do
    local kind, id, data, code, server = core.wait(loop.hub, i == 1 and (wait or -1) or 0)
    if not kind then
      break
    end
    loop.handlers[loop.kinds[kind]](id, data, code, server)
  end
  timer.fire()
  return active()
end

local function guarded(f, ...)
  if running then
    error("the event loop is already running", 3)
  end
  running = true
  local ok, result = pcall(f, ...)
  running = false
  if not ok then
    error(result, 0)
  end
  return result
end

-- Runs until no sockets, servers or timers remain.
function loop.run()
  guarded(function()
    while step() do
    end
  end)
end

-- Handles what is ready within `timeout` seconds (default 0) and returns
-- whether anything is still active. For embedding in another loop.
function loop.poll(timeout)
  if timeout ~= nil and (type(timeout) ~= "number" or not (timeout >= 0)) then
    error("timeout must be a non-negative number", 2)
  end
  return guarded(step, timeout or 0)
end

-- Turns a native `nil, err, fatal` into a raised error when fatal,
-- blaming the caller of the public method.
function loop.check(ok, err, fatal)
  if fatal then
    error(err, 3)
  end
  return ok, err
end

-- All Lua allocations and the cleanup guard precede the native resource.
-- C stores the id in the existing slot before returning, too.
loop.hub = core.hub(loop)
return loop
