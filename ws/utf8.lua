local byte = string.byte

local function is_valid(str)
  local len = #str
  local i = 1

  while i <= len do
    local b = byte(str, i)

    if b < 0x80 then
      i = i + 1
    elseif b >= 0xC0 and b < 0xE0 then
      if i + 1 > len then return false end
      local b2 = byte(str, i + 1)
      if b2 < 0x80 or b2 >= 0xC0 then return false end
      -- reject overlong (< U+0080)
      if b < 0xC2 then return false end
      i = i + 2
    elseif b >= 0xE0 and b < 0xF0 then
      if i + 2 > len then return false end
      local b2, b3 = byte(str, i + 1), byte(str, i + 2)
      if b2 < 0x80 or b2 >= 0xC0 then return false end
      if b3 < 0x80 or b3 >= 0xC0 then return false end
      -- reject overlong (< U+0800)
      if b == 0xE0 and b2 < 0xA0 then return false end
      -- reject surrogates (U+D800 - U+DFFF)
      if b == 0xED and b2 >= 0xA0 then return false end
      i = i + 3
    elseif b >= 0xF0 and b < 0xF8 then
      if i + 3 > len then return false end
      local b2, b3, b4 = byte(str, i + 1), byte(str, i + 2), byte(str, i + 3)
      if b2 < 0x80 or b2 >= 0xC0 then return false end
      if b3 < 0x80 or b3 >= 0xC0 then return false end
      if b4 < 0x80 or b4 >= 0xC0 then return false end
      -- reject overlong (< U+10000)
      if b == 0xF0 and b2 < 0x90 then return false end
      -- reject > U+10FFFF
      if b == 0xF4 and b2 > 0x8F then return false end
      if b > 0xF4 then return false end
      i = i + 4
    else
      return false
    end
  end

  return true
end

return { is_valid = is_valid }
