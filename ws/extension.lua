local validation = require("ws.validation")
local token_chars = validation.token_chars
local byte, sub = string.byte, string.sub

local M = {}

local function push(dest, name, elem)
  if not dest[name] then
    dest[name] = { elem }
  else
    local list = dest[name]
    list[#list + 1] = elem
  end
end

function M.parse(header)
  local offers = {}
  local params = {}
  local must_unescape = false
  local is_escaping = false
  local in_quotes = false
  local extension_name
  local param_name
  local start = -1
  local code = -1
  local stop = -1
  local len = #header

  for i = 1, len do
    code = byte(header, i)

    if not extension_name then
      if stop == -1 and code <= 127 and token_chars[code] == 1 then
        if start == -1 then start = i end
      elseif i ~= 1 and (code == 0x20 or code == 0x09) then
        if stop == -1 and start ~= -1 then stop = i end
      elseif code == 0x3B or code == 0x2C then
        if start == -1 then
          error("unexpected character at index " .. i)
        end
        if stop == -1 then stop = i end
        local name = sub(header, start, stop - 1)
        if code == 0x2C then
          push(offers, name, params)
          params = {}
        else
          extension_name = name
        end
        start = -1
        stop = -1
      else
        error("unexpected character at index " .. i)
      end
    elseif not param_name then
      if stop == -1 and code <= 127 and token_chars[code] == 1 then
        if start == -1 then start = i end
      elseif code == 0x20 or code == 0x09 then
        if stop == -1 and start ~= -1 then stop = i end
      elseif code == 0x3B or code == 0x2C then
        if start == -1 then
          error("unexpected character at index " .. i)
        end
        if stop == -1 then stop = i end
        push(params, sub(header, start, stop - 1), true)
        if code == 0x2C then
          push(offers, extension_name, params)
          params = {}
          extension_name = nil
        end
        start = -1
        stop = -1
      elseif code == 0x3D and start ~= -1 and stop == -1 then
        param_name = sub(header, start, i - 1)
        start = -1
        stop = -1
      else
        error("unexpected character at index " .. i)
      end
    else
      if is_escaping then
        if code > 127 or token_chars[code] ~= 1 then
          error("unexpected character at index " .. i)
        end
        if start == -1 then
          start = i
        elseif not must_unescape then
          must_unescape = true
        end
        is_escaping = false
      elseif in_quotes then
        if code <= 127 and token_chars[code] == 1 then
          if start == -1 then start = i end
        elseif code == 0x22 and start ~= -1 then
          in_quotes = false
          stop = i
        elseif code == 0x5C then
          is_escaping = true
        else
          error("unexpected character at index " .. i)
        end
      elseif code == 0x22 and byte(header, i - 1) == 0x3D then
        in_quotes = true
      elseif stop == -1 and code <= 127 and token_chars[code] == 1 then
        if start == -1 then start = i end
      elseif start ~= -1 and (code == 0x20 or code == 0x09) then
        if stop == -1 then stop = i end
      elseif code == 0x3B or code == 0x2C then
        if start == -1 then
          error("unexpected character at index " .. i)
        end
        if stop == -1 then stop = i end
        local value = sub(header, start, stop - 1)
        if must_unescape then
          value = value:gsub("\\", "")
          must_unescape = false
        end
        push(params, param_name, value)
        if code == 0x2C then
          push(offers, extension_name, params)
          params = {}
          extension_name = nil
        end
        param_name = nil
        start = -1
        stop = -1
      else
        error("unexpected character at index " .. i)
      end
    end
  end

  if start == -1 or in_quotes or code == 0x20 or code == 0x09 then
    error("unexpected end of input")
  end

  if stop == -1 then stop = len + 1 end
  local token = sub(header, start, stop - 1)

  if not extension_name then
    push(offers, token, params)
  else
    if not param_name then
      push(params, token, true)
    elseif must_unescape then
      push(params, param_name, token:gsub("\\", ""))
    else
      push(params, param_name, token)
    end
    push(offers, extension_name, params)
  end

  return offers
end

function M.format(extensions)
  local parts = {}
  for name, configs in pairs(extensions) do
    if type(configs) ~= "table" or #configs == 0 then
      configs = { configs }
    end
    for _, params in ipairs(configs) do
      local entry = { name }
      if type(params) == "table" then
        for k, v in pairs(params) do
          if type(k) == "string" then
            if v == true then
              entry[#entry + 1] = k
            else
              entry[#entry + 1] = k .. "=" .. tostring(v)
            end
          end
        end
      end
      parts[#parts + 1] = table.concat(entry, "; ")
    end
  end
  return table.concat(parts, ", ")
end

return M
