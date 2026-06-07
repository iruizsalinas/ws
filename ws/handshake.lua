local compat = require("ws.compat")
local sha1_mod = require("ws.sha1")
local base64 = require("ws.base64")
local url_mod = require("ws.url")
local frame_mod = require("ws.frame")
local extension = require("ws.extension")
local deflate_mod = require("ws.deflate")
local validation = require("ws.validation")

local M = {}

local function is_ip_address(host)
  return host:match("^%d+%.%d+%.%d+%.%d+$") ~= nil or
         host:find(":", 1, true) ~= nil
end

local function hostname_matches(pattern, host)
  pattern = pattern:lower()
  host = host:lower()

  if pattern == host then return true end
  if pattern:sub(1, 2) ~= "*." then return false end
  if pattern:find("*", 3, true) then return false end

  local suffix = pattern:sub(3)
  if suffix == "" or not suffix:find(".", 1, true) then return false end
  if suffix:find("%.%.") or suffix:sub(1, 1) == "." or suffix:sub(-1) == "." then
    return false
  end

  suffix = "." .. suffix
  if host:sub(-#suffix) ~= suffix then return false end

  local prefix = host:sub(1, #host - #suffix)
  return prefix ~= "" and prefix:find(".", 1, true) == nil
end

local function get_common_name(cert)
  local ok, subject = pcall(cert.subject, cert)
  if not ok then return nil end
  if type(subject) ~= "table" then return nil end
  for _, entry in ipairs(subject) do
    if entry.name == "commonName" or entry.oid == "2.5.4.3" then
      return entry.value
    end
  end
  return nil
end

local function verify_includes_peer(verify)
  if verify == "peer" then return true end
  if type(verify) ~= "table" then return false end

  for _, value in ipairs(verify) do
    if value == "peer" then return true end
  end
  return false
end

function M._verify_hostname(cert, host)
  local ok, extensions = pcall(cert.extensions, cert)
  if not ok then extensions = nil end

  local san = type(extensions) == "table" and extensions["2.5.29.17"] or nil
  if is_ip_address(host) then
    local ip_names = type(san) == "table" and san.iPAddress or nil
    if type(ip_names) == "string" then return ip_names == host end
    if type(ip_names) == "table" then
      for _, name in ipairs(ip_names) do
        if name == host then return true end
      end
    end
    return false
  end

  local dns_names = type(san) == "table" and san.dNSName or nil
  if type(dns_names) == "string" then
    return hostname_matches(dns_names, host)
  end
  if type(dns_names) == "table" then
    for _, name in ipairs(dns_names) do
      if hostname_matches(name, host) then return true end
    end
    return false
  end

  local common_name = get_common_name(cert)
  return type(common_name) == "string" and hostname_matches(common_name, host)
end

local function has_ctl(value)
  return type(value) == "string" and value:find("[%z\1-\31\127]") ~= nil
end

local function validate_header_name(name)
  if type(name) ~= "string" or name == "" then
    return nil, "invalid HTTP header name"
  end
  for i = 1, #name do
    local code = name:byte(i)
    if code > 127 or validation.token_chars[code] ~= 1 then
      return nil, "invalid HTTP header name"
    end
  end
  return true
end

local function validate_header_value(value)
  if validation.has_invalid_header_value(value) then
    return nil, "invalid HTTP header value"
  end
  return true
end

local function validate_token(value, what)
  if type(value) ~= "string" or value == "" then
    return nil, "invalid " .. what
  end
  for i = 1, #value do
    local code = value:byte(i)
    if code > 127 or validation.token_chars[code] ~= 1 then
      return nil, "invalid " .. what
    end
  end
  return true
end

local function validate_request_target(target)
  if has_ctl(target) or target:find("[ \t]") then
    return nil, "invalid URL request path"
  end
  return true
end

local function close_and_fail(sock, ws, msg)
  sock:close()
  ws:_abort(msg)
  return nil, msg
end

function M.perform(ws, address, protocols, socket_lib)
  local parsed, err = url_mod.parse(address)
  if not parsed then
    ws:_abort(err)
    return nil, err
  end

  ws.url = address

  local sock, cerr = socket_lib.tcp()
  if not sock then
    ws:_abort(cerr)
    return nil, cerr
  end

  sock:settimeout(ws._handshake_timeout or 30)

  local ok, err2 = sock:connect(parsed.host, parsed.port)
  if not ok then
    return close_and_fail(sock, ws, "connection failed: " .. tostring(err2))
  end

  if parsed.secure then
    sock, err = M._wrap_tls(sock, parsed.host, ws)
    if not sock then return nil, err end
  end

  local key = base64.encode(compat.random_bytes(16))
  local per_message_deflate
  local rok, request, pmd = pcall(M._build_request, ws, parsed, key, protocols)
  if not rok then
    return close_and_fail(sock, ws, request)
  end
  per_message_deflate = pmd

  local ok3, serr = sock:send(request)
  if not ok3 then
    return close_and_fail(sock, ws, "failed to send handshake: " .. tostring(serr))
  end

  local response_headers, status_code, rerr =
    M._read_response(sock, ws)
  if not response_headers then return nil, rerr or status_code end

  -- handle redirects
  if status_code >= 300 and status_code < 400 and ws._follow_redirects then
    local location = response_headers["location"]
    if location then
      ws._redirects = ws._redirects + 1
      if ws._redirects > ws._max_redirects then
        return close_and_fail(sock, ws, "maximum redirects exceeded")
      end
      sock:close()
      ws.ready_state = "CONNECTING"
      ws:emit("redirect", location)
      return M.perform(ws, location, protocols, socket_lib)
    end
  end

  if status_code ~= 101 then
    return close_and_fail(sock, ws, "unexpected server response: " .. status_code)
  end

  local _, verr = M._validate_response(
    sock, ws, response_headers, key, protocols, per_message_deflate)
  if verr then return nil, verr end

  sock:settimeout(0)
  return sock
end

function M._wrap_tls(sock, host, ws)
  local has_ssl, ssl = pcall(require, "ssl")
  if not has_ssl then
    return close_and_fail(sock, ws, "luasec is required for wss:// connections")
  end

  local tls_params = {
    mode = "client",
    protocol = ws._tls_options.protocol or "any",
    verify = ws._tls_options.verify or "peer",
    options = { "all" },
  }
  for k, v in pairs(ws._tls_options) do
    if k ~= "protocol" and k ~= "verify" then tls_params[k] = v end
  end

  local wrapped, werr = ssl.wrap(sock, tls_params)
  if not wrapped then
    return close_and_fail(sock, ws, "TLS wrap failed: " .. tostring(werr))
  end

  wrapped:sni(host)
  local hok, herr = wrapped:dohandshake()
  if not hok then
    wrapped:close()
    ws:_abort("TLS handshake failed: " .. tostring(herr))
    return nil, herr
  end

  if verify_includes_peer(tls_params.verify) then
    local cert = wrapped:getpeercertificate()
    if not cert or not M._verify_hostname(cert, host) then
      wrapped:close()
      local msg = "TLS hostname verification failed"
      ws:_abort(msg)
      return nil, msg
    end
  end

  return wrapped
end

function M._build_request(ws, parsed, key, protocols)
  assert(validate_request_target(parsed.request_path))
  assert(validate_header_value(parsed.host))

  local host = parsed.host
  if parsed.host_bracketed then
    host = "[" .. host .. "]"
  end

  local port_str = ""
  local default_port = parsed.secure and 443 or 80
  if parsed.port ~= default_port then
    port_str = ":" .. parsed.port
  end

  local headers = {
    "GET " .. parsed.request_path .. " HTTP/1.1",
    "Host: " .. host .. port_str,
    "Upgrade: websocket",
    "Connection: Upgrade",
    "Sec-WebSocket-Version: 13",
    "Sec-WebSocket-Key: " .. key,
  }

  local per_message_deflate
  if ws._per_message_deflate ~= false and deflate_mod.available() then
    local src = type(ws._per_message_deflate) == "table"
                and ws._per_message_deflate or {}
    local opts = {}
    for k, v in pairs(src) do opts[k] = v end
    opts.is_server = false
    opts.max_payload = ws._max_payload
    per_message_deflate = deflate_mod.new(opts)
    local offer = per_message_deflate:offer()
    headers[#headers + 1] = "Sec-WebSocket-Extensions: " ..
      extension.format({ [deflate_mod.extension_name] = { offer } })
  end

  if #protocols > 0 then
    for _, protocol in ipairs(protocols) do
      assert(validate_token(protocol, "subprotocol"))
    end
    headers[#headers + 1] = "Sec-WebSocket-Protocol: " .. table.concat(protocols, ",")
  end
  if ws._origin then
    assert(validate_header_value(ws._origin))
    headers[#headers + 1] = "Origin: " .. ws._origin
  end
  for k, v in pairs(ws._headers) do
    assert(validate_header_name(k))
    assert(validate_header_value(v))
    headers[#headers + 1] = k .. ": " .. v
  end

  headers[#headers + 1] = ""
  headers[#headers + 1] = ""
  return table.concat(headers, "\r\n"), per_message_deflate
end

function M._read_response(sock, ws)
  local max_header_size = ws._max_response_header_size or 8192
  local max_headers = ws._max_response_headers or 100
  local buffer = ""
  local size = 0
  local status_code
  local response_headers = {}
  local header_count = 0

  while size < max_header_size do
    local chunk, rerr = sock:receive(1)
    if not chunk then
      local message = status_code and "failed reading headers: " or
        "failed to read response: "
      return close_and_fail(sock, ws, message .. tostring(rerr))
    end

    size = size + 1
    buffer = buffer .. chunk

    while true do
      local newline = buffer:find("\n", 1, true)
      if not newline then break end

      local line = buffer:sub(1, newline - 1)
      if line:sub(-1) == "\r" then line = line:sub(1, -2) end
      buffer = buffer:sub(newline + 1)

      if not status_code then
        local _, code = line:match("^(HTTP/%d+%.%d+)%s+(%d+)")
        if not code then
          return close_and_fail(sock, ws, "invalid HTTP response")
        end
        status_code = tonumber(code)
      elseif line == "" then
        response_headers._last_header = nil
        return response_headers, status_code
      else
        header_count = header_count + 1
        if header_count > max_headers then
          return close_and_fail(sock, ws, "too many response headers")
        end

        if line:find("^[ \t]") then
          if not response_headers._last_header then
            return close_and_fail(sock, ws, "invalid HTTP response header")
          end
          local value = response_headers[response_headers._last_header] ..
            " " .. validation.trim_ows(line)
          if not validate_header_value(value) then
            return close_and_fail(sock, ws, "invalid HTTP response header")
          end
          response_headers[response_headers._last_header] = value
        else
          local name, value = line:match("^([^:]+):(.*)")
          if not name or not validate_header_name(name) or
             not validate_header_value(value) then
            return close_and_fail(sock, ws, "invalid HTTP response header")
          end
          response_headers._last_header =
            validation.append_header(response_headers, name, value)
        end
      end
    end
  end

  return close_and_fail(sock, ws, "response headers too large")
end

function M._validate_response(sock, ws, headers, key, protocols, pmd)
  local upgrade = headers["upgrade"]
  if not validation.header_has_token(upgrade, "websocket") then
    return close_and_fail(sock, ws, "invalid Upgrade header")
  end

  local connection = headers["connection"]
  if not validation.header_has_token(connection, "upgrade") then
    return close_and_fail(sock, ws, "invalid Connection header")
  end

  local expected = base64.encode(sha1_mod.sha1(key .. frame_mod.GUID))
  local accept = headers["sec-websocket-accept"]
  if accept then accept = validation.trim_ows(accept) end
  if accept ~= expected then
    return close_and_fail(sock, ws, "invalid Sec-WebSocket-Accept header")
  end

  local server_proto = headers["sec-websocket-protocol"]
  if server_proto then server_proto = validation.trim_ows(server_proto) end
  if server_proto then
    if #protocols == 0 then
      return close_and_fail(sock, ws, "server sent an unsolicited subprotocol")
    end

    local found = false
    for _, p in ipairs(protocols) do
      if p == server_proto then found = true break end
    end
    if not found then
      return close_and_fail(sock, ws, "server sent an invalid subprotocol")
    end
    ws.protocol = server_proto
  end

  local exts = {}
  local ext_header = headers["sec-websocket-extensions"]
  if ext_header then
    if not pmd then
      return close_and_fail(sock, ws, "server sent extensions but none were requested")
    end

    local eok, parsed = pcall(extension.parse, ext_header)
    if not eok then
      return close_and_fail(sock, ws, "invalid Sec-WebSocket-Extensions header")
    end

    local names = {}
    for k in pairs(parsed) do names[#names + 1] = k end
    if #names ~= 1 or names[1] ~= deflate_mod.extension_name then
      return close_and_fail(sock, ws, "server indicated an extension that was not requested")
    end

    local aok = pcall(pmd.accept, pmd, parsed[deflate_mod.extension_name])
    if not aok then
      return close_and_fail(sock, ws, "invalid Sec-WebSocket-Extensions header")
    end

    exts[deflate_mod.extension_name] = pmd
  end

  ws:_setup_socket(sock, exts)
  return nil
end

return M
