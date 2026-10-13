local H = require("tests.helper")
local core = require("ws.core")

H.test("hub ownership survives a failing Lua return hook", function()
  local owner = { hub = false }
  local function allocate()
    local id = core.hub(owner)
    return id
  end
  debug.sethook(function(event)
    if event == "return" and debug.getinfo(2, "f").func == allocate then
      debug.sethook()
      error("hub return interrupted")
    end
  end, "r")
  local ok, err = pcall(allocate)
  debug.sethook()
  local recorded = owner.hub
  if recorded then core.shutdown(recorded) end
  assert(not ok and tostring(err):find("hub return interrupted", 1, true), tostring(err))
  assert(recorded, "native hub was lost before ownership was recorded")
end)

H.test("a hub owner must reserve an unused slot", function()
  for _, owner in ipairs({ {}, { hub = 1 }, true }) do
    H.raises(function() core.hub(owner) end, "invalid hub owner")
  end
end)
