#!/usr/bin/env python
import argparse
import glob
import os, sys
import tkinter as tk
from PIL import Image, ImageTk
from tkinter.messagebox import askyesno
import threading
import queue

THUMBNAIL_WIDTH = 140
THUMBNAIL_HEIGHT = 140

# https://icon-icons.com
asset_dir = os.path.dirname(os.path.abspath(__file__))
home_dir = os.environ['HOME']

class PathInfo(str):
    def __new__(cls, path):
        obj = str.__new__(cls, path)
        obj.time = os.path.getmtime(path)
        obj.lname = os.path.basename(path).lower()
        obj.isdir = os.path.isdir(path)
        return obj

class FilePicker(tk.Frame):
    def __init__(self, args: argparse.Namespace, **kwargs):
        self.select_dir = args.type == 'dir'
        self.select_multi = not self.select_dir and args.type == 'files'
        self.select_save = args.type == 'save'
        self.save_filename = None
        if self.select_save and not os.path.isdir(args.path):
            self.save_filename = os.path.basename(args.path)
        self.sort_by = 'name'

        self.root = tk.Tk()
        self.root.geometry('720x480')
        self.root.wm_title(args.title or 'File Picker')
        self.frame = tk.Frame(self.root, **kwargs)
        self.frame.grid_columnconfigure(0, weight=1)
        self.frame.grid_rowconfigure(0, weight=1)
        self.frame.grid_rowconfigure(1, weight=0)

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
        self.bind_scroll(self.canvas)
        self.bind_scroll(self.items_frame)

        self.path_textfield = tk.Entry(lower_frame)
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

        self.num_items = 0
        self.queue = queue.Queue()
        self.loading_thread = threading.Thread(target=self.load_items)
        self.loading_thread.daemon = True
        self.loading_thread.start()
        self.frame.bind('<Configure>', self.on_resize)
        self.recalculate_max_cols()
        self.folder_icon = tk.PhotoImage(file=asset_dir+'/folder.png')
        self.doc_icon = tk.PhotoImage(file=asset_dir+'/document.png')
        self.unknown_icon = tk.PhotoImage(file=asset_dir+'/unknown.png')
        self.prev_sel = None

        self.bookmarks = ['', 'Documents', 'Pictures', 'Downloads']
        for i, bookmark in enumerate(self.bookmarks):
            btn = tk.Button(self.bookmark_frame, text=bookmark if bookmark else 'Home')
            btn.path = os.path.join(home_dir, bookmark)
            btn.grid(row=i, column=0, sticky='news')
            btn.bind("<Button-1>", self.on_bookmark_click)

        self.frame.pack(fill='both', expand=True)
        self.change_dir(args.path)

    def run(self):
        self.root.mainloop()

    def bind_scroll(self, thing):
        thing.bind('<Button-4>', lambda _: self.canvas.yview_scroll(-2,'units'))
        thing.bind('<Button-5>', lambda _: self.canvas.yview_scroll(2,'units'))

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def enqueue_item(self, item_path):
        self.queue.put(item_path)

    def load_items(self):
        while True:
            item_path = self.queue.get()
            self.load_item(item_path)

    def prep_file(self, label, item_path):
        label.sel = False
        label.path = item_path
        self.bind_scroll(label)
        if not self.select_dir:
            label.bind("<Button-1>", self.on_click_file)
            label.bind("<Double-Button-1>", self.on_double_click_file)
        label.grid(row=self.num_items//self.max_cols, column=self.num_items%self.max_cols)

    def load_item(self, item_path):
        try:
            base_path = os.path.basename(item_path)
            name = base_path if len(base_path) < 20 else base_path[len(base_path)-19:]
            if os.path.isfile(item_path):
                ext = os.path.splitext(base_path)[-1].lower()
                match ext:
                    case '.png'|'.jpg'|'.jpeg'|'.gif':
                        img = Image.open(item_path)
                        img.thumbnail((THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT))
                        img = ImageTk.PhotoImage(img)
                        label = tk.Label(self.items_frame, image=img, text=name, compound='top', bd=2)
                        label.__setattr__('img', img)
                        self.prep_file(label, item_path)
                    case '.txt'|'.pdf'|'.doc'|'.docx':
                        label = tk.Label(self.items_frame, image=self.doc_icon, text=name, compound='top', bd=2)
                        self.prep_file(label, item_path)
                    case _:
                        label = tk.Label(self.items_frame, image=self.unknown_icon, text=name, compound='top', bd=2)
                        self.prep_file(label, item_path)
            elif os.path.isdir(item_path):
                label = tk.Label(self.items_frame, image=self.folder_icon, text=name, compound='top', bd=2)
                label.path = item_path
                label.sel = False
                label.grid(row=self.num_items//self.max_cols, column=self.num_items%self.max_cols)
                label.bind("<Double-Button-1>", self.on_double_click_dir)
                if self.select_dir:
                    label.bind("<Button-1>", self.on_click_file)
                self.bind_scroll(label)
                label.bind("<ButtonRelease-1>", self.on_drag_dir_end)
            else:
                return
            self.num_items += 1
        except Exception as e:
            sys.stderr.write(f'Error loading item: {e}\n')

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
        new_bookmark = tk.Button(self.bookmark_frame, text=os.path.basename(path), command=lambda: self.change_dir(path))
        new_bookmark.path = path
        new_bookmark.grid(row=len(bookmarks), column=0, sticky='news')
        self.bookmark_frame.update_idletasks()

    def on_bookmark_click(self, event):
        self.change_dir(event.widget.path)

    def reorganize_items(self):
        num_child_rows = self.num_items // self.max_cols + (1 if self.num_items % self.max_cols != 0 else 0)
        all_items = self.items_frame.winfo_children()
        for row in range(num_child_rows):
            start = row * self.max_cols
            for col in range(self.max_cols):
                index = start + col
                if index < len(all_items):
                    all_items[index].grid(row=row, column=col)

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
            self.open_button.config(state='normal')
            self.cancel_button.config(state='normal')
        if label.sel:
            if not self.select_multi and self.prev_sel and self.prev_sel is not label:
                self.prev_sel.sel = False
                self.prev_sel.config(relief="flat", bg='black')
            self.prev_sel = label

    def change_dir(self, new_dir):
        if os.path.isdir(new_dir):
            self.prev_sel = None
            os.chdir(new_dir)
            self.path_textfield.delete(0, 'end')
            if self.save_filename:
                new_dir += '/' + self.save_filename
            self.path_textfield.insert(0, new_dir)
            while self.queue.qsize() > 0:
                self.queue.get()
            self.refresh_items()
            self.canvas.yview_moveto(0)

    def on_up_dir(self):
        new_dir = os.path.dirname(self.path_textfield.get())
        if self.select_save:
            new_dir = os.path.dirname(new_dir)
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
        selected_files = [label.path for label in self.items_frame.winfo_children() if label.sel]
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

    def refresh_items(self):
        self.items_frame.destroy()
        self.items_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_scroll(self.canvas)
        self.bind_scroll(self.items_frame)
        self.num_items = 0
        paths = [PathInfo(p) for p in glob.glob(os.path.join(os.getcwd(), '*'))]
        self.sort(paths)
        for path in paths:
            self.enqueue_item(path)

    def sort(self, paths: list):
        sort = lambda p: (not p.isdir, p.lname)
        if self.sort_by == 'time':
            sort = lambda p: (not p.isdir, p.time)
        paths.sort(key=sort)

    def on_resize(self, event=None):
        old = self.max_cols
        self.recalculate_max_cols()
        if old != self.max_cols:
            self.reorganize_items()

def main():
    parser = argparse.ArgumentParser(description="A filepicker with proper thumbnail support")
    parser.add_argument("-e", "--parent", help="window id of the window this one is transient to")
    parser.add_argument("-t", "--title", default="File Picker", help="title of the filepicker window")
    parser.add_argument("-k", "--type", choices=['file', 'files', 'dir', 'save'], help="type of file selection. One of [file files dir save]")
    parser.add_argument("-p", "--path", default=os.getcwd(), help="path of initial directory")
    parser.add_argument("-i", "--mime_list", nargs="*", help="list of allowed mime types. Can be empty.")
    args = parser.parse_args()
    
    picker = FilePicker(args)
    picker.run()

if __name__ == "__main__":
    main()
