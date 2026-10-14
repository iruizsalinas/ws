/* Close Lua states while shared runtime tasks and queued bytes are alive.
 * Loading another state must not revive stale pointers or unload worker code. */
#define _POSIX_C_SOURCE 200809L
#include <lua.h>
#include <lauxlib.h>
#include <lualib.h>
#include <stdio.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <time.h>
#endif

static const char *const script =
    "package.path='./?.lua;./?/init.lua;'..package.path;"
    "package.cpath='./?.so;./?.dll;'..package.cpath;"
    "local ws=require('ws'); local core=require('ws.core');"
    "local server=ws.listen{host='127.0.0.1',port=0,heartbeat=false};"
    "local client=ws.connect('ws://127.0.0.1:'..server.port,{heartbeat=false});"
    "if cycle%2==0 then "
    " local deadline=core.now()+10;"
    " while client.state~='open' do assert(core.now()<deadline); ws.poll(0.01) end;"
    " assert(client:send(string.rep('x',512*1024),true));"
    "end;"
    "if cycle%3==0 then collectgarbage('collect') end";

static void settle(void) {
#ifdef _WIN32
    Sleep(10);
#else
    struct timespec pause = {0, 10 * 1000 * 1000};
    nanosleep(&pause, NULL);
#endif
}

int main(void) {
    for (int cycle = 1; cycle <= 60; cycle++) {
        lua_State *lua = luaL_newstate();
        if (!lua) return 1;
        luaL_openlibs(lua);
        lua_pushinteger(lua, cycle);
        lua_setglobal(lua, "cycle");
        int status = luaL_loadstring(lua, script);
        if (!status) status = lua_pcall(lua, 0, 0, 0);
        if (status) {
            fprintf(stderr, "cycle %d: %s\n", cycle, lua_tostring(lua, -1));
            lua_close(lua);
            return 1;
        }
        lua_close(lua); /* intentionally no explicit socket/server shutdown */
        settle(); /* worker timers/I/O wake after the state and loader close */
    }
    puts("Repeated Lua state close/reload tests passed");
    return 0;
}
