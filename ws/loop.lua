-- The event loop: waits on the native hub and dispatches to handlers.
local core = require("ws.core")
local timer = require("ws.timer")

local loop = {
  hub = core.hub(),
  sockets = {}, -- id -> socket, until its close event
  servers = {}, -- id -> server, until closed
  handlers = {}, -- event kind -> function(id, data, code, server)
}

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
  local kind, id, data, code, server = core.wait(loop.hub, wait or -1)
  for _ = 1, BATCH do
    if not kind then
      break
    end
    loop.handlers[kind](id, data, code, server)
    kind, id, data, code, server = core.wait(loop.hub, 0)
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

return loop
