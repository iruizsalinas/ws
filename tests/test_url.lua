local T = require("helper")
T.init("test_url.lua")

local url = require("ws.url")

-- basic ws
local p = url.parse("ws://host")
T.check("ws basic", p ~= nil)
T.check_equal("ws protocol", p.protocol, "ws")
T.check_equal("ws host", p.host, "host")
T.check_equal("ws port default", p.port, 80)
T.check_equal("ws path default", p.path, "/")
T.check_equal("ws secure", p.secure, false)

-- ws with port
local p2 = url.parse("ws://host:8080")
T.check_equal("ws port", p2.port, 8080)

-- ws with path and query
local p3 = url.parse("ws://host/path/to?query=1&b=2")
T.check_equal("ws path", p3.path, "/path/to")
T.check_equal("ws query", p3.query, "query=1&b=2")
T.check_equal("ws request_path", p3.request_path, "/path/to?query=1&b=2")

-- wss
local p4 = url.parse("wss://host")
T.check_equal("wss protocol", p4.protocol, "wss")
T.check_equal("wss port default", p4.port, 443)
T.check_equal("wss secure", p4.secure, true)

-- wss with non-default port
local p5 = url.parse("wss://host:9443/ws")
T.check_equal("wss port", p5.port, 9443)
T.check_equal("wss path", p5.path, "/ws")

-- http -> ws
local p6 = url.parse("http://host:3000/ws")
T.check_equal("http->ws", p6.protocol, "ws")

-- https -> wss
local p7 = url.parse("https://host/ws")
T.check_equal("https->wss", p7.protocol, "wss")

-- invalid protocol
local p8, e8 = url.parse("ftp://host")
T.check("invalid proto nil", p8 == nil)
T.check("invalid proto err", e8 ~= nil)

-- fragment rejection
local p9, e9 = url.parse("ws://host/path#frag")
T.check("fragment nil", p9 == nil)
T.check("fragment err", e9 and e9:find("fragment"))

-- missing protocol
local p10, e10 = url.parse("host:8080")
T.check("no proto nil", p10 == nil)

-- ip address host
local p11 = url.parse("ws://192.168.1.1:8080/ws")
T.check_equal("ip host", p11.host, "192.168.1.1")
T.check_equal("ip port", p11.port, 8080)

-- path with no query
local p12 = url.parse("ws://host/just-path")
T.check_equal("no query path", p12.path, "/just-path")
T.check_equal("no query request", p12.request_path, "/just-path")

T.finish()
