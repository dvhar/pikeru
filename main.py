#!/usr/bin/env python
import glob
import os, sys
import tkinter as tk
from PIL import Image, ImageTk
import threading
import queue

THUMBNAIL_WIDTH = 140
THUMBNAIL_HEIGHT = 140
asset_dir = os.path.dirname(os.path.abspath(__file__))

class FilePicker(tk.Frame):
    def __init__(self, start_path, **kwargs):
        self.root = tk.Tk()
        self.root.geometry('640x480')
        self.root.wm_title('File Picker')
        self.frame = tk.Frame(self.root, **kwargs)
        self.frame.grid_columnconfigure(0, weight=1)
        self.frame.grid_rowconfigure(0, weight=1)

        self.canvas = tk.Canvas(self.frame)
        self.canvas.grid(row=0, column=0, sticky='news')
        self.scrollbar = tk.Scrollbar(self.frame, orient='vertical', command=self.canvas.yview)
        self.scrollbar.grid(row=0, column=1, sticky='ns')
        self.canvas.configure(yscrollcommand=self.scrollbar.set)

        self.items_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_scroll(self.canvas)
        self.bind_scroll(self.items_frame)

        self.button_frame = tk.Frame(self.frame)
        self.button_frame.grid(row=2, column=0, sticky='e')
        self.frame.grid_rowconfigure(1, weight=0)

        self.open_button = tk.Button(self.button_frame, width=10, text="Open", command=self.on_open)
        self.open_button.pack(side='right')
        self.cancel_button = tk.Button(self.button_frame, width=10, text="Cancel", command=self.root.destroy)
        self.cancel_button.pack(side='right')
        self.up_dir_button = tk.Button(self.button_frame, width=10, text="Up Dir", command=self.on_up_dir)
        self.up_dir_button.pack(side='right')

        self.directory_entry = tk.Entry(self.frame)
        self.directory_entry.grid(row=1, column=0, padx=(10, 0), pady=(1, 0), sticky='ew')
        self.directory_entry.insert(0, os.getcwd())
        self.directory_entry.bind("<Return>", self.on_type_dir)

        self.num_items = 0
        self.queue = queue.Queue()
        self.loading_thread = threading.Thread(target=self.load_items)
        self.loading_thread.daemon = True
        self.loading_thread.start()
        self.frame.bind('<Configure>', self.on_resize)
        self.recalculate_max_cols()
        self.folder_icon = tk.PhotoImage(file=asset_dir+'/folder.png')

        self.frame.pack(fill='both', expand=True)
        self.change_dir(start_path)

    def run(self):
        self.root.mainloop()

    def bind_scroll(self, thing):
        thing.bind('<Button-4>', lambda e: self.canvas.yview_scroll(-2,'units'))
        thing.bind('<Button-5>', lambda e: self.canvas.yview_scroll(2,'units'))

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def enqueue_item(self, item_path):
        self.queue.put(item_path)

    def load_items(self):
        while True:
            item_path = self.queue.get()
            self.load_item(item_path)

    def load_item(self, item_path):
        try:
            base_path = os.path.basename(item_path)
            name = base_path if len(base_path) < 20 else base_path[len(base_path)-19:]
            if os.path.isfile(item_path):
                ext = os.path.splitext(base_path)[-1].lower()
                if ext in [".png", ".jpg", ".jpeg"]:
                    img = Image.open(item_path)
                    img.thumbnail((THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT))
                    img = ImageTk.PhotoImage(img)
                    label = tk.Label(self.items_frame, image=img, text=name, compound='top', bd=2)
                    label.full_path = item_path
                    label.sel = 0
                    label.image = img
                    label.grid(row=self.num_items//self.max_cols, column=self.num_items%self.max_cols)
                    label.bind("<Button-1>", lambda e: self.toggle_border(label))
                    self.bind_scroll(label)
                    label.bind("<Double-Button-1>", self.on_double_click_file)
                elif ext in [".txt", ".pdf", ".doc", ".docx"]:
                    label = tk.Label(self.items_frame, text=name, compound='top', bd=2)
                    label.full_path = item_path
                    label.sel = 0
                    label.grid(row=self.num_items//self.max_cols, column=self.num_items%self.max_cols)
                    label.bind("<Button-1>", lambda e: self.toggle_border(label))
                    label.bind("<Double-Button-1>", self.on_double_click_file)
                else:
                    # Handle other file types here if needed
                    pass
            elif os.path.isdir(item_path):
                label = tk.Label(self.items_frame, image=self.folder_icon, text=name, compound='top', bd=2)
                label.full_path = item_path
                label.sel = 0
                label.image = self.folder_icon
                label.grid(row=self.num_items//self.max_cols, column=self.num_items%self.max_cols)
                label.bind("<Button-1>", lambda e: self.toggle_border(label))
                label.bind("<Double-Button-1>", self.on_double_click_dir)
                self.bind_scroll(label)
            else:
                return
            self.num_items += 1
        except Exception as e:
            sys.stderr.write(f'Error loading item: {e}\n')

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
        max_width = self.frame.winfo_width()
        self.max_cols = max(1, int(max_width / (THUMBNAIL_WIDTH+6))) # figure out proper width calculation

    def toggle_border(self, label):
        if label.sel == 0:
            label.config(relief="solid", bg='red')
            label.sel = 1
        else:
            label.config(relief="flat", bg='black')
            label.sel = 0
            self.open_button.config(state='normal')
            self.cancel_button.config(state='normal')

    def on_open(self):
        selected_files = [label.full_path for label in self.items_frame.winfo_children() if label.sel]
        print('\n'.join(selected_files))
        self.root.destroy()

    def on_double_click_file(self, event):
        print(event.widget.full_path)
        self.root.destroy()

    def change_dir(self, new_dir):
        if os.path.isdir(new_dir):
            os.chdir(new_dir)
            self.directory_entry.delete(0, 'end')
            self.directory_entry.insert(0, new_dir)
            self.refresh_items()
            self.canvas.yview_moveto(0)

    def on_up_dir(self):
        new_dir = os.path.dirname(self.directory_entry.get())
        self.change_dir(new_dir)

    def on_double_click_dir(self, event):
        new_dir = event.widget.full_path
        self.change_dir(new_dir)

    def on_type_dir(self, event):
        new_dir = self.directory_entry.get()
        self.change_dir(new_dir)

    def refresh_items(self):
        self.items_frame.destroy()
        self.items_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.items_frame, anchor='nw')
        self.items_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_scroll(self.canvas)
        self.bind_scroll(self.items_frame)
        self.num_items = 0
        paths = glob.glob(os.path.join(os.getcwd(), '*'))
        for path in paths:
            self.enqueue_item(path)

    def on_resize(self, event=None):
        old = self.max_cols
        self.recalculate_max_cols()
        if old != self.max_cols:
            self.reorganize_items()

picker = FilePicker(os.getcwd())
picker.run()
