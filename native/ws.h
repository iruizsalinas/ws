/* The C ABI exported by the Rust core (native/src). Every object is an
 * integer id. Returned strings belong to the caller, which releases them
 * with ws_free. */
#ifndef WS_H
#define WS_H

#include <stddef.h>
#include <stdint.h>

/* On ELF, ws.c then calls these directly and the module exports only
 * luaopen_ws_core, so same-named functions elsewhere cannot replace them. */
#ifdef __ELF__
#pragma GCC visibility push(hidden)
#endif

enum { WS_OK = 0, WS_SOFT = 1, WS_FATAL = 2 };

typedef struct {
    const char *ptr; /* NULL means absent */
    size_t len;
} ws_str;

typedef struct {
    int kind; /* 1 open .. 9 drain, see loop.kinds in ws/loop.lua */
    uint64_t id;
    uint64_t server;
    int code;
    ws_str data;
} ws_event;

ws_str ws_error(void);
void ws_free(ws_str s);
/* Opaque, stack-local conversion contexts; used only by the C glue. */
void *ws_copy_context(void);
void *ws_set_copy_context(void *context);
uint64_t ws_hub(void);
void ws_shutdown(uint64_t hub);
double ws_now(void);
int ws_wait(uint64_t hub, double timeout, ws_event *ev);

int ws_connect(uint64_t hub, ws_str url, ws_str protocols, ws_str headers, ws_str ca,
               double timeout, uint64_t max, uint64_t buffer, double heartbeat, uint64_t *id);
int ws_listen(uint64_t hub, ws_str host, uint16_t port, ws_str cert, ws_str key,
              uint64_t max, uint64_t buffer, uint64_t connections, double heartbeat,
              uint64_t *id);
uint16_t ws_port(uint64_t server);
void ws_unlisten(uint64_t server);
int ws_reply(uint64_t token, uint16_t status, ws_str protocol);

int ws_send(uint64_t id, ws_str data, _Bool binary);
int ws_ping(uint64_t id, ws_str data);
int ws_close(uint64_t id, int code, ws_str reason);
void ws_terminate(uint64_t id);
int ws_state(uint64_t id);
ws_str ws_protocol(uint64_t id);
uint64_t ws_buffered(uint64_t id);

#ifdef __ELF__
#pragma GCC visibility pop
#endif

#endif
