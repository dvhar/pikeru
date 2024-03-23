#include "config.h"
#include "xdpp.h"
#include "sds.h"
#include "logger.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <ini.h>
#include <sys/stat.h>

#define POSTPROCESS_DIR "/tmp/pk_postprocess"
static const char* const DEFAULT_CMDS[3] = {
    "/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
    "/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
    "/opt/pikeru/xdg_portal/contrib/pikeru-wrapper.sh"
};

void print_config(enum LOGLEVEL loglevel, struct xdpp_config *config) {
    logprint(loglevel, "config: cmd:  %s", config->filechooser_conf.cmd);
    logprint(loglevel, "config: default_save_dir:  %s", config->filechooser_conf.default_save_dir);
    logprint(loglevel, "config: postprocess_dir:  %s", config->filechooser_conf.postprocess_dir);
}

// NOTE: calling finish_config won't prepare the config to be read again from config file
// with init_config since to pointers and other values won't be reset to NULL, or 0
void finish_config(struct xdpp_config *config) {
    logprint(DEBUG, "config: destroying config");
    sdsfree(config->filechooser_conf.cmd);
    sdsfree(config->filechooser_conf.default_save_dir);
}

static void parse_string(char **dest, const char* value) {
    if (value == NULL || *value == '\0') {
        logprint(TRACE, "config: skipping empty value in config file");
        return;
    }
    sdsfree(*dest);
    if (value[0] == '~' && getenv("HOME") && strlen(value) > 1) {
        logprint(TRACE, "expanding home tilda from config");
        char* dir = sdscatfmt(sdsempty(), "%s%s", getenv("HOME"), value+1);
        *dest = dir;
        return;
    }
    *dest = sdsnew(value);
}

static int handle_ini_filechooser(struct config_filechooser *filechooser_conf, const char *key, const char *value) {
    if (strcmp(key, "cmd") == 0) {
        parse_string(&filechooser_conf->cmd, value);
    } else if (strcmp(key, "default_save_dir") == 0) {
        parse_string(&filechooser_conf->default_save_dir, value);
    } else if (strcmp(key, "postprocess_dir") == 0) {
        parse_string(&filechooser_conf->postprocess_dir, value);
    } else {
        logprint(TRACE, "config: skipping invalid key in config file");
        return 0;
    }
    return 1;
}

static int handle_ini_config(void *data, const char* section, const char *key, const char *value) {
    struct xdpp_config *config = (struct xdpp_config*)data;
    logprint(TRACE, "config: parsing section %s, key %s, value %s", section, key, value);

    if (strcmp(section, "filechooser") == 0) {
        return handle_ini_filechooser(&config->filechooser_conf, key, value);
    }

    logprint(TRACE, "config: skipping invalid key in config file");
    return 0;
}

static bool file_exists(const char *path) {
    return path && access(path, R_OK) != -1;
}
static bool dir_exists(const char *path) {
    struct stat path_stat;
    if (stat(path, &path_stat) == 0) {
        if (S_ISDIR(path_stat.st_mode)) {
            return 1;
        }
    } else {
        perror("stat");
    }
    return 0;
}

static void default_config(struct xdpp_config *config) {
    const char* cmd = NULL;
    for (size_t i=0; i<(sizeof(DEFAULT_CMDS)/sizeof(*DEFAULT_CMDS)); i++) {
        if (file_exists(DEFAULT_CMDS[i])) {
            cmd = DEFAULT_CMDS[i];
            break;
        }
    }
    config->filechooser_conf.cmd = sdsnew(cmd);
    config->filechooser_conf.default_save_dir = NULL;
    char* home = getenv("HOME");
    if (home) {
        char* default_save = sdscatfmt(sdsempty(), "%s/Downloads", home);
        if (dir_exists(default_save)) {
            config->filechooser_conf.default_save_dir = default_save;
        } else {
            sdsfree(default_save);
        }
    }
    if (!config->filechooser_conf.default_save_dir) {
        config->filechooser_conf.default_save_dir = sdsnew("/tmp");
    }
    config->filechooser_conf.postprocess_dir = sdsnew(POSTPROCESS_DIR);
}

static char *config_path(const char *prefix, const char *filename) {
    if (!prefix || !prefix[0] || !filename || !filename[0]) {
        return NULL;
    }

    char *config_folder = "xdg-desktop-portal-pikeru";
    char *path = sdscatfmt(sdsempty(), "%s/%s/%s", prefix, config_folder, filename);
    return path;
}

static char *get_config_path(void) {
    const char *home = getenv("HOME");
    char *config_home_fallback = sdsempty();
    if (home != NULL && home[0] != '\0') {
        config_home_fallback = sdscatfmt(config_home_fallback, "%s/.config", home);
    }

    const char *config_home = getenv("XDG_CONFIG_HOME");
    if (config_home == NULL || config_home[0] == '\0') {
        config_home = config_home_fallback;
    }

    const char *prefix[2];
    prefix[0] = config_home;
    prefix[1] = SYSCONFDIR "/xdg";

    const char *xdg_current_desktop = getenv("XDG_CURRENT_DESKTOP");
    const char *config_fallback = "config";

    char *config_list = NULL;
    for (size_t i = 0; i < 2; i++) {
        if (xdg_current_desktop) {
            config_list = sdsnew(xdg_current_desktop);
            char *config = strtok(config_list, ":");
            while (config) {
                char *path = config_path(prefix[i], config);
                if (!path) {
                    config = strtok(NULL, ":");
                    continue;
                }
                logprint(TRACE, "config: trying config file %s", path);
                if (file_exists(path)) {
                    sdsfree(config_list);
                    sdsfree(config_home_fallback);
                    return path;
                }
                sdsfree(path);
                config = strtok(NULL, ":");
            }
            sdsfree(config_list);
        }
        char *path = config_path(prefix[i], config_fallback);
        if (!path) {
            continue;
        }
        logprint(TRACE, "config: trying config file %s", path);
        if (file_exists(path)) {
            sdsfree(config_home_fallback);
            return path;
        }
        sdsfree(path);
    }

    sdsfree(config_home_fallback);
    return NULL;
}

void init_config(char ** const configfile, struct xdpp_config *config) {
    if (*configfile == NULL) {
        *configfile = get_config_path();
    }

    default_config(config);
    if (*configfile == NULL) {
        /*logprint(ERROR, "config: no config file found");*/
        return;
    }
    if (ini_parse(*configfile, handle_ini_config, config) < 0) {
        logprint(ERROR, "config: unable to load config file %s", *configfile);
    }
}
