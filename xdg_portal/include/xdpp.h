#ifndef XDPW_H
#define XDPW_H

#ifdef HAVE_LIBSYSTEMD
#include <systemd/sd-bus.h>
#elif HAVE_LIBELOGIND
#include <elogind/sd-bus.h>
#elif HAVE_BASU
#include <basu/sd-bus.h>
#endif

#include <stdbool.h>
#include <errno.h>
#include <stdlib.h>

#include "config.h"

struct xdpp_state {
    sd_bus *bus;
    struct xdpp_config *config;
};

struct xdpp_request {
    sd_bus_slot *slot;
};

struct xdpp_session {
    sd_bus_slot *slot;
    char *session_handle;
};

typedef void (*xdpp_event_loop_timer_func_t)(void *data);

enum {
    PORTAL_RESPONSE_SUCCESS = 0,
    PORTAL_RESPONSE_CANCELLED = 1,
    PORTAL_RESPONSE_ENDED = 2
};

int xdpp_filechooser_init(struct xdpp_state *state);

struct xdpp_request *xdpp_request_create(sd_bus *bus, const char *object_path);
void xdpp_request_destroy(struct xdpp_request *req);

struct xdpp_session *xdpp_session_create(struct xdpp_state *state, sd_bus *bus, char *object_path);
void xdpp_session_destroy(struct xdpp_session *req);

struct xdpp_timer *xdpp_add_timer(struct xdpp_state *state,
        uint64_t delay_ns, xdpp_event_loop_timer_func_t func, void *data);

void xdpp_destroy_timer(struct xdpp_timer *timer);

#endif
