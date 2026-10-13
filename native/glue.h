/* Values and module functions shared by the Lua glue translation units. */
#ifndef WS_GLUE_H
#define WS_GLUE_H
#include "luaapi.h"
#include "ws.h"

ws_str opt_str(lua_State *L, int i);
uint64_t check_id(lua_State *L, int i);
uint64_t opt_size(lua_State *L, int i);
double opt_heartbeat(lua_State *L, int i);
void prepare_str(lua_State *L, ws_str *s);
void push_str(lua_State *L, ws_str *s);
int result(lua_State *L, int rc, ws_str *s);
int handle(lua_State *L, int rc, uint64_t id, ws_str *s);

int l_wait(lua_State *L);
int l_connect(lua_State *L);
int l_listen(lua_State *L);
int l_port(lua_State *L);
int l_unlisten(lua_State *L);
int l_reply(lua_State *L);
int l_send(lua_State *L);
int l_ping(lua_State *L);
int l_close(lua_State *L);
int l_terminate(lua_State *L);
int l_protocol(lua_State *L);
#endif
