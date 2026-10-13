-- Event listeners for sockets and servers.
local emitter = {}

-- Registers `fn` for `name`, which must be one of `events`.
function emitter.on(self, events, name, fn)
  if not events[name] then
    error("unknown event '" .. tostring(name) .. "'", 3)
  end
  if type(fn) ~= "function" then
    error("listener must be a function", 3)
  end
  local list = self._on[name] or {}
  list[#list + 1] = fn
  self._on[name] = list
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
