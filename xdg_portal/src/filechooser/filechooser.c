#include "xdpp.h"
#include <errno.h>
#include <fcntl.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define PATH_PREFIX "file://"

static const char object_path[] = "/org/freedesktop/portal/desktop";
static const char interface_name[] = "org.freedesktop.impl.portal.FileChooser";

char* getdir(char* path, size_t len, char* postproc) {
    for (size_t i = len-1; i > 0; --i) {
        if (path[i] == '/') {
            char* dir = strndup(path, i);
            if (!strcmp(dir,postproc)) {
                free(dir);
                return NULL;
            }
            struct stat path_stat;
            if (stat(dir, &path_stat) == 0) {
                if (S_ISDIR(path_stat.st_mode)) {
                    return dir;
                }
            }
            free(dir);
            return NULL;
        }
    }
    return NULL;
}

static int exec_filechooser(void *data, bool writing, bool multiple, bool directory,
                            char *path, char ***selected_files, size_t *num_selected_files) {
  struct xdpp_state *state = data;
  char *cmd_script = state->config->filechooser_conf.cmd;
  char *postproc_dir = state->config->filechooser_conf.postprocess_dir;
  if (!cmd_script) {
    logprint(ERROR, "cmd not specified");
    return -1;
  }
  if (!postproc_dir) {
      postproc_dir = "/tmp";
  }
  static char* prev_path = NULL;
  if (prev_path == NULL) {
      char* home = getenv("HOME");
      prev_path = home ? strdup(home) : strdup("");
  }
  if (path == NULL)
      path = prev_path;

  char buf[8096];
  snprintf(buf, sizeof(buf), "POSTPROCESS_DIR=%s %s %d %d %d \"%s\"",
          postproc_dir, cmd_script, multiple, directory, writing, path);

  logprint(TRACE, "executing command: %s", buf);
  FILE *fp = popen(buf, "r");
  if (!fp) {
    logprint(ERROR, "could not execute %s: %d", buf, errno);
    return -1;
  }
  size_t n = fread(buf, 1, sizeof(buf)-1, fp);
  pclose(fp);
  if (!n) {
    logprint(INFO, "read 0 bytes. Errno: %d", errno);
    return 0;
  }
  buf[n] = 0;
  logprint(TRACE, "cmd output: %s", buf);
  size_t num_lines = 0;
  for (char* c = buf; *c; ++c){
      if (*c == '\n')
          num_lines++;
  }
  *num_selected_files = num_lines;
  *selected_files = calloc(1, (num_lines + 1) * sizeof(char *));
  const size_t prefixlen = strlen(PATH_PREFIX);
  char* line = strtok(buf, "\n");
  for (size_t i = 0; line && i<num_lines; ++i) {
    size_t linesize = strlen(line);
    if (i == 0 && linesize) {
        char* dirname = getdir(line, linesize, postproc_dir);
        if (dirname) {
            free(prev_path);
            prev_path = dirname;
        }
    }
    linesize += prefixlen + 1;
    char* sline = calloc(1, linesize+1);
    (*selected_files)[i] = sline;
    snprintf(sline, linesize, "%s%s", PATH_PREFIX, line);
    line = strtok(NULL, "\n");
  }
  return 0;
}

