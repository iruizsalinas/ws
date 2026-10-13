-- One-shot and repeating timers driven by the event loop.
local core = require("ws.core")

local timer = {}
local timers = {}

local Timer = {}
Timer.__index = Timer

function Timer:cancel()
  self.live = false
end

local function start(delay, fn, every)
  if type(delay) ~= "number" or not (delay >= 0 and delay < math.huge) or (every and delay == 0) then
    error(every and "interval must be a positive number of seconds" or "delay must be a non-negative number of seconds", 3)
  end
  if type(fn) ~= "function" then
    error("callback must be a function", 3)
  end
  local t = setmetatable({ due = core.now() + delay, fn = fn, every = every and delay, live = true }, Timer)
  timers[#timers + 1] = t
  return t
end

-- Calls `fn` once after `delay` seconds.
function timer.after(delay, fn)
  return start(delay, fn, false)
end

-- Calls `fn` every `interval` seconds until cancelled.
function timer.every(interval, fn)
  return start(interval, fn, true)
end

local function prune()
  local live = {}
  for _, t in ipairs(timers) do
    if t.live then live[#live + 1] = t end
  end
  timers = live
end

function timer.pending()
  prune()
  return #timers > 0
end

-- Seconds until the next timer is due, or nil if there is none.
function timer.wait()
  local soonest
  for _, t in ipairs(timers) do
    if t.live and (not soonest or t.due < soonest) then soonest = t.due end
  end
  return soonest and math.max(0, soonest - core.now())
end

-- Runs every timer due now, earliest first.
function timer.fire()
  local now = core.now()
  local due = {}
  for _, t in ipairs(timers) do
    if t.live and t.due <= now then due[#due + 1] = t end
  end
  table.sort(due, function(a, b) return a.due < b.due end)
  for _, t in ipairs(due) do
    if t.live then
      if t.every then
        t.due = core.now() + t.every
      else
        t.live = false
      end
      t.fn()
    end
  end
  prune()
end

return timer
