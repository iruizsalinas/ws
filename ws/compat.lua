local M = {}

local version = tonumber(_VERSION:match("(%d+%.%d+)"))
M.lua_version = version
M.is_luajit = type(jit) == "table"
M.unpack = table.unpack or unpack

local bxor, band, bor, rshift, lshift, bnot

if M.is_luajit then
  local bit = require("bit")
  bxor = bit.bxor
  band = bit.band
  bor = bit.bor
  rshift = bit.rshift
  lshift = bit.lshift
  bnot = bit.bnot
elseif version >= 5.3 then
  bxor = load("return function(a,b) return a ~ b end")()
  band = load("return function(a,b) return a & b end")()
  bor = load("return function(a,b) return a | b end")()
  rshift = load("return function(a,n) return a >> n end")()
  lshift = load("return function(a,n) return a << n end")()
  bnot = load("return function(a) return ~a end")()
elseif version == 5.2 then
  local ok, bit32 = pcall(require, "bit32")
  if ok then
    bxor = bit32.bxor
    band = bit32.band
    bor = bit32.bor
    rshift = bit32.rshift
    lshift = bit32.lshift
    bnot = bit32.bnot
  end
end

if not bxor then
  local floor = math.floor

  band = function(a, b)
    local r, bit = 0, 1
    for _ = 0, 31 do
      if a % 2 == 1 and b % 2 == 1 then r = r + bit end
      a = floor(a / 2)
      b = floor(b / 2)
      bit = bit * 2
    end
    return r
  end

  bor = function(a, b)
    local r, bit = 0, 1
    for _ = 0, 31 do
      if a % 2 == 1 or b % 2 == 1 then r = r + bit end
      a = floor(a / 2)
      b = floor(b / 2)
      bit = bit * 2
    end
    return r
  end

  bxor = function(a, b)
    local r, bit = 0, 1
    for _ = 0, 31 do
      if a % 2 ~= b % 2 then r = r + bit end
      a = floor(a / 2)
      b = floor(b / 2)
      bit = bit * 2
    end
    return r
  end

  bnot = function(a)
    return 0xFFFFFFFF - a
  end

  lshift = function(a, n)
    return floor(a * 2 ^ n) % 0x100000000
  end

  rshift = function(a, n)
    return floor(a / 2 ^ n)
  end
end

M.bxor = bxor
M.band = band
M.bor = bor
M.rshift = rshift
M.lshift = lshift
M.bnot = bnot

function M.add32(a, b)
  return (a + b) % 0x100000000
end

function M.rotl32(x, n)
  return bor(lshift(band(x, 0xFFFFFFFF), n), rshift(band(x, 0xFFFFFFFF), 32 - n)) % 0x100000000
end

local random_source
local urandom = io.open("/dev/urandom", "rb")
if urandom then
  random_source = function(n)
    local data = urandom:read(n)
    if data and #data == n then return data end
    return nil
  end
end

if not random_source and M.is_luajit then
  local ffi_ok, ffi = pcall(require, "ffi")
  if ffi_ok and ffi.os == "Windows" then
    pcall(function()
      ffi.cdef[[
        typedef int BOOL;
        typedef unsigned long HCRYPTPROV;
        BOOL __stdcall CryptAcquireContextA(HCRYPTPROV*, const char*,
          const char*, unsigned long, unsigned long);
        BOOL __stdcall CryptGenRandom(HCRYPTPROV, unsigned long, unsigned char*);
      ]]
      local advapi32 = ffi.load("advapi32")
      local prov = ffi.new("HCRYPTPROV[1]")
      if advapi32.CryptAcquireContextA(prov, nil, nil, 1, 0xF0000000) ~= 0 then
        random_source = function(n)
          local buf = ffi.new("unsigned char[?]", n)
          if advapi32.CryptGenRandom(prov[0], n, buf) ~= 0 then
            return ffi.string(buf, n)
          end
          return nil
        end
      end
    end)
  end
end

if not random_source then
  math.randomseed(os.time() + math.floor(os.clock() * 1000))
  for _ = 1, 100 do math.random() end
end

function M.random_bytes(n)
  if random_source then
    local data = random_source(n)
    if data then return data end
  end
  local t = {}
  for i = 1, n do
    t[i] = string.char(math.random(0, 255))
  end
  return table.concat(t)
end

return M
