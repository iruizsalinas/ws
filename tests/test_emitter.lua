local T = require("helper")
T.init("test_emitter.lua")

local Emitter = require("ws.emitter")

-- basic on/emit
local e = Emitter.new()
local called = false
local arg_val = nil
e:on("test", function(v) called = true; arg_val = v end)
e:emit("test", 42)
T.check("on fires", called)
T.check_equal("on passes args", arg_val, 42)

-- emit returns true when listeners exist
T.check("emit returns true", e:emit("test", 1))

-- emit returns false when no listeners
T.check("emit returns false for unknown", not e:emit("nonexistent"))

-- multiple arguments
local a1, a2, a3
e:on("multi", function(x, y, z) a1, a2, a3 = x, y, z end)
e:emit("multi", "a", "b", "c")
T.check_equal("multi arg 1", a1, "a")
T.check_equal("multi arg 2", a2, "b")
T.check_equal("multi arg 3", a3, "c")

-- multiple listeners fire in order
local order = {}
local e2 = Emitter.new()
e2:on("x", function() order[#order + 1] = "first" end)
e2:on("x", function() order[#order + 1] = "second" end)
e2:on("x", function() order[#order + 1] = "third" end)
e2:emit("x")
T.check_equal("order count", #order, 3)
T.check_equal("order 1", order[1], "first")
T.check_equal("order 2", order[2], "second")
T.check_equal("order 3", order[3], "third")

-- once fires only once
local e3 = Emitter.new()
local count = 0
e3:once("y", function() count = count + 1 end)
e3:emit("y")
e3:emit("y")
T.check_equal("once fires once", count, 1)

-- off removes listener
local e4 = Emitter.new()
local c4 = 0
local fn4 = function() c4 = c4 + 1 end
e4:on("z", fn4)
e4:emit("z")
e4:off("z", fn4)
e4:emit("z")
T.check_equal("off removes", c4, 1)

-- off removes once listener by original fn
local e5 = Emitter.new()
local c5 = 0
local fn5 = function() c5 = c5 + 1 end
e5:once("w", fn5)
e5:off("w", fn5)
e5:emit("w")
T.check_equal("off removes once", c5, 0)

-- listener_count
local e6 = Emitter.new()
T.check_equal("count 0", e6:listener_count("a"), 0)
e6:on("a", function() end)
T.check_equal("count 1", e6:listener_count("a"), 1)
e6:on("a", function() end)
T.check_equal("count 2", e6:listener_count("a"), 2)

-- remove_all_listeners for specific event
local e7 = Emitter.new()
e7:on("a", function() end)
e7:on("b", function() end)
e7:remove_all_listeners("a")
T.check_equal("remove_all a", e7:listener_count("a"), 0)
T.check_equal("remove_all keeps b", e7:listener_count("b"), 1)

-- remove_all_listeners for all events
e7:remove_all_listeners()
T.check_equal("remove_all all", e7:listener_count("b"), 0)

-- listener removes itself during emission (must not skip next)
local e8 = Emitter.new()
local results = {}
local self_remover
self_remover = function()
  results[#results + 1] = "self"
  e8:off("ev", self_remover)
end
e8:on("ev", self_remover)
e8:on("ev", function() results[#results + 1] = "next" end)
e8:emit("ev")
T.check_equal("self-remove count", #results, 2)
T.check_equal("self-remove 1", results[1], "self")
T.check_equal("self-remove 2", results[2], "next")

-- emit with no listeners does not error
local e9 = Emitter.new()
local ok = pcall(function() e9:emit("nothing") end)
T.check("emit no listeners no error", ok)

T.finish()
