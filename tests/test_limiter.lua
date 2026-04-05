local T = require("helper")
T.init("test_limiter.lua")

local Limiter = require("ws.limiter")

-- sequential execution with concurrency=1
local lim1 = Limiter.new(1)
local order1 = {}
local done_fns = {}
lim1:add(function(done) order1[#order1 + 1] = "a"; done_fns[1] = done; done() end)
lim1:add(function(done) order1[#order1 + 1] = "b"; done() end)
lim1:add(function(done) order1[#order1 + 1] = "c"; done() end)
T.check_equal("seq count", #order1, 3)
T.check_equal("seq 1", order1[1], "a")
T.check_equal("seq 2", order1[2], "b")
T.check_equal("seq 3", order1[3], "c")

-- infinite concurrency
local lim2 = Limiter.new()
local order2 = {}
lim2:add(function(done) order2[#order2 + 1] = "x"; done() end)
lim2:add(function(done) order2[#order2 + 1] = "y"; done() end)
T.check_equal("inf count", #order2, 2)

-- deferred done (concurrency=1, second job waits)
local lim3 = Limiter.new(1)
local order3 = {}
local deferred_done
lim3:add(function(done)
  order3[#order3 + 1] = "first"
  deferred_done = done
end)
lim3:add(function(done)
  order3[#order3 + 1] = "second"
  done()
end)
T.check_equal("deferred: only first ran", #order3, 1)
deferred_done()
T.check_equal("deferred: both ran", #order3, 2)
T.check_equal("deferred order 2", order3[2], "second")

-- empty add
local lim4 = Limiter.new(5)
T.check_equal("empty pending", lim4.pending, 0)

T.finish()
