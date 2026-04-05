local T = require("helper")
T.init("test_websocket.lua")

local WebSocket = require("ws.websocket")

-- initial state
local ws = WebSocket.new("ws://localhost:8080")
T.check_equal("initial state", ws.ready_state, "CONNECTING")
T.check_equal("initial protocol", ws.protocol, "")
T.check_equal("initial extensions", ws.extensions, "")

-- send on CONNECTING throws
local sok, serr = pcall(function() ws:send("data") end)
T.check("send CONNECTING throws", not sok)

-- ping on CONNECTING throws
local pok = pcall(function() ws:ping() end)
T.check("ping CONNECTING throws", not pok)

-- pong on CONNECTING throws
local pok2 = pcall(function() ws:pong() end)
T.check("pong CONNECTING throws", not pok2)

-- close on CONNECTING triggers abort
local close_events = {}
ws:on("error", function(e) close_events[#close_events + 1] = "error:" .. e end)
ws:on("close", function(c) close_events[#close_events + 1] = "close:" .. c end)
ws:close()
T.check_equal("close CONNECTING state", ws.ready_state, "CLOSED")
T.check("close CONNECTING has error", #close_events >= 1)

-- close on CLOSED is no-op
ws:close()
T.check_equal("close CLOSED state", ws.ready_state, "CLOSED")

-- terminate on CLOSED is no-op
ws:terminate()
T.check_equal("terminate CLOSED", ws.ready_state, "CLOSED")

-- send on CLOSED returns error
local ws2 = WebSocket.new("ws://localhost:8080")
ws2.ready_state = "CLOSED"
ws2._listeners = {}
local ok2, err2 = ws2:send("data")
T.check("send CLOSED nil", ok2 == nil)
T.check("send CLOSED err", err2 ~= nil)

-- send with callback on CLOSED calls callback with error
local ws3 = WebSocket.new("ws://localhost:8080")
ws3.ready_state = "CLOSED"
ws3._listeners = {}
local cb_err
ws3:send("data", function(e) cb_err = e end)
T.check("send CLOSED cb err", cb_err ~= nil)

-- class constants
T.check_equal("CONNECTING const", WebSocket.CONNECTING, "CONNECTING")
T.check_equal("OPEN const", WebSocket.OPEN, "OPEN")
T.check_equal("CLOSING const", WebSocket.CLOSING, "CLOSING")
T.check_equal("CLOSED const", WebSocket.CLOSED, "CLOSED")

-- new with string protocol
local ws4 = WebSocket.new("ws://host", { protocols = "chat" })
T.check_equal("string protocols", ws4._protocols[1], "chat")

-- new with table protocols
local ws5 = WebSocket.new("ws://host", { protocols = { "a", "b" } })
T.check_equal("table protocols 1", ws5._protocols[1], "a")
T.check_equal("table protocols 2", ws5._protocols[2], "b")

-- default options
local ws6 = WebSocket.new("ws://host")
T.check_equal("default auto_pong", ws6._auto_pong, true)
T.check_equal("default max_payload", ws6._max_payload, 100 * 1024 * 1024)
T.check_equal("default close_timeout", ws6._close_timeout, 30)
T.check_equal("default follow_redirects", ws6._follow_redirects, false)
T.check_equal("default max_redirects", ws6._max_redirects, 10)

-- _create_from_server
local ws7 = WebSocket._create_from_server({}, { auto_pong = false, close_timeout = 10 })
T.check_equal("server auto_pong", ws7._auto_pong, false)
T.check_equal("server close_timeout", ws7._close_timeout, 10)
T.check_equal("server is_server", ws7._is_server, true)

T.finish()