static int method_open_file(sd_bus_message *msg, void *data, sd_bus_error *ret_error) {
  int ret = 0;

  char *handle, *app_id, *parent_window, *title;
  ret = sd_bus_message_read(msg, "osss", &handle, &app_id, &parent_window, &title);
  if (ret < 0) {
    return ret;
  }

  ret = sd_bus_message_enter_container(msg, 'a', "{sv}");
  if (ret < 0) {
    return ret;
  }
  char *key;
  int inner_ret = 0;
  int multiple = 0, directory = 0;
  while ((ret = sd_bus_message_enter_container(msg, 'e', "sv")) > 0) {
    inner_ret = sd_bus_message_read(msg, "s", &key);
    if (inner_ret < 0) {
      return inner_ret;
    }

    logprint(DEBUG, "dbus: option %s", key);
    if (strcmp(key, "multiple") == 0) {
      sd_bus_message_read(msg, "v", "b", &multiple);
      logprint(DEBUG, "dbus: option multiple: %d", multiple);
    } else if (strcmp(key, "modal") == 0) {
      int modal;
      sd_bus_message_read(msg, "v", "b", &modal);
      logprint(DEBUG, "dbus: option modal: %d", modal);
    } else if (strcmp(key, "directory") == 0) {
      sd_bus_message_read(msg, "v", "b", &directory);
      logprint(DEBUG, "dbus: option directory: %d", directory);
    } else {
      logprint(WARN, "dbus: unknown option %s", key);
      sd_bus_message_skip(msg, "v");
    }

    inner_ret = sd_bus_message_exit_container(msg);
    if (inner_ret < 0) {
      return inner_ret;
    }
  }
  if (ret < 0) {
    return ret;
  }
  ret = sd_bus_message_exit_container(msg);
  if (ret < 0) {
    return ret;
  }

  // TODO: cleanup this
  struct xdpp_request *req = xdpp_request_create(sd_bus_message_get_bus(msg), handle);
  if (req == NULL) {
    return -ENOMEM;
  }

  char **selected_files = NULL;
  size_t num_selected_files = 0;
  ret = exec_filechooser(data, false, multiple, directory, NULL, &selected_files, &num_selected_files);
  if (ret) {
    goto cleanup;
  }

  logprint(TRACE, "(OpenFile) Number of selected files: %d", num_selected_files);
  for (size_t i = 0; i < num_selected_files; i++) {
    logprint(TRACE, "%d. %s", i, selected_files[i]);
  }

  sd_bus_message *reply = NULL;
  ret = sd_bus_message_new_method_return(msg, &reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_append(reply, "u", PORTAL_RESPONSE_SUCCESS, 1);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_open_container(reply, 'a', "{sv}");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_open_container(reply, 'e', "sv");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_append_basic(reply, 's', "uris");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_open_container(reply, 'v', "as");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_append_strv(reply, selected_files);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_close_container(reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_close_container(reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_close_container(reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_send(NULL, reply, NULL);
  if (ret < 0) {
    goto cleanup;
  }

  sd_bus_message_unref(reply);

cleanup:
  for (size_t i = 0; i < num_selected_files; i++) {
    free(selected_files[i]);
  }
  free(selected_files);

  return ret;
}

static int method_save_file(sd_bus_message *msg, void *data, sd_bus_error *ret_error) {
  int ret = 0;

  char *handle, *app_id, *parent_window, *title;
  ret = sd_bus_message_read(msg, "osss", &handle, &app_id, &parent_window, &title);
  if (ret < 0) {
    return ret;
  }

  ret = sd_bus_message_enter_container(msg, 'a', "{sv}");
  if (ret < 0) {
    return ret;
  }
  char *key;
  int inner_ret = 0;
  char *current_name;
  char *current_folder = NULL;
  while ((ret = sd_bus_message_enter_container(msg, 'e', "sv")) > 0) {
    inner_ret = sd_bus_message_read(msg, "s", &key);
    if (inner_ret < 0) {
      return inner_ret;
    }

    logprint(DEBUG, "dbus: option %s", key);
    if (strcmp(key, "current_name") == 0) {
      sd_bus_message_read(msg, "v", "s", &current_name);
      logprint(DEBUG, "dbus: option current_name: %s", current_name);
    } else if (strcmp(key, "current_folder") == 0) {
      const void *p = NULL;
      size_t sz = 0;
      inner_ret = sd_bus_message_enter_container(msg, 'v', "ay");
      if (inner_ret < 0) {
        return inner_ret;
      }
      inner_ret = sd_bus_message_read_array(msg, 'y', &p, &sz);
      if (inner_ret < 0) {
        return inner_ret;
      }
      current_folder = (char *)p;
      logprint(DEBUG, "dbus: option current_folder: %s", current_folder);
    } else {
      logprint(WARN, "dbus: unknown option %s", key);
      sd_bus_message_skip(msg, "v");
    }

    inner_ret = sd_bus_message_exit_container(msg);
    if (inner_ret < 0) {
      return inner_ret;
    }
  }

  // TODO: cleanup this
  struct xdpp_request *req = xdpp_request_create(sd_bus_message_get_bus(msg), handle);
  if (req == NULL) {
    return -ENOMEM;
  }

  if (current_folder == NULL) {
    struct xdpp_state *state = data;
    char *default_save_dir = state->config->filechooser_conf.default_save_dir;
    if (!default_save_dir) {
      logprint(ERROR, "default_save_dir not specified");
      return -1;
    }
    current_folder = default_save_dir;
  }

  size_t path_size = snprintf(NULL, 0, "%s/%s", current_folder, current_name) + 1;
  char *path = calloc(1, path_size);
  snprintf(path, path_size, "%s/%s", current_folder, current_name);

  bool file_already_exists = true;
  while (file_already_exists) {
    if (access(path, F_OK) == 0) {
      char *path_tmp = calloc(1, path_size);
      snprintf(path_tmp, path_size, "%s", path);
      path_size += 1;
      path = realloc(path, path_size);
      snprintf(path, path_size, "%s_", path_tmp);
      free(path_tmp);
    } else {
      file_already_exists = false;
    }
  }

  char **selected_files = NULL;
  size_t num_selected_files = 0;
  ret = exec_filechooser(data, true, false, false, path, &selected_files, &num_selected_files);
  if (ret || num_selected_files == 0) {
    remove(path);
    ret = -1;
    goto cleanup;
  }

  logprint(TRACE, "(SaveFile) Number of selected files: %d", num_selected_files);
  for (size_t i = 0; i < num_selected_files; i++) {
    logprint(TRACE, "%d. %s", i, selected_files[i]);
  }

  sd_bus_message *reply = NULL;
  ret = sd_bus_message_new_method_return(msg, &reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_append(reply, "u", PORTAL_RESPONSE_SUCCESS, 1);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_open_container(reply, 'a', "{sv}");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_open_container(reply, 'e', "sv");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_append_basic(reply, 's', "uris");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_open_container(reply, 'v', "as");
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_append_strv(reply, selected_files);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_close_container(reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_close_container(reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_message_close_container(reply);
  if (ret < 0) {
    goto cleanup;
  }

  ret = sd_bus_send(NULL, reply, NULL);
  if (ret < 0) {
    goto cleanup;
  }

  sd_bus_message_unref(reply);

cleanup:
  for (size_t i = 0; i < num_selected_files; i++) {
    free(selected_files[i]);
  }
  free(selected_files);
  free(path);

  return ret;
}

static const sd_bus_vtable filechooser_vtable[] = {
    SD_BUS_VTABLE_START(0),
    SD_BUS_METHOD("OpenFile", "osssa{sv}", "ua{sv}", method_open_file, SD_BUS_VTABLE_UNPRIVILEGED),
    SD_BUS_METHOD("SaveFile", "osssa{sv}", "ua{sv}", method_save_file, SD_BUS_VTABLE_UNPRIVILEGED),
    SD_BUS_VTABLE_END};

int xdpp_filechooser_init(struct xdpp_state *state) {
  // TODO: cleanup
  sd_bus_slot *slot = NULL;
  logprint(DEBUG, "dbus: init %s", interface_name);
  return sd_bus_add_object_vtable(state->bus, &slot, object_path,
                                  interface_name, filechooser_vtable, state);
}
