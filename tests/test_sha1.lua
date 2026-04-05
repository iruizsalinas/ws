local T = require("helper")
T.init("test_sha1.lua")

local sha1 = require("ws.sha1")

T.check_equal("empty string",
  sha1.sha1_hex(""),
  "da39a3ee5e6b4b0d3255bfef95601890afd80709")

T.check_equal("abc",
  sha1.sha1_hex("abc"),
  "a9993e364706816aba3e25717850c26c9cd0d89d")

T.check_equal("448-bit message",
  sha1.sha1_hex("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
  "84983e441c3bd26ebaae4aa1f95129e5e54670f1")

T.check_equal("websocket GUID",
  sha1.sha1_hex("dGhlIHNhbXBsZSBub25jZQ==258EAFA5-E914-47DA-95CA-C5AB0DC85B11"),
  "b37a4f2cc0624f1690f64606cf385945b2bec4ea")

T.check_equal("single char",
  sha1.sha1_hex("a"),
  "86f7e437faa5a7fce15d1ddcb9eaeaea377667b8")

T.check_equal("binary output length",
  #sha1.sha1("test"), 20)

T.check_equal("hex output length",
  #sha1.sha1_hex("test"), 40)


T.finish()
