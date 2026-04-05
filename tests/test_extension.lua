local T = require("helper")
T.init("test_extension.lua")

local ext = require("ws.extension")

-- simple extension
local r1 = ext.parse("permessage-deflate")
T.check("basic name", r1["permessage-deflate"] ~= nil)
T.check_equal("basic count", #r1["permessage-deflate"], 1)

-- extension with boolean param
local r2 = ext.parse("permessage-deflate; server_no_context_takeover")
local p2 = r2["permessage-deflate"][1]
T.check("bool param exists", p2.server_no_context_takeover ~= nil)

-- extension with value param
local r3 = ext.parse("permessage-deflate; client_max_window_bits=15")
local p3 = r3["permessage-deflate"][1]
T.check("value param", p3.client_max_window_bits ~= nil)
T.check_equal("value param val", p3.client_max_window_bits[1], "15")

-- multiple params
local r4 = ext.parse("permessage-deflate; server_no_context_takeover; client_max_window_bits=15")
local p4 = r4["permessage-deflate"][1]
T.check("multi param 1", p4.server_no_context_takeover ~= nil)
T.check("multi param 2", p4.client_max_window_bits ~= nil)

-- multiple extensions
local r5 = ext.parse("ext1, ext2")
T.check("multi ext 1", r5["ext1"] ~= nil)
T.check("multi ext 2", r5["ext2"] ~= nil)

-- multiple offers of same extension
local r6 = ext.parse("permessage-deflate; server_no_context_takeover, permessage-deflate")
T.check_equal("multi offer count", #r6["permessage-deflate"], 2)

-- quoted string value
local r7 = ext.parse('ext; param="value"')
local p7 = r7["ext"][1]
T.check_equal("quoted value", p7.param[1], "value")

-- format
local formatted = ext.format({
  ["permessage-deflate"] = {{ server_no_context_takeover = true }}
})
T.check("format contains name", formatted:find("permessage%-deflate") ~= nil)
T.check("format contains param", formatted:find("server_no_context_takeover") ~= nil)

-- format with value
local formatted2 = ext.format({
  ["permessage-deflate"] = {{ client_max_window_bits = 15 }}
})
T.check("format value", formatted2:find("client_max_window_bits=15") ~= nil)

-- malformed: empty
T.check_error("empty string", function() ext.parse("") end)

-- malformed: just semicolon
T.check_error("just semicolon", function() ext.parse(";") end)

-- malformed: trailing comma
T.check_error("trailing comma", function() ext.parse("ext,") end)

T.finish()
