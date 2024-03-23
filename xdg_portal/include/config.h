#ifndef CONFIG_H
#define CONFIG_H

#include "logger.h"

struct config_filechooser {
    char *cmd;
    char *default_save_dir;
    char *postprocess_dir;
};

struct xdpp_config {
    struct config_filechooser filechooser_conf;
};

void print_config(enum LOGLEVEL loglevel, struct xdpp_config *config);
void finish_config(struct xdpp_config *config);
void init_config(char ** const configfile, struct xdpp_config *config);

#endif
