-- End-to-end loopback benchmark, including Rust, the C ABI and Lua callbacks.
-- lua scripts/bench.lua [connections=32] [messages=100] [bytes=256]
--                       [window=8] [ws|wss] [binary|text] [idle_seconds=0]
-- Run a release build first. TLS uses the local test CA, never disabled checks.
package.path = "./?.lua;./?/init.lua;" .. package.path
package.cpath = "./?.so;./?.dll;" .. package.cpath
local ws = require("ws")
local core = require("ws.core")
local function integer(i, default, max)
  local n = default
  if arg[i] then n = tonumber(arg[i]) end
  assert(n and n >= 1 and n <= max and n % 1 == 0, "invalid argument " .. i)
  return n
end
local connections = integer(1, 32, 1000)
local messages = integer(2, 100, 100000)
local bytes = integer(3, 256, 1048576)
local window = integer(4, 8, 64)
local transport = arg[5] or "ws"
local kind = arg[6] or "binary"
local idle = arg[7] and tonumber(arg[7]) or 0
assert(transport == "ws" or transport == "wss", "transport must be ws or wss")
assert(kind == "binary" or kind == "text", "kind must be binary or text")
assert(idle and idle >= 0 and idle <= 60, "idle must be 0 to 60 seconds")
assert(connections * messages <= 1000000, "at most one million samples")
local payload = string.rep("x", bytes)
local binary = kind == "binary"
local tlsdir = "tests/fixtures/tls/"
local options = { host = "127.0.0.1", port = 0, heartbeat = false,
  limits = { connections = connections } }
if transport == "wss" then
  options.tls = { cert = tlsdir .. "server.pem", key = tlsdir .. "server-key.pem" }
end
local server = ws.listen(options)
local clients, samples = {}, {}
local phase, count, complete, started = "warmup", 10, 0, 0
local timeout = ws.after(120 + idle, function() error("benchmark timed out") end)
local fill, begin_phase

fill = function(s)
  while s.sent < count and s.sent - s.received < window do
    local at = core.now()
    local ok, err = s.socket:send(payload, binary)
    if not ok then
      assert(err == "send buffer is full", err)
      return -- retry on drain
    end
    s.sent = s.sent + 1
    s.pending[s.sent] = at
  end
end

begin_phase = function()
  complete = 0
  started = core.now()
  for _, s in ipairs(clients) do
    s.sent, s.received, s.pending = 0, 0, {}
    fill(s)
  end
end

server:on("connection", function(c)
  c:on("message", function(data, is_binary)
    local ok, err = c:send(data, is_binary)
    assert(ok, err) -- fail explicitly if the echo server can't keep up
  end)
end)

local function connect_one()
  local opts = { heartbeat = false }
  if transport == "wss" then opts.tls = { ca = tlsdir .. "ca.pem" } end
  local c = ws.connect(transport .. "://localhost:" .. server.port, opts)
  local s = { socket = c }
  clients[#clients + 1] = s
  c:on("drain", function() fill(s) end)
  c:on("message", function(data, is_binary)
    assert(data == payload and is_binary == binary, "echo mismatch")
    s.received = s.received + 1
    local at = assert(s.pending[s.received], "unsolicited echo")
    s.pending[s.received] = nil
    if phase == "measured" then samples[#samples + 1] = (core.now() - at) * 1000 end
    if s.received < count then
      fill(s)
    else
      complete = complete + 1
      if complete == connections then
        if phase == "warmup" then
          phase, count = "measured", messages
          begin_phase()
        else
          local elapsed = core.now() - started
          table.sort(samples)
          local function percentile(p) return samples[math.ceil(#samples * p)] end
          print(string.format(
            '{"lua":"%s","transport":"%s","kind":"%s","connections":%d,'
            .. '"messages_per_connection":%d,"bytes":%d,"window":%d,"seconds":%.6f,'
            .. '"echoes_per_second":%.2f,"rtt_ms_p50":%.3f,"rtt_ms_p95":%.3f,"rtt_ms_p99":%.3f}',
            _VERSION, transport, kind, connections, messages, bytes, window, elapsed,
            #samples / elapsed, percentile(0.50), percentile(0.95), percentile(0.99)))
          timeout:cancel()
          for _, client in ipairs(clients) do client.socket:terminate() end
          server:close()
        end
      end
    end
  end)
  c:on("open", function()
    if #clients < connections then
      connect_one() -- bound concurrent handshakes, independently of connection count
    elseif idle > 0 then
      io.stderr:write("idle: " .. connections .. " connections open\n")
      io.stderr:flush()
      ws.after(idle, begin_phase)
    else
      begin_phase()
    end
  end)
end

connect_one()
ws.run()
