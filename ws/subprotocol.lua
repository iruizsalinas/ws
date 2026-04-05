local validation = require("ws.validation")
local token_chars = validation.token_chars
local byte, sub = string.byte, string.sub

local M = {}

function M.parse(header)
  local protocols = {}
  local seen = {}
  local start = -1
  local stop = -1
  local len = #header

  for i = 1, len do
    local code = byte(header, i)

    if stop == -1 and code <= 127 and token_chars[code] == 1 then
      if start == -1 then start = i end
    elseif i ~= 1 and (code == 0x20 or code == 0x09) then
      if stop == -1 and start ~= -1 then stop = i end
    elseif code == 0x2C then
      if start == -1 then
        error("unexpected character at index " .. i)
      end
      if stop == -1 then stop = i end
      local protocol = sub(header, start, stop - 1)
      if seen[protocol] then
        error("duplicate subprotocol: " .. protocol)
      end
      seen[protocol] = true
      protocols[#protocols + 1] = protocol
      start = -1
      stop = -1
    else
      error("unexpected character at index " .. i)
    end
  end

  if start == -1 or stop ~= -1 then
    error("unexpected end of input")
  end

  local protocol = sub(header, start, len)
  if seen[protocol] then
    error("duplicate subprotocol: " .. protocol)
  end
  protocols[#protocols + 1] = protocol
  return protocols
end

return M
