#ifndef CONFIG_H
#define CONFIG_H

#include "logger.h"

struct config_filechooser {
    char *cmd;
    char *default_dir;
};

struct xdpw_config {
    struct config_filechooser filechooser_conf;
};

void print_config(enum LOGLEVEL loglevel, struct xdpw_config *config);
void finish_config(struct xdpw_config *config);
void init_config(char ** const configfile, struct xdpw_config *config);

#endif
