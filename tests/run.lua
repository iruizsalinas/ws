-- Runs the Lua test suite against the built module in ws/.
--   lua tests/run.lua
package.path = "./?.lua;./?/init.lua;" .. package.path
package.cpath = "./?.so;./?.dll;" .. package.cpath

dofile("tests/loop.lua")
local H = require("tests.helper")
for _, name in ipairs({ "echo", "close", "server", "errors", "timers", "tls", "keepalive" }) do
  require("tests." .. name)
end
os.exit(H.report() and 0 or 1)
