local M = {}
M.__index = M

-- weak-keyed table to map once wrappers to their originals
local once_map = setmetatable({}, { __mode = "k" })

function M.new()
  return setmetatable({ _listeners = {} }, M)
end

function M:on(event, fn)
  if type(fn) ~= "function" then
    error("listener must be a function", 2)
  end
  local list = self._listeners[event]
  if not list then
    list = {}
    self._listeners[event] = list
  end
  list[#list + 1] = fn
  return self
end

function M:once(event, fn)
  if type(fn) ~= "function" then
    error("listener must be a function", 2)
  end
  local function wrapper(...)
    self:off(event, wrapper)
    fn(...)
  end
  once_map[wrapper] = fn
  return self:on(event, wrapper)
end

function M:off(event, fn)
  local list = self._listeners[event]
  if not list then return self end
  for i = #list, 1, -1 do
    if list[i] == fn or once_map[list[i]] == fn then
      table.remove(list, i)
    end
  end
  return self
end

function M:emit(event, ...)
  local list = self._listeners[event]
  if not list or #list == 0 then return false end
  local snapshot = {}
  for i = 1, #list do snapshot[i] = list[i] end
  for i = 1, #snapshot do
    snapshot[i](...)
  end
  return true
end

function M:listener_count(event)
  local list = self._listeners[event]
  return list and #list or 0
end

function M:remove_all_listeners(event)
  if event then
    self._listeners[event] = nil
  else
    self._listeners = {}
  end
  return self
end

function M.mixin(target)
  target.on = M.on
  target.once = M.once
  target.off = M.off
  target.emit = M.emit
  target.listener_count = M.listener_count
  target.remove_all_listeners = M.remove_all_listeners
  return target
end

function M.init(self)
  self._listeners = {}
end

return M
