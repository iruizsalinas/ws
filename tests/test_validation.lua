local T = require("helper")
T.init("test_validation.lua")

local validation = require("ws.validation")

-- valid codes
T.check("1000 valid", validation.is_valid_status_code(1000))
T.check("1001 valid", validation.is_valid_status_code(1001))
T.check("1002 valid", validation.is_valid_status_code(1002))
T.check("1003 valid", validation.is_valid_status_code(1003))
T.check("1007 valid", validation.is_valid_status_code(1007))
T.check("1008 valid", validation.is_valid_status_code(1008))
T.check("1009 valid", validation.is_valid_status_code(1009))
T.check("1010 valid", validation.is_valid_status_code(1010))
T.check("1011 valid", validation.is_valid_status_code(1011))
T.check("1012 valid", validation.is_valid_status_code(1012))
T.check("1013 valid", validation.is_valid_status_code(1013))
T.check("1014 valid", validation.is_valid_status_code(1014))
T.check("3000 valid", validation.is_valid_status_code(3000))
T.check("3999 valid", validation.is_valid_status_code(3999))
T.check("4000 valid", validation.is_valid_status_code(4000))
T.check("4999 valid", validation.is_valid_status_code(4999))

-- reserved codes that must not be sent
T.check("1004 reserved", not validation.is_valid_status_code(1004))
T.check("1005 reserved", not validation.is_valid_status_code(1005))
T.check("1006 reserved", not validation.is_valid_status_code(1006))

-- invalid codes
T.check("0 invalid", not validation.is_valid_status_code(0))
T.check("999 invalid", not validation.is_valid_status_code(999))
T.check("1015 invalid", not validation.is_valid_status_code(1015))
T.check("1016 invalid", not validation.is_valid_status_code(1016))
T.check("2000 invalid", not validation.is_valid_status_code(2000))
T.check("2999 invalid", not validation.is_valid_status_code(2999))
T.check("5000 invalid", not validation.is_valid_status_code(5000))
T.check("65535 invalid", not validation.is_valid_status_code(65535))

-- token_chars
T.check("token ! valid", validation.token_chars[33] == 1)
T.check("token space invalid", validation.token_chars[32] == 0)
T.check("token A valid", validation.token_chars[65] == 1)
T.check("token ( invalid", validation.token_chars[40] == 0)

-- header_has_token
T.check("connection token exact", validation.header_has_token("Upgrade", "upgrade"))
T.check("connection token in list", validation.header_has_token("keep-alive, Upgrade", "upgrade"))
T.check("token match trims whitespace", validation.header_has_token(" keep-alive ,\tUpgrade ", "upgrade"))
T.check("token missing", not validation.header_has_token("close", "upgrade"))

T.finish()
