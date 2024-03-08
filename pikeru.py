#!/usr/bin/env python
import argparse, configparser
import glob
import os, sys, time
import tkinter as tk
from PIL import Image, ImageTk
from tkinter.messagebox import askyesno
from tkinter import ttk
import threading
from multiprocessing import cpu_count
import queue
import hashlib
import cv2
from tkinterdnd2 import TkinterDnD, DND_FILES, DND_TEXT
import requests
import subprocess
import mimetypes
import inotify.adapters
import inotify.constants

THUMBNAIL_WIDTH = 140
THUMBNAIL_HEIGHT = 140
INIT_WIDTH = 1024
INIT_HEIGHT = 720

# https://icon-icons.com
asset_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'assets')
home_dir = os.environ['HOME']
config_file = os.path.join(home_dir,'.config','pikeru.conf')
cache_dir = os.path.join(home_dir,'.cache','pikeru')

class FilePicker(tk.Frame):
    def __init__(self, args: argparse.Namespace, **kwargs):
        self.select_dir = args.mode == 'dir'
        self.select_multi = args.mode == 'files'
        self.select_save = args.mode == 'save'
        self.save_filename = None
        if self.select_save and not os.path.isdir(args.path):
            self.save_filename = os.path.basename(args.path)
        self.allowed_mimes = set(args.mime_list.split(' ')) if args.mime_list else None
        self.enable_mime_filtering = self.allowed_mimes != None
        self.ino = inotify.adapters.Inotify()
        self.watch_thread = threading.Thread(target=self.watch_loop, daemon=True)
        self.watch_thread.start()
        self.dropped_files = set()

        self.root = TkinterDnD.Tk()
        self.root.geometry(f'{INIT_WIDTH}x{INIT_HEIGHT}')
        self.root.wm_title(args.title or 'File Picker')
        x = (self.root.winfo_screenwidth() / 2) - (INIT_WIDTH / 2)
        y = (self.root.winfo_screenheight() / 2) - (INIT_HEIGHT / 2)
        self.root.geometry(f'+{int(x)}+{int(y)}')
        self.frame = tk.Frame(self.root, **kwargs)
        self.frame.grid_columnconfigure(0, weight=1)
        self.frame.grid_rowconfigure(0, weight=1)
        self.frame.grid_rowconfigure(1, weight=0)

        self.root.drop_target_register(DND_FILES, DND_TEXT)
        self.root.dnd_bind('<<Drop>>', self.drop_data)

        upper_frame = tk.Frame(self.frame)
        upper_frame.grid(row=0, column=0, sticky='news')
        upper_frame.grid_columnconfigure(0, weight=0)
        upper_frame.grid_columnconfigure(1, weight=1)
        upper_frame.grid_rowconfigure(0, weight=1)

        lower_frame = tk.Frame(self.frame)
        lower_frame.grid(row=1, column=0, sticky='news')
        lower_frame.grid_columnconfigure(0, weight=1)

        self.bookmark_frame = tk.Frame(upper_frame)
        self.bookmark_frame.grid(row=0, column=0, sticky='news')
        self.canvas = tk.Canvas(upper_frame)
        self.canvas.grid(row=0, column=1, sticky='news')
        self.scrollbar = tk.Scrollbar(upper_frame, orient='vertical', command=self.canvas.yview)
        self.scrollbar.grid(row=0, column=2, sticky='ns')
        self.canvas.configure(yscrollcommand=self.scrollbar.set)

        self.items_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_listeners(self.canvas)
        self.bind_listeners(self.items_frame)

        self.path_textfield = tk.Entry(lower_frame, insertbackground='red')
        self.path_textfield.grid(row=0, column=0, padx=(10, 0), pady=(1, 0), sticky='ew')
        self.path_textfield.insert(0, args.path)
        self.path_textfield.bind("<Return>", self.on_type_enter)

        self.button_frame = tk.Frame(lower_frame)
        self.button_frame.grid(row=1, column=0, sticky='we')
        button_text = "Save" if self.select_save else "Open"
        self.open_button = tk.Button(self.button_frame, width=10, text=button_text, command=self.on_select_button)
        self.open_button.pack(side='right')
        self.cancel_button = tk.Button(self.button_frame, width=10, text="Cancel", command=self.root.destroy)
        self.cancel_button.pack(side='right')
        self.up_dir_button = tk.Button(self.button_frame, width=10, text="Up Dir", command=self.on_up_dir)
        self.up_dir_button.pack(side='right')

        self.sort_button = tk.Button(self.button_frame, width=10, text="Sort", command=self.show_sort_menu)
        self.sort_button.pack(side='right')

        if self.enable_mime_filtering:
            self.mime_switch = tk.BooleanVar()
            self.mime_switch.set(self.enable_mime_filtering)
            self.mime_switch_btn = ttk.Checkbutton(self.button_frame, variable=self.mime_switch,
                  text="Filter mime", command=self.toggle_mime_filter)
            self.mime_switch_btn.pack(side='left')

        self.queue = queue.Queue()
        self.lock = threading.Lock()
        self.threads = []
        for i in range(cpu_count()):
            loading_thread = threading.Thread(target=self.load_items, daemon=True)
            loading_thread.start()
            self.threads.append(loading_thread)

        self.frame.bind('<Configure>', self.on_resize)
        max_width = INIT_WIDTH - self.bookmark_frame.winfo_width()
        self.max_cols = max(1, int(max_width / (THUMBNAIL_WIDTH+6)))
        self.folder_icon = tk.PhotoImage(file=asset_dir+'/folder.png')
        self.doc_icon = tk.PhotoImage(file=asset_dir+'/document.png')
        self.unknown_icon = tk.PhotoImage(file=asset_dir+'/unknown.png')
        self.error_icon = tk.PhotoImage(file=asset_dir+'/error.png')
        self.prev_sel = None
        self.load_config()

        for i, (name, path) in enumerate(self.bookmarks.items()):
            btn = tk.Button(self.bookmark_frame, text=name)
            btn.path = path
            btn.grid(row=i, column=0, sticky='news')
            btn.bind("<Button-1>", self.on_bookmark_click)

        self.frame.pack(fill='both', expand=True)
        self.change_dir(args.path)

    def toggle_mime_filter(self):
        self.enable_mime_filtering = not self.enable_mime_filtering
        self.load_dir()

    def mime_is_allowed(self, path):
        if not self.allowed_mimes or not hasattr(path, 'mime'):
            return True
        return path.mime in self.allowed_mimes

    def drop_data(self, event):
        url = event.data
        if url.startswith('http://') or url.startswith('https://'):
            response = requests.get(url)
            filename = os.path.basename(url)
            filepath = os.path.join(os.getcwd(), filename)
            self.dropped_files.add(filepath)
            with open(filepath, 'wb') as f:
                f.write(response.content)
            item = PathInfo(filepath)
            item.idx = len(self.items)
            self.items.append(None)
            self.load_item(item)
            self.on_click_file(FakeEvent(self.items[-1]))

    def run(self):
        self.root.mainloop()

    def mouse_nav(self, event):
        match event.num:
            case 4: self.canvas.yview_scroll(-2,'units')
            case 5: self.canvas.yview_scroll(2,'units')
            case 8: self.on_up_dir()

    def bind_listeners(self, thing):
        thing.bind('<Button>', self.mouse_nav)

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def load_items(self):
        while True:
            item_path = self.queue.get()
            self.load_item(item_path)

    def prep_file(self, label, item_path):
        label.sel = False
        label.path = item_path
        self.items[item_path.idx] = label
        self.bind_listeners(label)
        if not self.select_dir:
            label.bind("<Button-1>", self.on_click_file)
            label.bind("<Double-Button-1>", self.on_double_click_file)
        if os.path.dirname(item_path) == os.getcwd():
            label.grid(row=item_path.idx//self.max_cols, column=item_path.idx%self.max_cols)

    def prep_dir(self, label, item_path):
        label.path = item_path
        label.sel = False
        self.items[item_path.idx] = label
        self.bind_listeners(label)
        label.bind("<Double-Button-1>", self.on_double_click_dir)
        if self.select_dir:
            label.bind("<Button-1>", self.on_click_file)
        label.bind("<ButtonRelease-1>", self.on_drag_dir_end)
        if os.path.dirname(item_path) == os.getcwd():
            label.grid(row=item_path.idx//self.max_cols, column=item_path.idx%self.max_cols)

    def load_item(self, item_path):
        base_path = os.path.basename(item_path)
        name = base_path if len(base_path) < 20 else base_path[len(base_path)-19:]
        try:
            if os.path.isfile(item_path):
                ext = os.path.splitext(base_path)[-1].lower()
                match ext:
                    case '.png'|'.jpg'|'.jpeg'|'.gif':
                        img = self.prepare_cached_thumbnail(item_path, 'pic')
                        label = tk.Label(self.items_frame, image=img, text=name, compound='top')
                        label.__setattr__('img', img)
                        label.bind("<Button-3>", lambda e:self.on_view_image(e, True))
                        self.prep_file(label, item_path)
                    case '.mp4'|'.avi'|'.mkv'|'.webm':
                        img = self.prepare_cached_thumbnail(item_path, 'vid')
                        label = tk.Label(self.items_frame, image=img, text=name, compound='top')
                        label.__setattr__('img', img)
                        label.__setattr__('vid', True)
                        label.bind("<Button-3>", lambda e:self.on_view_image(e, True))
                        self.prep_file(label, item_path)
                    case '.txt'|'.pdf'|'.doc'|'.docx':
                        label = tk.Label(self.items_frame, image=self.doc_icon, text=name, compound='top')
                        label.__setattr__('img', self.doc_icon)
                        self.prep_file(label, item_path)
                    case _:
                        label = tk.Label(self.items_frame, image=self.unknown_icon, text=name, compound='top')
                        label.__setattr__('img', self.unknown_icon)
                        self.prep_file(label, item_path)
            elif os.path.isdir(item_path):
                label = tk.Label(self.items_frame, image=self.folder_icon, text=name, compound='top')
                self.prep_dir(label, item_path)
            else:
                return
        except Exception as e:
            label = tk.Label(self.items_frame, image=self.error_icon, text=name, compound='top')
            label.__setattr__('img', self.unknown_icon)
            self.prep_file(label, item_path)
            label.path.mime = 'application/octet-stream'
            sys.stderr.write(f'Error loading item: {e}\t{item_path}\n')

    def prepare_cached_thumbnail(self, item_path, imtype):
        md5hash = hashlib.md5(item_path.encode()).hexdigest()
        cache_path = os.path.join(cache_dir, f'{md5hash}.png')
        if os.path.isfile(cache_path):
            img = Image.open(cache_path)
            img = ImageTk.PhotoImage(img)
            return img
        else:
            if imtype == 'pic':
                img = Image.open(item_path)
                img.thumbnail((THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT))
                img.save(cache_path)
                img = ImageTk.PhotoImage(img)
                return img
            else:
                cap = cv2.VideoCapture(item_path)
                ret, frame = cap.read()
                cap.release()
                if not ret:
                    return self.error_icon
                frame = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
                img = Image.fromarray(frame)
                img.thumbnail((THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT))
                img.save(cache_path)
                img = ImageTk.PhotoImage(img)
                return img

    def on_drag_dir_end(self, event):
        source = event.widget
        target = source.winfo_containing(event.x_root, event.y_root)
        if target != self.bookmark_frame:
            return
        bookmarks =  self.bookmark_frame.winfo_children()
        for child in bookmarks:
            if source.path == child.path:
                return
        path = source.path
        basename = os.path.basename(path)
        new_bookmark = tk.Button(self.bookmark_frame, text=basename, command=lambda: self.change_dir(path))
        new_bookmark.path = path
        new_bookmark.grid(row=len(bookmarks), column=0, sticky='news')
        self.bookmark_frame.update_idletasks()
        with open(config_file, 'a') as f:
            f.write(f'{basename}={path}\n')

    def on_bookmark_click(self, event):
        self.change_dir(event.widget.path)

    def reorganize_items(self):
        num_rows = len(self.items) // self.max_cols + (1 if len(self.items) % self.max_cols != 0 else 0)
        for row in range(num_rows):
            start = row * self.max_cols
            for col in range(self.max_cols):
                idx = start + col
                if idx < len(self.items) and self.items[idx]:
                    self.items[idx].grid(row=row, column=col)

    def recalculate_max_cols(self):
        max_width = self.frame.winfo_width() - self.bookmark_frame.winfo_width()
        self.max_cols = max(1, int(max_width / (THUMBNAIL_WIDTH+6))) # figure out proper width calculation

    def on_click_file(self, event):
        label = event.widget
        if label.sel == False:
            label.config(relief="solid", bg='red')
            label.sel = True
        else:
            label.config(relief="flat", bg='black')
            label.sel = False
            self.path_textfield.delete(0, 'end')
            self.path_textfield.insert(0, os.getcwd())
        if label.sel:
            if not self.select_multi and self.prev_sel and self.prev_sel is not label:
                self.prev_sel.sel = False
                self.prev_sel.config(relief="flat", bg='black')
            self.prev_sel = label
            self.path_textfield.delete(0, 'end')
            self.path_textfield.insert(0, label.path)

    def on_view_image(self, event, goback):
        label : tk.Label = event.widget
        if not hasattr(label, 'img'):
            return
        if hasattr(label, 'vid'):
            cap = cv2.VideoCapture(label.path)
            ret, frame = cap.read()
            cap.release()
            if not ret:
                return
            frame = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
            img = Image.fromarray(frame)
        else:
            img = Image.open(label.path)
        m = min(self.canvas.winfo_height() / img.height, self.canvas.winfo_width() / img.width)
        img.thumbnail((img.width * m, img.height * m))
        expanded_img = ImageTk.PhotoImage(img)
        temp_label = tk.Label(self.canvas, image=expanded_img, bd=0)
        temp_label.img = expanded_img
        temp_label.path = label.path
        if goback:
            self.current_y = self.canvas.yview()[0]
        self.canvas.yview_moveto(0)
        self.canvas.delete("all")
        self.canvas.unbind("<Button>")
        x_pos = (self.canvas.winfo_width() - temp_label.winfo_width()) // 2
        y_pos = (self.canvas.winfo_height() - temp_label.winfo_height()) // 2
        self.canvas.create_window(x_pos, y_pos, window=temp_label, anchor='center')
        temp_label.bind("<Button-2>", self.close_expanded_image)
        temp_label.bind("<Button-3>", self.close_expanded_image)
        temp_label.bind("<Button-4>", self.on_scroll_image)
        temp_label.bind("<Button-5>", self.on_scroll_image)
        temp_label.bind("<Double-Button-1>",lambda _: self.on_double_click_file(event))
        if not event.widget.sel:
            self.on_click_file(event)
            self.unselect = event
        else:
            self.unselect = None

    def on_scroll_image(self, event):
        step = -1 if event.num==4 else 1
        idx = event.widget.path.idx
        inrange = (lambda i: i > 0) if step == -1 else (lambda i: i<(len(self.items)-1))
        nextimage = None
        while inrange(idx):
            idx += step
            item = self.items[idx]
            if not hasattr(item.path, 'mime') or not item.path.mime.startswith('image'):
                continue
            nextimage = item
            break
        if nextimage:
            self.on_view_image(FakeEvent(nextimage), False)

    def close_expanded_image(self, event):
        event.widget.destroy()
        self.canvas.yview_moveto(self.current_y)
        self.items_frame.grid()
        self.canvas.delete("all")
        self.canvas.create_window(0, 0, window=self.items_frame, anchor='nw')
        self.canvas.bind("<Button>", self.mouse_nav)
        if self.unselect:
            self.on_click_file(self.unselect)
            self.unselect = None

    def watch_loop(self):
        for e in self.ino.event_gen():
            if e:
                path, file = e[2], e[3]
                filepath = os.path.join(path, file)
                if not self.mime_is_allowed(filepath) or filepath in self.dropped_files:
                    return
                print('new', filepath)
                time.sleep(0.1)
                item = PathInfo(filepath)
                item.idx = len(self.items)
                self.items.append(None)
                self.load_item(item)

    def change_dir(self, new_dir):
        if os.path.isdir(new_dir):
            self.ino.remove_watch(os.getcwd())
            self.ino.add_watch(new_dir, mask=inotify.constants.IN_CREATE)
            self.prev_sel = None
            os.chdir(new_dir)
            self.path_textfield.delete(0, 'end')
            if self.save_filename:
                new_dir += '/' + self.save_filename
            self.path_textfield.insert(0, new_dir)
            while self.queue.qsize() > 0:
                self.queue.get()
            self.dropped_files.clear()
            self.load_dir()
            self.canvas.yview_moveto(0)

    def on_up_dir(self):
        new_dir = os.path.dirname(os.getcwd())
        self.change_dir(new_dir)

    def on_double_click_dir(self, event):
        new_dir = event.widget.path
        self.change_dir(new_dir)

    def final_selection(self, selection):
        if self.select_save and os.path.isfile(selection):
            msg = f'Overwrite file {os.path.basename(selection)}?'
            overwrite = askyesno(title='Confirm Overwrite', message=msg)
            if not overwrite:
                return
        print(selection)
        self.root.destroy()

    def on_double_click_file(self, event):
        if self.select_save:
            self.final_selection(event.widget.path)
        else:
            print(event.widget.path)
            self.root.destroy()

    def on_select_button(self):
        selected_files = [label.path for label in self.items if label.sel]
        if self.select_save and len(selected_files) == 0:
            self.final_selection(self.path_textfield.get())
        elif self.select_save:
            self.final_selection(selected_files[0])
        else:
            print('\n'.join(selected_files))
            self.root.destroy()

    def on_type_enter(self, event):
        txt = self.path_textfield.get()
        if os.path.isdir(txt):
            self.change_dir(txt)
        elif self.select_save and txt[-1] != '/':
            self.final_selection(txt)

    def load_dir(self):
        self.items_frame.destroy()
        self.items_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_listeners(self.canvas)
        self.bind_listeners(self.items_frame)
        paths = [pi for pi in (PathInfo(p) for p in glob.glob(os.path.join(os.getcwd(), '*')))
                 if self.mime_is_allowed(pi) or not self.enable_mime_filtering]
        paths.sort(key=lambda p: (not p.isdir, p.lname))
        self.items = [None] * len(paths)
        for i, path in enumerate(paths):
            path.idx = i
            self.queue.put(path)

    def show_sort_menu(self):
        self.sort_popup = tk.Menu(self.root, tearoff=False)
        self.sort_popup.add_command(label="Name asc", command=lambda :self.on_sort('name', True))
        self.sort_popup.add_command(label="Name desc", command=lambda :self.on_sort('name', False))
        self.sort_popup.add_command(label="Date oldest first", command=lambda :self.on_sort('time', True))
        self.sort_popup.add_command(label="Date newest first", command=lambda :self.on_sort('time', False))
        self.sort_popup.post(self.sort_button.winfo_rootx(), self.sort_button.winfo_rooty())

    def on_sort(self, by, asc):
        match (by, asc):
            case ('name', True): sort = lambda w: (not w.path.isdir, w.path.lname)
            case ('name', False): sort = lambda w: (w.path.isdir, w.path.lname)
            case ('time', True): sort = lambda w: (not w.path.isdir, w.path.time)
            case ('time', False): sort = lambda w: (w.path.isdir, w.path.time)
            case _: quit(1)
        self.items.sort(key=sort, reverse=not asc)
        num_rows = len(self.items) // self.max_cols + (1 if len(self.items) % self.max_cols != 0 else 0)
        for row in range(num_rows):
            start = row * self.max_cols
            for col in range(self.max_cols):
                idx = start + col
                if idx < len(self.items) and self.items[idx]:
                    self.items[idx].grid(row=row, column=col)
                    self.items[idx].path.idx = idx

    def on_resize(self, event=None):
        old = self.max_cols
        self.recalculate_max_cols()
        if old != self.max_cols:
            self.reorganize_items()

    def run_cmd(self, cmd: str):
        selected_items = [label.path for label in self.items if label.sel]
        for item_path in selected_items:
            base_name = os.path.basename(item_path)
            directory = os.path.dirname(item_path)
            part, ext = os.path.splitext(base_name) if os.path.isfile(item_path) else ''
            cmd = cmd.replace('[path]', item_path)
            cmd = cmd.replace('[name]', base_name)
            cmd = cmd.replace('[ext]', ext)
            cmd = cmd.replace('[dir]', directory)
            cmd = cmd.replace('[part]', part)
            proc = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stdout, stderr = proc.communicate()
            print(cmd, file=sys.stderr)
            if stderr:
                print(stderr, file=sys.stderr)
            if stdout:
                print(stdout, file=sys.stderr)

    def show_cmd_menu(self):
        self.commands_popup = tk.Menu(self.root, tearoff=False)
        for cmd_name, cmd_val in self.commands.items():
            self.commands_popup.add_command(label=cmd_name, command=lambda: self.run_cmd(cmd_val))
        self.commands_popup.post(self.cmd_button.winfo_rootx(), self.cmd_button.winfo_rooty())

    def load_config(self):
        if not os.path.isfile(config_file):
            with open(config_file, 'w') as f:
                f.write(conftxt.format(home_dir=home_dir))
                f.writelines([f'{bm}={os.path.join(home_dir, bm)}\n' for bm in ["Documents", "Pictures", "Downloads"]])
        config = CaseConfigParser()
        config.read(os.path.expanduser(config_file))
        self.bookmarks = config['Bookmarks']
        if config.has_section('Commands'):
            self.commands = config['Commands']
            self.cmd_button = tk.Button(self.button_frame, width=10, text="Cmd", command=self.show_cmd_menu)
            self.cmd_button.pack(side='right')

class FakeEvent:
    def __init__(self, widget):
        self.widget = widget

class PathInfo(str):
    def __new__(cls, path):
        obj = str.__new__(cls, path)
        obj.time = os.path.getmtime(path)
        obj.lname = os.path.basename(path).lower()
        obj.isdir = False
        if os.path.isfile(path):
            obj.mime = mimetypes.guess_type(path)[0] or 'application/octet-stream'
        else:
            obj.isdir = os.path.isdir(path)
        return obj

class CaseConfigParser(configparser.RawConfigParser):
    def __init__(self, defaults=None):
        super().__init__(defaults)
    def optionxform(self, optionstr):
        return optionstr

conftxt = '''# Commands from the cmd menu will substitute these values from the selected files before running, as seen in the resize example:
# [path] is full file path
# [dir] is directory
# [name] is the filename without full path
# [part] is the filename without path or extension
# [ext] is the file extension, including the period
[Commands]\nresize=convert -resize 1200 [path] [dir]/[part]_resized[ext]\n
[Bookmarks]\nHome={home_dir}\n'''

def main():
    parser = argparse.ArgumentParser(description="A filepicker with proper thumbnail support")
    parser.add_argument("-e", "--parent", help="window id of the window this one is transient to")
    parser.add_argument("-t", "--title", default="File Picker", help="title of the filepicker window")
    parser.add_argument("-m", "--mode", choices=['file', 'files', 'dir', 'save'], help="Mode of file selection. One of [file files dir save]")
    parser.add_argument("-p", "--path", default=os.getcwd(), help="path of initial directory")
    parser.add_argument("-i", "--mime_list", default=None, help="list of allowed mime types. Can be empty.")
    args = parser.parse_args()
    os.makedirs(cache_dir, exist_ok=True)
    
    picker = FilePicker(args)
    picker.run()

if __name__ == "__main__":
    main()
