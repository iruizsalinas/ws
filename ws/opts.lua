-- Option tables: unknown keys and wrong types are errors, not surprises.
local opts = {}

local function integer(v, min, max)
  return type(v) == "number" and v % 1 == 0 and v >= min and v <= max
end

-- Each kind returns the value to use, or nil and what was expected.
local kinds = {
  string = function(v)
    if type(v) == "string" then return v end
    return nil, "a string"
  end,
  ["function"] = function(v)
    if type(v) == "function" then return v end
    return nil, "a function"
  end,
  seconds = function(v)
    if type(v) == "number" and v > 0 and v < math.huge then return v end
    return nil, "a positive number of seconds"
  end,
  size = function(v)
    if integer(v, 1, 2 ^ 53) then return v end
    return nil, "a positive integer"
  end,
  port = function(v)
    if integer(v, 0, 65535) then return v end
    return nil, "an integer from 0 to 65535"
  end,
  -- A string or an array of strings, as an array.
  list = function(v)
    if type(v) == "string" then v = { v } end
    if type(v) ~= "table" then return nil, "a string or an array of strings" end
    local out, seen = {}, {}
    for _, item in ipairs(v) do
      if type(item) ~= "string" or not item:match("^[A-Za-z0-9!#$%%&'*+%.%^_`|~%-]+$") or seen[item] then
        return nil, "an array of distinct nonempty tokens"
      end
      seen[item] = true
      out[#out + 1] = item
    end
    for key in pairs(v) do
      if not integer(key, 1, #out) then return nil, "a dense array of tokens" end
    end
    return out
  end,
  -- A table of names to values, as "name:value" lines.
  headers = function(v)
    if type(v) ~= "table" then return nil, "a table of strings" end
    local lines = {}
    for name, value in pairs(v) do
      if type(name) ~= "string" or type(value) ~= "string" or name:find("[%c:]") or value:find("[\r\n%z]") then
        return nil, "a table of valid header names and values"
      end
      lines[#lines + 1] = name .. ":" .. value .. "\n"
    end
    table.sort(lines)
    return table.concat(lines)
  end,
}

-- Validates `options` against `spec` (key -> kind, or a nested spec table);
-- errors blame `level` and name nested options by path, like `limits.message`.
function opts.check(options, spec, level, path)
  if options == nil then
    return {}
  end
  if type(options) ~= "table" then
    error(path and ("option '" .. path .. "' must be a table") or "options must be a table", level + 1)
  end
  local out = {}
  for key, value in pairs(options) do
    local kind = spec[key]
    local name = path and (path .. "." .. tostring(key)) or tostring(key)
    if not kind then
      error("unknown option '" .. name .. "'", level + 1)
    end
    if type(kind) == "table" then
      out[key] = opts.check(value, kind, level + 1, name)
    else
      local v, expected = kinds[kind](value)
      if v == nil then
        error("option '" .. name .. "' must be " .. expected, level + 1)
      end
      out[key] = v
    end
  end
  return out
end

return opts
