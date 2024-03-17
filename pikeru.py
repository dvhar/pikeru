#!/usr/bin/env python
import argparse, configparser
import glob
import os, sys, time
import tkinter as tk
from PIL import Image, ImageTk
from tkinter.messagebox import askyesno
from tkinter import simpledialog, messagebox
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
import tkinter.font

SCALE = 1

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
        self.allowed_mimes = set(args.mime_list.strip().split(' ')) if args.mime_list else None
        self.enable_mime_filtering = self.allowed_mimes != None
        self.ino = inotify.adapters.Inotify()
        self.watch_thread = threading.Thread(target=self.watch_loop, daemon=True)
        self.watch_thread.start()
        self.already_added = set()
        self.items = []
        self.config()

        self.root = TkinterDnD.Tk()
        self.widgetfont = tkinter.font.Font(family="Helvetica", size=12)
        self.itemfont = tkinter.font.Font(family="Helvetica", size=10)
        self.root.geometry(f'{self.INIT_WIDTH}x{self.INIT_HEIGHT}')
        self.root.tk.call('tk','scaling',SCALE)
        self.root.wm_title(args.title or 'File Picker')
        x = (self.root.winfo_screenwidth() / 2) - (self.INIT_WIDTH / 2)
        y = (self.root.winfo_screenheight() / 2) - (self.INIT_HEIGHT / 2)
        self.root.geometry(f'+{int(x)}+{int(y)}')
        self.frame = tk.Frame(self.root, **kwargs)
        self.frame.grid_columnconfigure(0, weight=1)
        self.frame.grid_rowconfigure(0, weight=0)
        self.frame.grid_rowconfigure(1, weight=1)

        self.root.drop_target_register(DND_FILES, DND_TEXT)
        self.root.dnd_bind('<<Drop>>', self.drop_data)

        lower_frame = tk.Frame(self.frame)
        lower_frame.grid(row=1, column=0, sticky='news')
        lower_frame.grid_columnconfigure(0, weight=0)
        lower_frame.grid_columnconfigure(1, weight=1)
        lower_frame.grid_rowconfigure(0, weight=1)

        upper_frame = tk.Frame(self.frame)
        upper_frame.grid(row=0, column=0, sticky='news')
        upper_frame.grid_columnconfigure(0, weight=1)

        self.bookmark_frame = tk.Frame(lower_frame)
        self.bookmark_frame.grid(row=0, column=0, sticky='news')
        self.canvas = tk.Canvas(lower_frame)
        self.canvas.grid(row=0, column=1, sticky='news')
        self.scrollbar = tk.Scrollbar(lower_frame, orient='vertical', command=self.canvas.yview)
        self.scrollbar.grid(row=0, column=2, sticky='ns')
        self.canvas.configure(yscrollcommand=self.scrollbar.set)

        self.items_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_listeners(self.canvas)
        self.bind_listeners(self.items_frame)

        self.path_textfield = tk.Entry(upper_frame, insertbackground='red', font=self.widgetfont)
        self.path_textfield.grid(row=1, column=0, padx=(10, 0), pady=(1, 0), sticky='ew')
        self.path_textfield.insert(0, args.path)
        self.path_textfield.bind("<Return>", self.on_type_enter)

        self.button_frame = tk.Frame(upper_frame)
        self.button_frame.grid(row=0, column=0, sticky='we')
        button_text = "Save" if self.select_save else "Open"
        self.open_button = tk.Button(self.button_frame, width=10, text=button_text, command=self.on_select_button, font=self.widgetfont)
        self.open_button.pack(side='right')
        self.cancel_button = tk.Button(self.button_frame, width=10, text="Cancel", command=self.root.destroy, font=self.widgetfont)
        self.cancel_button.pack(side='right')
        self.up_dir_button = tk.Button(self.button_frame, width=7, text="Up Dir", command=self.on_up_dir, font=self.widgetfont)
        self.up_dir_button.pack(side='right')
        self.new_dir_button = tk.Button(self.button_frame, width=7, text="New Dir", command=self.create_directory, font=self.widgetfont)
        self.new_dir_button.pack(side='right')
        self.sort_button = tk.Button(self.button_frame, width=7, text="Sort", command=self.show_sort_menu, font=self.widgetfont)
        self.sort_button.pack(side='right')
        self.root.bind("<Button-1>", self.withdraw_menus)
        self.cmd_button = tk.Button(self.button_frame, width=7, text="Cmd", command=self.show_cmd_menu, font=self.widgetfont)
        self.cmd_button.pack(side='right')
        self.cmd_menu = tk.Menu(self.root, tearoff=False, font=self.widgetfont)
        for cmd_name, cmd_val in self.commands.items():
            self.cmd_menu.add_command(label=cmd_name, command=lambda cmd=cmd_val: self.run_cmd(cmd))

        self.size_label = tk.Label(self.button_frame, text='', font=self.widgetfont)
        self.size_label.pack(side='left')

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
        max_width = self.INIT_WIDTH - self.bookmark_frame.winfo_width()
        self.max_cols = max(1, int(max_width / (self.THUMBNAIL_WIDTH+6)))
        self.folder_icon = get_asset('folder.png')
        self.doc_icon = get_asset('document.png')
        self.unknown_icon = get_asset('unknown.png')
        self.error_icon = get_asset('error.png')
        self.prev_sel = []

        for i, (name, path) in enumerate(self.bookmarks.items()):
            btn = tk.Button(self.bookmark_frame, text=name, font=self.widgetfont)
            btn.path = path
            btn.grid(row=i, column=0, sticky='news')
            btn.bind("<Button-1>", lambda e: self.change_dir(e.widget.path))

        self.frame.pack(fill='both', expand=True)
        self.change_dir(args.path)

    def withdraw_menus(self, event):
        if hasattr(self, 'sort_popup') and self.sort_popup.winfo_exists():
            self.sort_popup.unpost()
        if hasattr(self, 'cmd_menu') and self.cmd_menu.winfo_exists():
            self.cmd_menu.unpost()

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
            self.already_added.add(filepath)
            with open(filepath, 'wb') as f:
                f.write(response.content)
            item = PathInfo(filepath)
            item.idx = len(self.items)
            self.items.append(None)
            self.load_item(item)
            self.on_click_file(FakeEvent(self.items[-1]))

    def create_directory(self):
        new_dir_name = simpledialog.askstring("New Directory", "Enter the name of the new directory:")
        if new_dir_name:
            new_dir_path = os.path.join(os.getcwd(), new_dir_name)
            try:
                os.mkdir(new_dir_path)
            except FileExistsError:
                messagebox.showerror("Error", f"Directory '{new_dir_name}' already exists.")
            except OSError as e:
                messagebox.showerror("Error", f"Failed to create directory due to error: {e}")

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
            self.load_item(self.queue.get())

    def prep_file(self, label, item_path):
        label.sel = False
        label.path = item_path
        if item_path.idx >= len(self.items):
            return
        self.items[item_path.idx] = label
        self.bind_listeners(label)
        if not self.select_dir:
            label.bind("<Button-1>", self.on_click_file)
            label.bind("<Double-Button-1>", self.on_double_click_file)
        if os.path.dirname(item_path) == os.getcwd():
            label.grid(row=item_path.idx//self.max_cols, column=item_path.idx%self.max_cols)

    def prep_dir(self, label, item_path):
        if item_path.idx >= len(self.items):
            return
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
        name = base_path if len(base_path) < 20 else '...'+base_path[len(base_path)-17:]
        try:
            if os.path.isfile(item_path):
                ext = os.path.splitext(base_path)[-1].lower()
                match ext:
                    case '.png'|'.jpg'|'.jpeg'|'.gif'|'.webp':
                        img = self.prepare_cached_thumbnail(item_path, 'pic')
                        label = tk.Label(self.items_frame, image=img, text=name, compound='top', font=self.itemfont)
                        label.__setattr__('img', img)
                        label.bind("<Button-3>", lambda e:self.on_view_image(e, True))
                        self.prep_file(label, item_path)
                    case '.mp4'|'.avi'|'.mkv'|'.webm':
                        img = self.prepare_cached_thumbnail(item_path, 'vid')
                        label = tk.Label(self.items_frame, image=img, text=name, compound='top', font=self.itemfont)
                        label.__setattr__('img', img)
                        label.__setattr__('vid', True)
                        label.bind("<Button-3>", lambda e:self.on_view_image(e, True))
                        self.prep_file(label, item_path)
                    case '.txt'|'.pdf'|'.doc'|'.docx':
                        label = tk.Label(self.items_frame, image=self.doc_icon, text=name, compound='top', font=self.itemfont)
                        label.__setattr__('img', self.doc_icon)
                        self.prep_file(label, item_path)
                    case _:
                        label = tk.Label(self.items_frame, image=self.unknown_icon, text=name, compound='top', font=self.itemfont)
                        label.__setattr__('img', self.unknown_icon)
                        self.prep_file(label, item_path)
            elif os.path.isdir(item_path):
                label = tk.Label(self.items_frame, image=self.folder_icon, text=name, compound='top', font=self.itemfont)
                self.prep_dir(label, item_path)
            else:
                return
        except Exception as e:
            label = tk.Label(self.items_frame, image=self.error_icon, text=name, compound='top', font=self.itemfont)
            label.__setattr__('img', self.unknown_icon)
            self.prep_file(label, item_path)
            label.__setattr__('path', item_path)
            label.path.mime = 'application/octet-stream'
            sys.stderr.write(f'Error loading item: {e}\t{item_path}\n')

    def prepare_cached_thumbnail(self, item_path, imtype):
        md5hash = hashlib.md5(item_path.encode()).hexdigest()
        cache_path = os.path.join(cache_dir, f'{md5hash}{SCALE}.png')
        if os.path.isfile(cache_path):
            img = Image.open(cache_path)
            img = ImageTk.PhotoImage(img)
            return img
        else:
            if imtype == 'pic':
                img = Image.open(item_path)
                img.thumbnail((self.THUMBNAIL_WIDTH, self.THUMBNAIL_HEIGHT))
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
                img.thumbnail((self.THUMBNAIL_WIDTH, self.THUMBNAIL_HEIGHT))
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
        new_bookmark = tk.Button(self.bookmark_frame, text=basename, command=lambda: self.change_dir(path), font=self.widgetfont)
        new_bookmark.path = path
        new_bookmark.grid(row=len(bookmarks), column=0, sticky='news')
        self.bookmark_frame.update_idletasks()
        with open(config_file, 'a') as f:
            f.write(f'{basename}={path}\n')

    def reorganize_items(self):
        num_rows = len(self.items) // self.max_cols + (1 if len(self.items) % self.max_cols != 0 else 0)
        for row in range(num_rows):
            start = row * self.max_cols
            for col in range(self.max_cols):
                idx = start + col
                if idx < len(self.items) and self.items[idx]:
                    self.items[idx].grid(row=row, column=col)

    def on_click_file(self, event):
        label = event.widget
        shift = event.state & 0x1
        ctrl = event.state & 0x4
        # select a whole range
        if self.select_multi and shift and len(self.prev_sel) > 0:
            lo = hi = label.path.idx
            for item in self.prev_sel:
                lo = min(item.path.idx, lo)
                hi = max(item.path.idx, hi)
            for i in range(lo, hi+1):
                if not self.items[i].sel:
                    item = self.items[i]
                    item.sel = True
                    cfg = item.cget('background')
                    item.origbg = cfg
                    item.config(bg='red')
                    self.prev_sel.append(item)
            return
        # toggle clicked item on or off
        if label.sel == False:
            cfg = label.cget('background')
            label.origbg = cfg
            label.config(bg='red')
            label.sel = True
        elif len(self.prev_sel) == 1 or ctrl:
            label.config(bg=label.origbg)
            label.sel = False
            self.path_textfield.delete(0, 'end')
            self.path_textfield.insert(0, os.getcwd())
            if ctrl:
                self.prev_sel = [ps for ps in self.prev_sel if ps.path != label.path]
        # clear previous selections if normal click
        if not (self.select_multi and ctrl):
            for ps in self.prev_sel:
                if ps.path != label.path:
                    ps.sel = False
                    try:
                        ps.config(bg=label.origbg)
                    except:
                        pass
            self.prev_sel = []
        # handle newly selected file or clear textbox
        if label.sel:
            self.prev_sel.append(label)
            self.path_textfield.delete(0, 'end')
            self.path_textfield.insert(0, label.path)
            if os.path.isfile(label.path):
                self.size_label.configure(text=get_size(label.path))
        else:
            self.size_label.configure(text='')

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
        resized = img.resize((int(img.width * m), int(img.height * m)))
        resized_photo = ImageTk.PhotoImage(resized)
        big_image = tk.Label(self.canvas, image=resized_photo, bd=0)
        big_image.img = resized_photo
        big_image.orig = img
        big_image.path = label.path
        if goback:
            self.current_y = self.canvas.yview()[0]
        self.canvas.yview_moveto(0)
        self.canvas.delete("all")
        self.canvas.unbind("<Button>")
        self.canvas.bind("<Button-4>", lambda _:self.on_scroll_image(FakeEvent(big_image,4)))
        self.canvas.bind("<Button-5>", lambda _:self.on_scroll_image(FakeEvent(big_image,5)))
        x_pos = (self.canvas.winfo_width() - big_image.winfo_width()) // 2
        y_pos = (self.canvas.winfo_height() - big_image.winfo_height()) // 2
        self.canvas.create_window(x_pos, y_pos, window=big_image, anchor='center')
        big_image.bind("<Button-2>", self.close_expanded_image)
        big_image.bind("<Button-3>", self.close_expanded_image)
        big_image.bind("<Button-4>", self.on_scroll_image)
        big_image.bind("<Button-5>", self.on_scroll_image)
        big_image.bind("<Double-Button-1>",lambda _: self.on_double_click_file(event))
        self.__setattr__('bigimg', big_image)
        if not event.widget.sel:
            self.on_click_file(event)
            self.unselect = event
        else:
            self.unselect = None

    def on_scroll_image(self, event):
        step = -1 if event.num==4 else 1
        ctrl_pressed = event.state & 0x4
        if ctrl_pressed:
            zoom_factor = 1.1 if step < 0 else 1/1.1
            self.resize_image(zoom_factor)
            return
        idx = event.widget.path.idx
        inrange = (lambda i: i > 0) if step == -1 else (lambda i: i<(len(self.items)-1))
        nextimage = None
        while inrange(idx):
            idx += step
            item = self.items[idx]
            if not hasattr(item.path, 'mime') or not (item.path.mime.startswith('image') or item.path.mime.startswith('video')):
                continue
            nextimage = item
            break
        if nextimage:
            self.on_view_image(FakeEvent(nextimage), False)

    def resize_image(self, factor):
        prev: ImageTk.PhotoImage = self.bigimg.img
        new_width = int(prev.width() * factor)
        new_height = int(prev.height() * factor)
        resized_pil_img = self.bigimg.orig.resize((new_width, new_height))
        resized_img = ImageTk.PhotoImage(resized_pil_img)
        self.bigimg.config(image=resized_img)
        self.bigimg.img = resized_img

    def close_expanded_image(self, event):
        self.bigimg.destroy()
        del self.bigimg
        self.canvas.yview_moveto(self.current_y)
        self.items_frame.grid()
        self.canvas.delete("all")
        self.canvas.create_window(0, 0, window=self.items_frame, anchor='nw')
        self.canvas.bind("<Button>", self.mouse_nav)
        self.canvas.unbind("<Button-4>")
        self.canvas.unbind("<Button-5>")
        if self.unselect:
            self.on_click_file(self.unselect)
            self.unselect = None

    def watch_loop(self):
        for e in self.ino.event_gen():
            if e:
                path, file = e[2], e[3]
                filepath = os.path.join(path, file)
                if not self.mime_is_allowed(filepath) or filepath in self.already_added:
                    return
                time.sleep(0.5)
                item = PathInfo(filepath)
                item.idx = len(self.items)
                self.items.append(None)
                self.load_item(item)

    def change_dir(self, new_dir):
        if self.select_save and not os.path.isdir(new_dir):
            new_dir = os.path.dirname(new_dir)
        if os.path.isdir(new_dir):
            self.ino.remove_watch(os.getcwd())
            self.ino.add_watch(new_dir, mask=inotify.constants.IN_CREATE)
            self.prev_sel = []
            os.chdir(new_dir)
            self.path_textfield.delete(0, 'end')
            if self.save_filename:
                new_dir += '/' + self.save_filename
            self.path_textfield.insert(0, new_dir)
            while self.queue.qsize() > 0:
                self.queue.get()
            self.already_added.clear()
            self.load_dir()
            self.canvas.yview_moveto(0)

    def on_up_dir(self):
        self.change_dir(os.path.dirname(os.getcwd()))

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
        if hasattr(self, 'bigimg'):
            self.close_expanded_image(None)
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
        self.sort_popup = tk.Menu(self.root, tearoff=False, font=self.widgetfont)
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
        max_width = self.frame.winfo_width() - self.bookmark_frame.winfo_width()
        self.max_cols = max(1, int(max_width / (self.THUMBNAIL_WIDTH+6))) # figure out proper width calculation
        if old != self.max_cols:
            self.reorganize_items()

    def run_cmd(self, cmd: str):
        selected_items = [label.path for label in self.items if label.sel]
        for item_path in selected_items:
            base_name = os.path.basename(item_path)
            directory = os.path.dirname(item_path)
            part, ext = os.path.splitext(base_name) if os.path.isfile(item_path) else (base_name,'')
            cmd = cmd.replace('[path]', f'"{item_path}"')
            cmd = cmd.replace('[name]', f'"{base_name}"')
            cmd = cmd.replace('[ext]', ext)
            cmd = cmd.replace('[dir]', f'"{directory}"')
            cmd = cmd.replace('[part]', f'"{part}"')
            proc = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stdout, stderr = proc.communicate()
            print(cmd, file=sys.stderr)
            if stderr:
                print(stderr, file=sys.stderr)
            if stdout:
                print(stdout, file=sys.stderr)

    def show_cmd_menu(self):
        self.cmd_menu.post(self.cmd_button.winfo_rootx(), self.cmd_button.winfo_rooty())

    def config(self):
        config = CaseConfigParser()
        config.read(os.path.expanduser(config_file))
        try:
            self.bookmarks = config['Bookmarks']
            self.commands = config['Commands']
            global SCALE
            SCALE = float(config.get('Settings','dpi_scale'))
        except Exception as e:
            print(e, file=sys.stderr)
            print(f'updated config file - backing up to {config_file}.old', file=sys.stderr)
            os.rename(config_file, config_file+'.old')
            writeconfig(config)
        self.INIT_WIDTH = int(1024*SCALE)
        self.INIT_HEIGHT = int(720*SCALE)
        self.THUMBNAIL_WIDTH = int(140*SCALE)
        self.THUMBNAIL_HEIGHT = int(140*SCALE)

def get_asset(file):
    img = Image.open(os.path.join(asset_dir, file))
    if SCALE != 1:
        img = img.resize((int(img.width * SCALE), int(img.height * SCALE)))
    img = ImageTk.PhotoImage(img)
    return img

def get_size(path):
    size = os.path.getsize(path)
    if size > 1073741824:
        return f'{size//1073741824}GB'
    if size > 1048576:
        return f'{size//1048576}MB'
    if size > 1024:
        return f'{size//1024}KB'
    return f'{size}B'

class FakeEvent:
    def __init__(self, widget, num=0):
        self.widget = widget
        self.num = num
        self.state = 0

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


def writeconfig(oldvals: CaseConfigParser|None = None):
    if os.path.isfile(config_file):
        return
    with open(config_file, 'w') as f:
        print(f'writing config to {config_file}', file=sys.stderr)
        confcomment = '''# Commands from the cmd menu will substitute the follwong values from the selected files before running, as seen in the convert examples. All paths and filenames are already quoted for you.
# [path] is full file path
# [name] is the filename without full path
# [dir] is the current directory without trailing slash
# [part] is the filename without path or extension
# [ext] is the file extension, including the period
'''
        def getsection(name, default):
            it = oldvals[name] if oldvals and oldvals.has_section(name) else default
            f.write(f'[{name}]\n')
            f.writelines(f'{v[0]} = {v[1]}\n' for v in it.items())
            f.write('\n')
        cmds = {'resize':'convert -resize 1200 [path] [part]_resized[ext]',
                'convert webp':'convert [path] [part].jpg'}
        sets = {'dpi_scale':'1'}
        bkmk = {'Home':home_dir}
        bkmk.update({k:os.path.join(home_dir,k) for k in ["Documents", "Pictures", "Downloads"]})
        f.write(confcomment)
        getsection('Commands', cmds)
        getsection('Settings', sets)
        getsection('Bookmarks', bkmk)

def main():
    parser = argparse.ArgumentParser(description="A filepicker with proper thumbnail support")
    parser.add_argument("-e", "--parent", help="window id of the window this one is transient to")
    parser.add_argument("-t", "--title", default="File Picker", help="title of the filepicker window")
    parser.add_argument("-m", "--mode", choices=['file', 'files', 'dir', 'save'], help="Mode of file selection. One of [file files dir save]")
    parser.add_argument("-p", "--path", default=os.getcwd(), help="path of initial directory")
    parser.add_argument("-i", "--mime_list", default=None, help="list of allowed mime types. Can be empty.")
    args = parser.parse_args()
    os.makedirs(cache_dir, exist_ok=True)
    writeconfig()
    
    picker = FilePicker(args)
    picker.run()

if __name__ == "__main__":
    main()
