#!/usr/bin/env python3
"""D-Bus caller for the pikeru portal backend using dbus-python."""
import sys
import dbus


def call_open_file(service, path, multiple, directory):
    bus = dbus.SessionBus()
    obj = bus.get_object(service, path)
    proxy = dbus.Interface(obj, "org.freedesktop.impl.portal.FileChooser")
    
    opts = {
        "multiple": dbus.Boolean(multiple),
        "directory": dbus.Boolean(directory),
    }
    
    result = proxy.OpenFile(
        "/org/test/handle",
        ":org.test.client",
        "",
        "Test",
        opts,
        variant_level=1)
    
    status = int(result[0])
    # result[1] is a dbus.Dictionary {string: variant<Array<strings>>}
    # Extract the 'uris' array from it
    uris = [str(u) for u in result[1].get('uris', [])] if status == 0 else []
    print("STATUS:%d URIS:%s" % (status, '|'.join(uris)))


def call_configure(service, path, respect_gitignore, search_ignore):
    bus = dbus.SessionBus()
    obj = bus.get_object(service, path)
    proxy = dbus.Interface(obj, "org.freedesktop.impl.portal.SearchIndexer")
    try:
        proxy.Configure(respect_gitignore, search_ignore)
        print("OK")
    except Exception as e:
        print("ERROR:%s" % str(e))


def call_update(service, path, dirs):
    bus = dbus.SessionBus()
    obj = bus.get_object(service, path)
    proxy = dbus.Interface(obj, "org.freedesktop.impl.portal.SearchIndexer")
    try:
        proxy.Update(dbus.Array(dirs, signature='s'))
        print("OK")
    except Exception as e:
        print("ERROR:%s" % str(e))


def call_clear_queue(service, path):
    bus = dbus.SessionBus()
    obj = bus.get_object(service, path)
    proxy = dbus.Interface(obj, "org.freedesktop.impl.portal.SearchIndexer")
    try:
        proxy.ClearQueue()
        print("OK")
    except Exception as e:
        print("ERROR:%s" % str(e))


def call_text_embed(service, path, text):
    bus = dbus.SessionBus()
    obj = bus.get_object(service, path)
    proxy = dbus.Interface(obj, "org.freedesktop.impl.portal.SearchIndexer")
    try:
        embedding = proxy.TextEmbed(text)
        # embedding is a dbus.Array of bytes — output hex for easy consumption
        hex_str = ''.join('{:02x}'.format(b) for b in embedding)
        print("EMBED:{}".format(hex_str))
    except Exception as e:
        print("ERROR:%s" % str(e))


def _ping(service):
    bus = dbus.SessionBus()
    try:
        owner = bus.get_name_owner(service)
        print("OK")
    except Exception as e:
        print("ERROR:%s" % str(e), file=sys.stderr); sys.exit(1)

if __name__ == "__main__":
    if sys.argv[1] == "_ping":
        _ping(sys.argv[2]); sys.exit(0)

    method = sys.argv[1]
    service = sys.argv[2]
    path = sys.argv[3]

    if method == "open_file":
        call_open_file(service, path, sys.argv[4] == "true", sys.argv[5] == "true")
    elif method == "configure":
        call_configure(service, path, sys.argv[4] == "true", sys.argv[5] if len(sys.argv) > 5 else "")
    elif method == "update":
        dirs = sys.argv[4:] if len(sys.argv) > 4 else []
        call_update(service, path, dirs)
    elif method == "clear_queue":
        call_clear_queue(service, path)
    elif method == "text_embed":
        call_text_embed(service, path, sys.argv[4])
    else:
        print("ERROR:unknown %s" % method, file=sys.stderr); sys.exit(1)
