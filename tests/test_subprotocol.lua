local T = require("helper")
T.init("test_subprotocol.lua")

local sub = require("ws.subprotocol")

-- single protocol
local r1 = sub.parse("graphql-ws")
T.check_equal("single count", #r1, 1)
T.check_equal("single value", r1[1], "graphql-ws")

-- multiple protocols
local r2 = sub.parse("graphql-ws, graphql-transport-ws")
T.check_equal("multi count", #r2, 2)
T.check_equal("multi 1", r2[1], "graphql-ws")
T.check_equal("multi 2", r2[2], "graphql-transport-ws")

-- whitespace around comma
local r3 = sub.parse("proto1 , proto2")
T.check_equal("ws count", #r3, 2)
T.check_equal("ws 1", r3[1], "proto1")
T.check_equal("ws 2", r3[2], "proto2")

-- reject empty
T.check_error("empty", function() sub.parse("") end)

-- reject duplicate
T.check_error("duplicate", function() sub.parse("proto, proto") end)

-- reject leading comma
T.check_error("leading comma", function() sub.parse(",proto") end)

-- reject space in token
T.check_error("space in token", function() sub.parse("bad proto") end)

T.finish()
