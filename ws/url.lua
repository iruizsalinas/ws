local M = {}

local function has_ctl_or_space(value)
  return type(value) == "string" and value:find("[%z\1-\32\127]") ~= nil
end

function M.parse(url)
  local result = {}

  local protocol, rest = url:match("^(%a[%w+%-.]*)://(.+)$")
  if not protocol then
    return nil, "invalid URL: missing protocol"
  end
  result.protocol = protocol:lower()

  if result.protocol == "http" then
    result.protocol = "ws"
  elseif result.protocol == "https" then
    result.protocol = "wss"
  end

  if result.protocol ~= "ws" and result.protocol ~= "wss" then
    return nil, "invalid protocol: " .. result.protocol
  end

  result.secure = result.protocol == "wss"

  if rest:find("#", 1, true) then
    return nil, "URL contains a fragment identifier"
  end

  -- split authority from path/query
  local split_at = rest:find("[/?]")
  local auth_host, path_query
  if split_at then
    auth_host = rest:sub(1, split_at - 1)
    path_query = rest:sub(split_at)
  else
    auth_host = rest
    path_query = "/"
  end

  -- extract userinfo
  local at
  for i = 1, #auth_host do
    if auth_host:sub(i, i) == "@" then at = i end
  end
  if at then
    local userinfo = auth_host:sub(1, at - 1)
    local hostport = auth_host:sub(at + 1)
    if hostport == "" or has_ctl_or_space(userinfo) then
      return nil, "invalid URL: invalid userinfo"
    end
    local user, pass = userinfo:match("^([^:]*):(.*)$")
    if user then
      result.username = user
      result.password = pass
    else
      result.username = userinfo
    end
    auth_host = hostport
  end

  -- parse host and port
  local host, port
  if auth_host:sub(1, 1) == "[" then
    local bracketed, rest_port = auth_host:match("^%[([^%]]+)%](.*)$")
    if not bracketed or bracketed == "" then
      return nil, "invalid URL: invalid host"
    end
    host = bracketed
    if rest_port ~= "" then
      port = rest_port:match("^:(%d+)$")
      if not port then
        return nil, "invalid URL: invalid port"
      end
    end
  else
    if auth_host:find("[%[%]@]") then
      return nil, "invalid URL: invalid host"
    end
    local first_colon = auth_host:find(":", 1, true)
    if first_colon then
      if auth_host:find(":", first_colon + 1, true) then
        return nil, "invalid URL: invalid host"
      end
      host = auth_host:sub(1, first_colon - 1)
      port = auth_host:sub(first_colon + 1)
      if port == "" or not port:match("^%d+$") then
        return nil, "invalid URL: invalid port"
      end
    else
      host = auth_host
    end
  end

  result.host = host
  result.port = port and tonumber(port) or (result.secure and 443 or 80)
  if not result.host or result.host == "" or has_ctl_or_space(result.host) then
    return nil, "invalid URL: invalid host"
  end
  if not result.port or result.port < 1 or result.port > 65535 or
     result.port ~= math.floor(result.port) then
    return nil, "invalid URL: invalid port"
  end

  -- split path and query
  local query_at = path_query:find("?", 1, true)
  if query_at then
    result.path = path_query:sub(1, query_at - 1)
    result.query = path_query:sub(query_at + 1)
  else
    result.path = path_query
    result.query = nil
  end

  if result.path == "" then
    result.path = "/"
  end
  if has_ctl_or_space(result.path) or
     (result.query and has_ctl_or_space(result.query)) then
    return nil, "invalid URL: invalid request path"
  end

  result.request_path = result.path
  if result.query and result.query ~= "" then
    result.request_path = result.path .. "?" .. result.query
  end

  return result
end

return M
