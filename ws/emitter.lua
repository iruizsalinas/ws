-- Event listeners for sockets and servers.
local emitter = {}

-- What `once` wrappers stand for, so `off` also takes the original.
local wrapped = setmetatable({}, { __mode = "k" })

-- Errors blame the caller of the public `:on`, `:once` or `:off`.
local function check(events, name, fn)
  if not events[name] then
    error("unknown event '" .. tostring(name) .. "'", 4)
  end
  if type(fn) ~= "function" then
    error("listener must be a function", 4)
  end
end

local function add(self, name, fn)
  local list = self._on[name] or {}
  list[#list + 1] = fn
  self._on[name] = list
end

-- Registers `fn` for `name`, which must be one of `events`.
function emitter.on(self, events, name, fn)
  check(events, name, fn)
  add(self, name, fn)
end

-- Registers `fn` for the next `name` event only.
function emitter.once(self, events, name, fn)
  check(events, name, fn)
  local function wrapper(...)
    emitter.remove(self, name, wrapper)
    return fn(...)
  end
  wrapped[wrapper] = fn
  add(self, name, wrapper)
end

-- Removes the most recently added `fn` for `name`, as in Node.js.
function emitter.remove(self, name, fn)
  local list = self._on[name] or {}
  for i = #list, 1, -1 do
    if list[i] == fn or wrapped[list[i]] == fn then
      table.remove(list, i)
      return
    end
  end
end

function emitter.off(self, events, name, fn)
  check(events, name, fn)
  emitter.remove(self, name, fn)
end

-- Calls the listeners for `name`; returns whether there were any.
function emitter.emit(self, name, ...)
  local list = self._on[name]
  if not list or #list == 0 then
    return false
  end
  local snapshot = {}
  for i, fn in ipairs(list) do
    snapshot[i] = fn
  end
  for _, fn in ipairs(snapshot) do
    fn(...)
  end
  return true
end

return emitter
