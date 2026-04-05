local M = {}

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

  -- split auth from host
  local auth_host, path_query = rest:match("^([^/]*)(/.*)$")
  if not auth_host then
    auth_host = rest
    path_query = "/"
  end

  -- extract userinfo
  local userinfo, hostport = auth_host:match("^([^@]+)@(.+)$")
  if userinfo then
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
  local host, port = auth_host:match("^%[([^%]]+)%]:(%d+)$")
  if not host then
    host, port = auth_host:match("^%[([^%]]+)%]$")
    if not host then
      host, port = auth_host:match("^([^:]+):(%d+)$")
      if not host then
        host = auth_host
      end
    end
  end

  result.host = host
  result.port = port and tonumber(port) or (result.secure and 443 or 80)

  -- split path and query
  local path, query = path_query:match("^([^?]*)?(.*)$")
  if path then
    result.path = path
    result.query = query
  else
    result.path = path_query
    result.query = nil
  end

  if result.path == "" then
    result.path = "/"
  end

  -- reject fragments
  if url:find("#") then
    return nil, "URL contains a fragment identifier"
  end

  result.request_path = result.path
  if result.query and result.query ~= "" then
    result.request_path = result.path .. "?" .. result.query
  end

  return result
end

return M
