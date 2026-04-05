local byte, char, sub = string.byte, string.char, string.sub
local concat = table.concat

local alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
local encode_map = {}
local decode_map = {}

for i = 1, 64 do
  local c = sub(alphabet, i, i)
  encode_map[i - 1] = c
  decode_map[byte(c)] = i - 1
end
decode_map[byte("=")] = 0

local function encode(data)
  local parts = {}
  local n = 1
  local len = #data

  for i = 1, len - 2, 3 do
    local b1, b2, b3 = byte(data, i, i + 2)
    parts[n] = encode_map[math.floor(b1 / 4)]
    parts[n + 1] = encode_map[(b1 % 4) * 16 + math.floor(b2 / 16)]
    parts[n + 2] = encode_map[(b2 % 16) * 4 + math.floor(b3 / 64)]
    parts[n + 3] = encode_map[b3 % 64]
    n = n + 4
  end

  local remainder = len % 3
  if remainder == 1 then
    local b1 = byte(data, len)
    parts[n] = encode_map[math.floor(b1 / 4)]
    parts[n + 1] = encode_map[(b1 % 4) * 16]
    parts[n + 2] = "="
    parts[n + 3] = "="
  elseif remainder == 2 then
    local b1, b2 = byte(data, len - 1, len)
    parts[n] = encode_map[math.floor(b1 / 4)]
    parts[n + 1] = encode_map[(b1 % 4) * 16 + math.floor(b2 / 16)]
    parts[n + 2] = encode_map[(b2 % 16) * 4]
    parts[n + 3] = "="
  end

  return concat(parts)
end

local function decode(data)
  data = data:gsub("[^A-Za-z0-9+/=]", "")
  local parts = {}
  local n = 1
  local len = #data

  for i = 1, len, 4 do
    local c1 = decode_map[byte(data, i)] or 0
    local c2 = decode_map[byte(data, i + 1)] or 0
    local c3 = decode_map[byte(data, i + 2)] or 0
    local c4 = decode_map[byte(data, i + 3)] or 0

    parts[n] = char(c1 * 4 + math.floor(c2 / 16))
    n = n + 1

    if sub(data, i + 2, i + 2) ~= "=" then
      parts[n] = char((c2 % 16) * 16 + math.floor(c3 / 4))
      n = n + 1
    end

    if sub(data, i + 3, i + 3) ~= "=" then
      parts[n] = char((c3 % 4) * 64 + c4)
      n = n + 1
    end
  end

  return concat(parts)
end

return {
  encode = encode,
  decode = decode,
}
