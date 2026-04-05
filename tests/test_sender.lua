local T = require("helper")
T.init("test_sender.lua")

local Sender = require("ws.sender")

local function make_socket()
  return {
    writes = {},
    send = function(self, data)
      self.writes[#self.writes + 1] = data
      return true
    end,
  }
end

local sock = make_socket()
local sender = Sender.new(sock, {}, false)

T.check_error("close rejects invalid utf8 reason", function()
  sender:close(1000, "\255")
end)

sender:close(1000, "bye")
T.check("valid close frame sent", #sock.writes == 1)

T.finish()
