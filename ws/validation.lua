local M = {}

-- rfc 7230 token characters
-- '!', '#', '$', '%', '&', '\'', '*', '+', '-', '.', 0-9, A-Z, '^', '_', '`', a-z, '|', '~'
M.token_chars = {
  [0]=0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  -- 0-15
  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,      -- 16-31
  0,1,0,1,1,1,1,1,0,0,1,1,0,1,1,0,      -- 32-47
  1,1,1,1,1,1,1,1,1,1,0,0,0,0,0,0,      -- 48-63
  0,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,      -- 64-79
  1,1,1,1,1,1,1,1,1,1,1,0,0,0,1,1,      -- 80-95
  1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,      -- 96-111
  1,1,1,1,1,1,1,1,1,1,1,0,1,0,1,0       -- 112-127
}

function M.header_has_token(value, expected)
  if type(value) ~= "string" or type(expected) ~= "string" then
    return false
  end

  expected = expected:lower()
  local start = 1
  local len = #value

  while start <= len do
    while start <= len do
      local code = value:byte(start)
      if code == 0x20 or code == 0x09 or code == 0x2C then
        start = start + 1
      else
        break
      end
    end

    if start > len then
      break
    end

    local stop = start
    while stop <= len do
      local code = value:byte(stop)
      if code == 0x20 or code == 0x09 or code == 0x2C then
        break
      end
      stop = stop + 1
    end

    local token_end = stop - 1
    while token_end >= start do
      local code = value:byte(token_end)
      if code == 0x20 or code == 0x09 then
        token_end = token_end - 1
      else
        break
      end
    end

    if token_end >= start and value:sub(start, token_end):lower() == expected then
      return true
    end

    start = stop + 1
  end

  return false
end

function M.is_valid_status_code(code)
  return (code >= 1000 and code <= 1014 and
          code ~= 1004 and code ~= 1005 and code ~= 1006) or
         (code >= 3000 and code <= 4999)
end

return M
