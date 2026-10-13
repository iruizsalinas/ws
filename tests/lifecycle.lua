-- A failed cleanup-guard allocation must not leave a native hub behind.
package.path = "./?.lua;./?/init.lua;" .. package.path
local saved = {}
for _, name in ipairs({ "ws.core", "ws.loop", "ws.timer" }) do
  saved[name] = package.loaded[name]
  package.loaded[name] = nil
end
local live = 0
package.loaded["ws.core"] = {
  hub = function(owner)
    live = live + 1
    if owner then owner.hub = live end
    return live
  end,
  shutdown = function() live = live - 1 end,
}
package.loaded["ws.timer"] = {}
local proxy, meta = newproxy, setmetatable
if proxy then
  newproxy = function() error("guard allocation failed") end
else
  setmetatable = function(t, m)
    if m.__gc then error("guard allocation failed") end
    return meta(t, m)
  end
end
local ok, err = pcall(require, "ws.loop")
newproxy, setmetatable = proxy, meta
for _, name in ipairs({ "ws.core", "ws.loop", "ws.timer" }) do
  package.loaded[name] = saved[name]
end
assert(not ok and tostring(err):find("guard allocation failed", 1, true), tostring(err))
assert(live == 0, "failed guard allocation leaked a native hub")
print("loop initialization cleanup regression passed")
