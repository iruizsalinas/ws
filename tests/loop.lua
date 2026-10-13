-- Deterministic queue test; no native module required.
package.path = "./?.lua;./?/init.lua;" .. package.path
local queued, taken, delivered = 513, 0, 0
local saved = {}
for _, name in ipairs({ "ws.core", "ws.loop", "ws.timer" }) do
  saved[name] = package.loaded[name]
  package.loaded[name] = nil
end
package.loaded["ws.core"] = {
  hub = function() return 1 end,
  now = function() return 0 end,
  wait = function()
    if taken < queued then
      taken = taken + 1
      return "message", taken
    end
  end,
}
local ok, err = pcall(function()
  local loop = require("ws.loop")
  loop.sockets[1] = true
  loop.handlers.message = function(id)
    delivered = delivered + 1
    assert(id == delivered, "event lost at batch boundary")
    if delivered == queued then loop.sockets[1] = nil end
  end
  loop.poll(0)
  assert(delivered == 256 and taken == 256, "batch consumed an undispatched event")
  loop.poll(0)
  assert(delivered == 512 and taken == 512)
  assert(loop.poll(0) == false)
  assert(delivered == queued)
end)
for _, name in ipairs({ "ws.core", "ws.loop", "ws.timer" }) do
  package.loaded[name] = saved[name]
end
assert(ok, err)
print("loop batch regression passed")
