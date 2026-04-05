local compat = require("ws.compat")
local sha1_mod = require("ws.sha1")
local base64 = require("ws.base64")
local url_mod = require("ws.url")
local frame_mod = require("ws.frame")
local extension = require("ws.extension")
local deflate_mod = require("ws.deflate")

local M = {}

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
  local request, pmd = M._build_request(ws, parsed, key, protocols)
  per_message_deflate = pmd

  local ok3, serr = sock:send(request)
  if not ok3 then
    return close_and_fail(sock, ws, "failed to send handshake: " .. tostring(serr))
  end

  local response_headers, status_code, rerr =
    M._read_response(sock, ws)
  if not response_headers then return nil, rerr end

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

  local verr = M._validate_response(
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
    verify = ws._tls_options.verify or "none",
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

  return wrapped
end

function M._build_request(ws, parsed, key, protocols)
  local port_str = ""
  local default_port = parsed.secure and 443 or 80
  if parsed.port ~= default_port then
    port_str = ":" .. parsed.port
  end

  local headers = {
    "GET " .. parsed.request_path .. " HTTP/1.1",
    "Host: " .. parsed.host .. port_str,
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
    headers[#headers + 1] = "Sec-WebSocket-Protocol: " .. table.concat(protocols, ",")
  end
  if ws._origin then
    headers[#headers + 1] = "Origin: " .. ws._origin
  end
  for k, v in pairs(ws._headers) do
    headers[#headers + 1] = k .. ": " .. v
  end

  headers[#headers + 1] = ""
  headers[#headers + 1] = ""
  return table.concat(headers, "\r\n"), per_message_deflate
end

function M._read_response(sock, ws)
  local status_line, rlerr = sock:receive("*l")
  if not status_line then
    return close_and_fail(sock, ws, "failed to read response: " .. tostring(rlerr))
  end

  local _, status_code = status_line:match("^(HTTP/%d+%.%d+)%s+(%d+)")
  if not status_code then
    return close_and_fail(sock, ws, "invalid HTTP response")
  end

  local response_headers = {}
  while true do
    local line, lerr = sock:receive("*l")
    if not line then
      return close_and_fail(sock, ws, "failed reading headers: " .. tostring(lerr))
    end
    if line == "" then break end
    local name, value = line:match("^([^:]+):%s*(.*)")
    if name then response_headers[name:lower()] = value end
  end

  return response_headers, tonumber(status_code)
end

function M._validate_response(sock, ws, headers, key, protocols, pmd)
  local upgrade = headers["upgrade"]
  if not upgrade or upgrade:lower() ~= "websocket" then
    return close_and_fail(sock, ws, "invalid Upgrade header")
  end

  local expected = base64.encode(sha1_mod.sha1(key .. frame_mod.GUID))
  if headers["sec-websocket-accept"] ~= expected then
    return close_and_fail(sock, ws, "invalid Sec-WebSocket-Accept header")
  end

  local server_proto = headers["sec-websocket-protocol"]
  if server_proto then
    local found = false
    for _, p in ipairs(protocols) do
      if p == server_proto then found = true break end
    end
    if #protocols > 0 and not found then
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
