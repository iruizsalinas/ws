-- A tiny test harness. Each test drives the real event loop over sockets.
local ws = require("ws")
local loop = require("ws.loop")

local H = { passed = 0, failed = {} }

-- Runs the loop until idle; a test that hangs fails instead of blocking.
function H.run(limit)
  local deadline = os.time() + (limit or 10)
  while ws.poll(0.05) do
    if os.time() > deadline then
      error("timed out waiting for the loop to go idle", 2)
    end
  end
end

-- Drops whatever a failed test left behind.
local function reset()
  for _, s in pairs(loop.servers) do s:close() end
  for _, s in pairs(loop.sockets) do s:terminate() end
  pcall(H.run, 5)
end

function H.test(name, fn)
  local ok, err = pcall(fn)
  if ok then
    H.passed = H.passed + 1
    io.write(".")
  else
    H.failed[#H.failed + 1] = name .. ": " .. tostring(err)
    io.write("F")
    reset()
  end
  io.flush()
end

function H.eq(actual, expected, what)
  if actual ~= expected then
    error((what or "value") .. ": expected " .. tostring(expected) .. ", got " .. tostring(actual), 2)
  end
end

function H.raises(fn, text)
  local ok, err = pcall(fn)
  if ok then error("expected an error containing '" .. text .. "'", 2) end
  if not tostring(err):find(text, 1, true) then
    error("expected an error containing '" .. text .. "', got: " .. tostring(err), 2)
  end
end

function H.server(options)
  options = options or {}
  options.port, options.host = 0, "127.0.0.1"
  return ws.listen(options)
end

function H.url(server, path)
  return "ws://127.0.0.1:" .. server.port .. (path or "/")
end

-- An echo server that shuts down once its first connection closes.
function H.echo(options)
  local server = H.server(options)
  server:on("connection", function(c)
    c:on("message", function(data, binary) c:send(data, binary) end)
    c:on("close", function() server:close() end)
  end)
  return server
end

function H.report()
  print(("\n%d passed, %d failed"):format(H.passed, #H.failed))
  for _, f in ipairs(H.failed) do
    print("  FAIL " .. f)
  end
  return #H.failed == 0
end

return H
