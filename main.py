#!/usr/bin/env python
import argparse, configparser
import os, sys, stat
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
import sv_ttk
SCALE = 1

# https://icon-icons.com
asset_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'assets')
home_dir = os.environ['HOME']
config_file = os.path.join(home_dir,'.config','pikeru.conf')
cache_dir = os.path.join(home_dir,'.cache','pikeru')

butstyle = 'solid'
bd = 1

class FilePicker():
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
        self.items: list[tk.Label] = []
        self.dir_history = []
        self.show_hidden = False
        self.multidir = None
        self.nav_id = 0
        self.last_clicked = 0
        self.read_config()

        self.root = TkinterDnD.Tk()
        self.root.geometry(f'{self.INIT_WIDTH}x{self.INIT_HEIGHT}')
        self.root.tk.call('tk','scaling',SCALE)
        if self.THEME != 'none':
            sv_ttk.set_theme(self.THEME)
        self.root.drop_target_register(DND_FILES, DND_TEXT)
        self.root.dnd_bind('<<Drop>>', self.drop_data)
        self.widgetfont = tkinter.font.Font(family="Helvetica", size=12)
        self.itemfont = tkinter.font.Font(family="Helvetica", size=9)
        self.root.wm_title(args.title or 'File Picker')
        x = (self.root.winfo_screenwidth() / 2) - (self.INIT_WIDTH / 2)
        y = (self.root.winfo_screenheight() / 2) - (self.INIT_HEIGHT / 2)
        self.root.geometry(f'+{int(x)}+{int(y)}')
        self.root.bind('<Left>', self.on_key_press)
        self.root.bind('<Right>', self.on_key_press)
        self.root.bind('<Up>', self.on_key_press)
        self.root.bind('<Down>', self.on_key_press)
        self.root.bind('<Return>', self.on_select_button)

        self.frame = tk.Frame(self.root, **kwargs)
        self.frame.grid_columnconfigure(0, weight=1)
        self.frame.grid_rowconfigure(0, weight=0)
        self.frame.grid_rowconfigure(1, weight=1)

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
        self.canvas.bind('<Button-1>', self.deselect_all)
        self.items_frame.bind('<Button-1>', self.deselect_all)

        self.path_textfield = tk.Entry(upper_frame, insertbackground='red', font=self.widgetfont)
        self.path_textfield.grid(row=1, column=0, padx=(10, 0), pady=(1, 0), sticky='ew')
        self.path_textfield.insert(0, args.path)
        self.path_textfield.bind("<Return>", self.on_type_enter)

        self.button_frame = tk.Frame(upper_frame)
        self.button_frame.grid(row=0, column=0, sticky='we')
        button_text = "Save" if self.select_save else "Open"
        self.open_button = tk.Button(self.button_frame,
                                     bd=bd, relief=butstyle, width=10, text=button_text,
                                     command=lambda:self.on_select_button(None), font=self.widgetfont)
        self.open_button.pack(side='right')
        self.cancel_button = tk.Button(self.button_frame,
                                       bd=bd, relief=butstyle, width=10, text="Cancel", command=self.root.destroy, font=self.widgetfont)
        self.cancel_button.pack(side='right')
        self.up_dir_button = tk.Button(self.button_frame,
                                       bd=bd, relief=butstyle, width=7, text="Up Dir", command=self.on_up_dir, font=self.widgetfont)
        self.up_dir_button.pack(side='right')
        self.new_dir_button = tk.Button(self.button_frame,
                                        bd=bd, relief=butstyle, width=7, text="New Dir", command=self.create_directory, font=self.widgetfont)
        self.new_dir_button.pack(side='right')
        self.view_button = tk.Button(self.button_frame,
                                     bd=bd, relief=butstyle, width=7, text="View", command=self.show_view_menu, font=self.widgetfont)
        self.view_button.pack(side='right')
        self.root.bind("<Button-1>", self.withdraw_menus)
        self.cmd_button = tk.Button(self.button_frame,
                                    bd=bd, relief=butstyle, width=7, text="Cmd", command=self.show_cmd_menu, font=self.widgetfont)
        self.cmd_button.pack(side='right')
        self.cmd_menu = tk.Menu(self.root, tearoff=False, font=self.widgetfont)
        for cmd_name, cmd_val in self.commands.items():
            self.cmd_menu.add_command(label=cmd_name, command=lambda cmd=cmd_val: self.run_cmd(cmd))

        self.size_label = tk.Label(self.button_frame, text='', font=self.widgetfont)
        self.size_label.pack(side='left')

        if self.enable_mime_filtering:
            self.mime_switch_btn = tk.Checkbutton(self.button_frame,
                  text="Filter mime", command=self.toggle_mime_filter, font=self.widgetfont)
            self.mime_switch_btn.pack(side='left')

        self.in_queue = queue.Queue()
        self.out_queue = queue.Queue()
        self.threads = []
        for i in range(cpu_count()):
            loading_thread = threading.Thread(target=self.load_items, daemon=True)
            loading_thread.start()
            self.threads.append(loading_thread)

        self.frame.bind('<Configure>', self.on_resize)
        max_width = self.frame.winfo_width() - self.bookmark_frame.winfo_width() - self.scrollbar.winfo_width()
        self.max_cols = max(1, max_width // (self.THUMBNAIL_SIZE+4))
        self.folder_icon = get_asset('folder.png')
        self.doc_icon = get_asset('document.png')
        self.unknown_icon = get_asset('unknown.png')
        self.error_icon = get_asset('error.png')
        self.prev_sel: list[tk.Label] = []

        for i, (name, path) in enumerate(self.bookmarks.items()):
            btn = tk.Button(self.bookmark_frame, text=name, font=self.widgetfont, relief=butstyle, bd=bd)
            btn.path = path
            btn.grid(row=i, column=0, sticky='news')
            btn.bind("<Button-1>", lambda e: self.change_dir(e.widget.path))

        self.frame.pack(fill='both', expand=True)
        self.root.after(0, self.thumb_listener)
        self.change_dir(args.path)

    def withdraw_menus(self, event):
        if hasattr(self, 'view_menu') and self.view_menu.winfo_exists():
            self.view_menu.destroy()
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
        url: str = event.data
        tries = 2
        while tries > 0:
            if url.startswith('http://') or url.startswith('https://'):
                try:
                    print('url:', url, file=sys.stderr)
                    response = requests.get(url, allow_redirects=True, timeout=5)
                    print('status code:', response.status_code, file=sys.stderr)
                    filename = os.path.basename(url.split('?')[0])
                    filepath = os.path.join(os.getcwd(), filename)
                    if response.status_code < 300:
                        self.already_added.add(filepath)
                        with open(filepath, 'wb') as f:
                            f.write(response.content)
                        item = PathInfo(filepath)
                        item.idx = len(self.items)
                        item.nav_id = self.nav_id
                        self.items.append(None)
                        self.load_item(item)
                        self.on_click_file(FakeEvent(self.items[-1]))
                        break
                    url = url.split('?')[0]
                    tries -= 1
                except:
                    url = url.split('?')[0]
                    tries -= 1

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
            case 9: self.on_down_dir(event)

    def bind_listeners(self, thing):
        thing.bind('<Button>', self.mouse_nav)

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def load_items(self):
        while True:
            self.load_item(self.in_queue.get())

    def thumb_listener(self):
        try:
            name, path, img, ftype = self.out_queue.get(block=False)
            if path.nav_id != self.nav_id or path.idx >= len(self.items):
                self.root.after(0, self.thumb_listener)
                return
            label = tk.Label(self.items_frame, image=img, text=name, compound='top', font=self.itemfont, width=self.THUMBNAIL_SIZE)
            match ftype:
                case 1: # img
                    label.__setattr__('img', img)
                    label.bind("<Button-3>", lambda e:self.on_view_image(e, True))
                    self.prep_file(label, path)
                case 2: # vid
                    label.__setattr__('img', img)
                    label.__setattr__('vid', True)
                    label.bind("<Button-3>", lambda e:self.on_view_image(e, True))
                    self.prep_file(label, path)
                case 3: # doc or uncategorized
                    self.prep_file(label, path)
                case 4: # directory
                    self.prep_dir(label, path)
                case 5: # error
                    label.__setattr__('path', path)
                    label.path.mime = 'application/octet-stream'
                    self.prep_file(label, path)
            self.root.after(0, self.thumb_listener)
        except:
            self.root.after(100, self.thumb_listener)

    def prep_file(self, label, path):
        if path.idx >= len(self.items):
            return
        label.sel = False
        label.path = path
        self.items[path.idx] = label
        self.bind_listeners(label)
        if not self.select_dir:
            label.bind("<Button-1>", self.on_click_file)
            label.bind("<Button-2>", lambda _:self.on_click_file(FakeEvent(label, state=0x4)))
            label.bind("<Double-Button-1>", self.on_double_click_file)
        if path.nav_id == self.nav_id:
            label.grid(row=path.idx//self.max_cols, column=path.idx%self.max_cols)

    def prep_dir(self, label, path):
        if path.idx >= len(self.items):
            return
        label.path = path
        label.sel = False
        self.items[path.idx] = label
        self.bind_listeners(label)
        label.bind("<Double-Button-1>", self.on_double_click_dir)
        label.bind("<Button-1>", self.on_click_file)
        label.bind("<Button-2>", lambda _:self.on_click_file(FakeEvent(label, state=0x4)))
        label.bind("<Button-3>", lambda _:self.on_click_file(FakeEvent(label, state=0x1)))
        label.bind("<ButtonRelease-1>", self.on_drag_dir_end)
        if path.nav_id == self.nav_id:
            label.grid(row=path.idx//self.max_cols, column=path.idx%self.max_cols)

    def load_item(self, path):
        base_path = os.path.basename(path)
        name = base_path if len(base_path) < 20 else '...'+base_path[len(base_path)-17:]
        try:
            if path.isfile:
                ext = os.path.splitext(base_path)[-1].lower()
                match ext:
                    case '.png'|'.jpg'|'.jpeg'|'.gif'|'.webp'|'.tiff'|'.bmp'|'.ppm':
                        img = self.prepare_cached_thumbnail(path, 'pic', ext)
                        self.out_queue.put((name, path, img, 1))
                    case '.mp4'|'.avi'|'.mkv'|'.webm':
                        img = self.prepare_cached_thumbnail(path, 'vid', '.jpg')
                        self.out_queue.put((name, path, img, 2))
                    case '.txt'|'.pdf'|'.doc'|'.docx':
                        self.out_queue.put((name, path, self.doc_icon, 3))
                    case _:
                        self.out_queue.put((name, path, self.unknown_icon, 3))
            elif os.path.isdir(path):
                self.out_queue.put((name, path, self.folder_icon, 4))
            else:
                return
        except Exception as e:
            self.out_queue.put((name, path, self.error_icon, 5))
            sys.stderr.write(f'Error loading item: {e}\t{path}\n')

    def prepare_cached_thumbnail(self, path, imtype, ext):
        md5hash = hashlib.md5(path.encode()).hexdigest()
        cache_path = os.path.join(cache_dir, f'{md5hash}{self.THUMBNAIL_SIZE}{ext}')
        try:
            st = os.stat(cache_path)
        except:
            st = None
        if st and stat.S_ISREG(st.st_mode) and st.st_mtime > path.time:
            img = Image.open(cache_path)
            img = ImageTk.PhotoImage(img)
            return img
        elif os.path.dirname(path) == cache_dir:
            img = Image.open(path)
            img.thumbnail((self.THUMBNAIL_SIZE, self.THUMBNAIL_SIZE))
            img = ImageTk.PhotoImage(img)
            return img
        else:
            if imtype == 'pic':
                img = Image.open(path)
                img.thumbnail((self.THUMBNAIL_SIZE, self.THUMBNAIL_SIZE))
                img.save(cache_path)
                img = ImageTk.PhotoImage(img)
                return img
            else:
                cap = cv2.VideoCapture(path)
                ret, frame = cap.read()
                cap.release()
                if not ret:
                    return self.error_icon
                frame = cv2.cvtColor(frame, cv2.COLOR_BGR2RGB)
                img = Image.fromarray(frame)
                img.thumbnail((self.THUMBNAIL_SIZE, self.THUMBNAIL_SIZE))
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
                    self.items[idx].path.idx = idx

    def on_key_press(self, event):
        if len(self.prev_sel) < 1:
            return
        state = event.state & 7
        mc = self.max_cols
        idx = 0
        match event.keysym:
            case 'Up':
                idx = self.last_clicked - mc
            case 'Down':
                idx = self.last_clicked + mc
            case 'Left':
                idx = self.last_clicked - 1
            case 'Right':
                idx = self.last_clicked + 1
        if idx < 0 or idx >= len(self.items):
            return
        self.on_click_file(FakeEvent(self.items[idx], state=state))

    def deselect_all(self, e):
        for ps in self.prev_sel:
            ps.config(bg=ps.origbg)
            ps.sel = False

    def on_click_file(self, event, noscroll=False):
        clicked: tk.Label = event.widget
        # scroll to keep selection in view
        cvy1, cvy2 = self.canvas.yview() if not noscroll else self.current_y
        dif = cvy2-cvy1
        ifh = self.items_frame.winfo_height()
        vp1, vp2 = ifh * cvy1, ifh *cvy2
        cly1, cly2 = clicked.winfo_y(), clicked.winfo_y()+clicked.winfo_height()
        newvp1, newvp2 = cly1/ifh, cly2/ifh
        if noscroll:
            if cly1 < vp1:
                self.current_y = (newvp1, newvp1+dif)
            elif cly2 > vp2:
                self.current_y = (newvp2-dif, newvp2)
        else:
            if cly1 < vp1:
                self.canvas.yview_moveto(newvp1)
            elif cly2 > vp2:
                self.canvas.yview_moveto(newvp2-dif)
        self.last_clicked = clicked.path.idx
        shift = event.state & 0x1
        ctrl = event.state & 0x4
        isdir = clicked.path.isdir
        # select a whole range of same type. Always allow multi if dir
        while (self.select_multi or isdir) and shift and len(self.prev_sel) > 0:
            prevdir = self.prev_sel[0].path.isdir
            if prevdir != isdir:
                break
            lo = hi = clicked.path.idx
            for item in self.prev_sel:
                lo = min(item.path.idx, lo)
                hi = max(item.path.idx, hi)
            for i in range(lo, hi+1):
                item = self.items[i]
                if not item.sel and item.path.isdir == prevdir:
                    item.sel = True
                    cfg = item.cget('background')
                    item.origbg = cfg
                    item.config(bg='#800000')
                    self.prev_sel.append(item)
            return
        # select a new item
        if clicked.sel == False:
            cfg = clicked.cget('background')
            clicked.origbg = cfg
            clicked.config(bg='#800000')
            clicked.sel = True
        # deselect a selected item
        elif len(self.prev_sel) == 1 or ctrl:
            clicked.config(bg=clicked.origbg)
            clicked.sel = False
            self.path_textfield.delete(0, 'end')
            self.path_textfield.insert(0, os.getcwd())
            if ctrl:
                self.prev_sel = [ps for ps in self.prev_sel if ps.path != clicked.path]
        def prune_different_types():
            prune = any(ps for ps in self.prev_sel if ps.path.isdir != clicked.path.isdir)
            if prune:
                prevsel = []
                for ps in self.prev_sel:
                    if ps.path.isdir != clicked.path.isdir:
                        ps.config(bg=ps.origbg)
                        ps.sel = False
                    else:
                        prevsel.append(ps)
                self.prev_sel = prevsel
        # clear previous selections if normal click
        if not (self.select_multi and ctrl):
            if isdir and ctrl:
                prune_different_types()
            else:
                for ps in self.prev_sel:
                    if ps.path != clicked.path:
                        ps.sel = False
                        try:
                            ps.config(bg=clicked.origbg)
                        except:
                            pass
                self.prev_sel = []
        else:
            prune_different_types()
        # handle newly selected file or clear textbox
        if clicked.sel:
            self.prev_sel.append(clicked)
            self.path_textfield.delete(0, 'end')
            self.path_textfield.insert(0, clicked.path)
            if clicked.path.isfile:
                self.size_label.configure(text=get_size(clicked.path))
        else:
            self.size_label.configure(text='')

    def image_binds(self, event):
        match event.num:
            case 2: self.close_expanded_image(event)
            case 3: self.close_expanded_image(event)
            case 4: self.on_scroll_image(event)
            case 5: self.on_scroll_image(event)
            case 8: self.close_expanded_image(event)

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
            self.current_y = self.canvas.yview()
        self.canvas.yview_moveto(0)
        self.canvas.delete("all")
        self.canvas.unbind("<Button>")
        self.canvas.bind("<Button-4>", lambda _:self.on_scroll_image(FakeEvent(big_image, num=4)))
        self.canvas.bind("<Button-5>", lambda _:self.on_scroll_image(FakeEvent(big_image, num=5)))
        self.root.unbind('<Left>')
        self.root.unbind('<Right>')
        self.root.bind('<Left>', lambda _:self.on_scroll_image(FakeEvent(big_image, num=4)))
        self.root.bind('<Right>', lambda _:self.on_scroll_image(FakeEvent(big_image, num=5)))
        x_pos = (self.canvas.winfo_width() - big_image.winfo_width()) // 2
        y_pos = (self.canvas.winfo_height() - big_image.winfo_height()) // 2
        self.canvas.create_window(x_pos, y_pos, window=big_image, anchor='center')
        big_image.bind("<Button>", self.image_binds)
        big_image.bind("<Double-Button-1>",lambda _: self.on_double_click_file(event))
        self.__setattr__('bigimg', big_image)
        if not event.widget.sel:
            self.on_click_file(event, noscroll=True)

    def on_scroll_image(self, event):
        step = -1 if event.num==4 else 1
        ctrl = event.state & 0x4
        if ctrl:
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
        self.canvas.yview_moveto(self.current_y[0])
        self.items_frame.grid()
        self.canvas.delete("all")
        self.canvas.create_window(0, 0, window=self.items_frame, anchor='nw')
        self.canvas.bind("<Button>", self.mouse_nav)
        self.canvas.unbind("<Button-4>")
        self.canvas.unbind("<Button-5>")
        self.root.unbind('<Left>')
        self.root.unbind('<Right>')
        self.root.bind('<Left>', self.on_key_press)
        self.root.bind('<Right>', self.on_key_press)

    def load_newfile(self, path):
        path.idx = len(self.items)
        path.nav_id = self.nav_id
        self.items.append(None)
        self.load_item(path)

    def watch_loop(self):
        for e in self.ino.event_gen():
            if e:
                action, directory, file = e[1], e[2], e[3]
                try:
                    path = os.path.join(directory, file)
                    if 'IN_CREATE' in action:
                        path = PathInfo(path)
                        if not self.mime_is_allowed(path) or path in self.already_added:
                            return
                        self.root.after(500, self.load_newfile, path)
                    elif 'IN_DELETE' in action:
                        for item in self.items:
                            if item.path == path:
                                item.destroy()
                                break
                        self.items = [f for f in self.items if f.path != path]
                        self.reorganize_items()
                except Exception as e:
                    print('Error reading new file:', e, file=sys.stderr)

    def change_dir(self, new_dir, save=False):
        if self.select_save and not os.path.isdir(new_dir):
            new_dir = os.path.dirname(new_dir)
        if os.path.isdir(new_dir):
            cwd = os.getcwd()
            if save:
                self.dir_history.append(cwd)
            self.ino.remove_watch(cwd)
            self.ino.add_watch(new_dir, mask=inotify.constants.IN_CREATE|inotify.constants.IN_DELETE)
            self.prev_sel = []
            os.chdir(new_dir)
            self.path_textfield.delete(0, 'end')
            if self.save_filename:
                new_dir = os.path.join(new_dir, self.save_filename)
            self.path_textfield.insert(0, new_dir)
            while self.in_queue.qsize() > 0:
                self.in_queue.get()
            self.already_added.clear()
            self.load_dir()
            self.canvas.yview_moveto(0)

    def on_up_dir(self):
        self.change_dir(os.getcwd() if self.multidir else os.path.dirname(os.getcwd()), True)

    def on_down_dir(self, event):
        if self.multidir and hasattr(event.widget, 'path'):
            self.change_dir(os.path.dirname(event.widget.path))
        elif len(self.dir_history) > 0:
            self.change_dir(self.dir_history.pop())


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

    def on_select_button(self, event):
        selections = [label.path for label in self.prev_sel]
        if not self.select_dir and any(path.isdir for path in selections):
            if len(selections) == 1:
                self.change_dir(selections[0])
                return
            else:
                self.ino.remove_watch(os.getcwd())
                for new_dir in selections:
                    self.ino.add_watch(new_dir, mask=inotify.constants.IN_CREATE|inotify.constants.IN_DELETE)
                self.load_dir(selections)
                return
        if self.select_save and len(selections) == 0:
            self.final_selection(self.path_textfield.get())
        elif self.select_save:
            self.final_selection(selections[0])
        else:
            print('\n'.join(selections))
            self.root.destroy()

    def on_type_enter(self, event):
        txt = self.path_textfield.get()
        if os.path.isdir(txt):
            self.change_dir(txt)
        elif self.select_save and txt[-1] != '/':
            self.final_selection(txt)

    def ls(self, dirs: str|list) -> list:
        if isinstance(dirs, list):
            files = []
            for d in dirs:
                files += self.ls(d)
            return files
        ret = []
        for f in os.listdir(dirs):
            try:
                if f.startswith('.') and not self.show_hidden:
                    continue
                path = PathInfo(os.path.join(dirs,f))
                if (not self.enable_mime_filtering or self.mime_is_allowed(path)):
                    ret.append(path)
            except:
                pass
        ret.sort(key=self.get_sort_func(self.SORT, True), reverse=not self.SORT[1])
        return ret

    def load_dir(self, dirs = None):
        self.nav_id += 1
        if self.multidir:
            for prevdir in self.multidir:
                self.ino.remove_watch(prevdir)
        self.multidir = dirs
        if hasattr(self, 'bigimg'):
            self.close_expanded_image(None)
        self.items_frame.destroy()
        self.items_frame = tk.Frame(self.canvas)
        self.canvas.delete("all")
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_listeners(self.canvas)
        self.bind_listeners(self.items_frame)
        paths = self.ls(dirs if dirs else os.getcwd())
        self.items = [None] * len(paths)
        for i, path in enumerate(paths):
            path.idx = i
            path.nav_id = self.nav_id
            self.in_queue.put(path)

    def on_show(self):
        self.show_hidden = not self.show_hidden
        self.load_dir()

    def show_view_menu(self):
        show = 'Show' if not self.show_hidden else 'Hide'
        self.view_menu = tk.Menu(self.root, tearoff=False, font=self.widgetfont)
        self.view_menu.add_command(label="Sort name asc", command=lambda :self.on_sort('name', True))
        self.view_menu.add_command(label="Sort name desc", command=lambda :self.on_sort('name', False))
        self.view_menu.add_command(label="Sort oldest first", command=lambda :self.on_sort('time', True))
        self.view_menu.add_command(label="Sort newest first", command=lambda :self.on_sort('time', False))
        self.view_menu.add_command(label=f"{show} Hidden files", command=self.on_show)
        self.view_menu.post(self.view_button.winfo_rootx(),
                            self.view_button.winfo_rooty()+self.view_button.winfo_height())

    def get_sort_func(self, order, justpath):
        match order, justpath:
            case ('name', True), True: return lambda p: (not p.isdir, p.lname)
            case ('name', False),True: return lambda p: (p.isdir, p.lname)
            case ('time', True), True: return lambda p: (not p.isdir, p.time)
            case ('time', False),True: return lambda p: (p.isdir, p.time)
            case ('name', True), False: return lambda w: (not w.path.isdir, w.path.lname)
            case ('name', False),False: return lambda w: (w.path.isdir, w.path.lname)
            case ('time', True), False: return lambda w: (not w.path.isdir, w.path.time)
            case ('time', False),False: return lambda w: (w.path.isdir, w.path.time)
            case _: 
                print('sort_by must be one of [name_asc, name_desc, time_asc, time_desc]', file=sys.stderr)
                return None

    def on_sort(self, by, asc):
        sort = self.get_sort_func((by, asc), False)
        if not sort:
            return
        self.SORT = (by, asc)
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
        max_width = self.frame.winfo_width() - self.bookmark_frame.winfo_width() - self.scrollbar.winfo_width()
        self.max_cols = max(1, max_width // (self.THUMBNAIL_SIZE+4))
        if old != self.max_cols:
            self.reorganize_items()

    def run_cmd(self, cmdtemplate: str):
        for item in self.prev_sel:
            path = item.path
            base_name = os.path.basename(path)
            directory = os.path.dirname(path)
            part, ext = os.path.splitext(base_name) if path.isfile else (base_name,'')
            cmd = cmdtemplate
            cmd = cmd.replace('[path]', f'"{path}"')
            cmd = cmd.replace('[name]', f'"{base_name}"')
            cmd = cmd.replace('[ext]', ext)
            cmd = cmd.replace('[dir]', f'"{directory}"')
            cmd = cmd.replace('[part]', f'"{part}"')
            proc = subprocess.Popen(cmd, shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=directory)
            stdout, stderr = proc.communicate()
            print(cmd, file=sys.stderr)
            if stderr:
                print(stderr, file=sys.stderr)
            if stdout:
                print(stdout, file=sys.stderr)

    def show_cmd_menu(self):
        self.cmd_menu.post(self.cmd_button.winfo_rootx(),
                           self.cmd_button.winfo_rooty()+self.cmd_button.winfo_height())

    def read_config(self):
        self.write_config()
        config = CaseConfigParser()
        config.read(os.path.expanduser(config_file))
        need_update = False
        try:
            self.bookmarks = config['Bookmarks']
        except:
            need_update = True
        try:
            self.commands = config['Commands']
        except:
            need_update = True
        try:
            global SCALE, butstyle
            SCALE = float(config.get('Settings','dpi_scale'))
            window_size = config.get('Settings','window_size')
            w, h = window_size.split('x')
            self.INIT_WIDTH, self.INIT_HEIGHT = int(w.strip()), int(h.strip())
            self.THUMBNAIL_SIZE = int(config.get('Settings','thumbnail_size'))
            theme = config.get('Settings','theme')
            if theme not in ['dark','light','none']:
                print('theme needs to be one of "dark", "light", or "none"', file=sys.stderr)
                theme = 'none'
            if theme == 'light':
                butstyle = 'raised'
            self.THEME = theme
            sortby = config.get('Settings','sort_by')
            by, asc = sortby.split('_')
            self.SORT = (by.lower(), asc == 'asc')
        except:
            need_update = True
        if need_update:
            os.unlink(config_file)
            self.write_config(config)
            self.read_config()
            return
        self.INIT_WIDTH = int(self.INIT_WIDTH * SCALE)
        self.INIT_HEIGHT = int(self.INIT_HEIGHT * SCALE)
        self.THUMBNAIL_SIZE = int(self.THUMBNAIL_SIZE * SCALE)

    def write_config(self, oldvals = None):
        if os.path.isfile(config_file):
            return
        with open(config_file, 'w') as f:
            print(f'Updating config file {config_file}', file=sys.stderr)
            confcomment = '''# Commands from the cmd menu will substitute the follwong values from the selected files before running, as seen in the convert examples. All paths and filenames are already quoted for you.
# [path] is full file path
# [name] is the filename without full path
# [dir] is the current directory without trailing slash
# [part] is the filename without path or extension
# [ext] is the file extension, including the period
'''
            def write_section(name, default: dict):
                section = oldvals[name] if oldvals and oldvals.has_section(name) else default
                f.write(f'[{name}]\n')
                if oldvals:
                    default.update(section)
                    section = default
                for k,v in section.items():
                    f.write(f'{k} = {v}\n')
                f.write('\n')
            cmds = {'resize':'convert -resize 1200 [path] [dir]/[part]_resized[ext]',
                    'convert webp':'convert [path] [dir]/[part].jpg'}
            settings = {'dpi_scale':'1',
                    'window_size':'990x720',
                    'thumbnail_size':'140',
                    'theme':'dark',
                    'sort_by':'name_asc'}
            bkmk = {'Home':home_dir}
            bkmk.update({k:os.path.join(home_dir,k) for k in ["Documents", "Pictures", "Downloads"]})
            f.write(confcomment)
            write_section('Commands', cmds)
            write_section('Settings', settings)
            write_section('Bookmarks', bkmk)


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
    def __init__(self, widget, num=0, state=0):
        self.widget = widget
        self.num = num
        self.state = state

class PathInfo(str):
    def __new__(cls, path):
        obj = str.__new__(cls, path)
        st = os.stat(path)
        obj.time = st.st_mtime
        obj.lname = os.path.basename(path).lower()
        obj.isdir = False
        obj.isfile = stat.S_ISREG(st.st_mode)
        if obj.isfile:
            obj.mime = mimetypes.guess_type(path)[0] or 'application/octet-stream'
        else:
            obj.isdir = stat.S_ISDIR(st.st_mode)
        return obj

class CaseConfigParser(configparser.RawConfigParser):
    def __init__(self, defaults=None):
        super().__init__(defaults)
    def optionxform(self, optionstr):
        return optionstr

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
