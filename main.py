#!/usr/bin/env python
import glob
import os, sys
import tkinter as tk
from PIL import Image, ImageTk
import threading
import queue

asset_dir = os.path.dirname(os.path.abspath(__file__))

class FilePicker(tk.Frame):
    def __init__(self, master=None, **kwargs):
        self.frame = tk.Frame(master, **kwargs)
        self.frame.grid_columnconfigure(0, weight=1)
        self.frame.grid_rowconfigure(0, weight=1)

        self.canvas = tk.Canvas(self.frame)
        self.canvas.grid(row=0, column=0, sticky='news')
        self.scrollbar = tk.Scrollbar(self.frame, orient='vertical', command=self.canvas.yview)
        self.scrollbar.grid(row=0, column=1, sticky='ns')
        self.canvas.configure(yscrollcommand=self.scrollbar.set)

        self.images_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.images_frame, anchor='nw')
        self.images_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_scroll(self.canvas)
        self.bind_scroll(self.images_frame)

        self.button_frame = tk.Frame(self.frame)
        self.button_frame.grid(row=2, column=0, sticky='e')
        self.frame.grid_rowconfigure(1, weight=0)

        self.open_button = tk.Button(self.button_frame, width=10, text="Open", command=self.on_open)
        self.open_button.pack(side='right')
        self.cancel_button = tk.Button(self.button_frame, width=10, text="Cancel", command=root.destroy)
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

    def bind_scroll(self, thing):
        thing.bind('<Button-4>', lambda e: self.canvas.yview_scroll(-2,'units'))
        thing.bind('<Button-5>', lambda e: self.canvas.yview_scroll(2,'units'))

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def enqueue_item(self, item_path):
        self.queue.put(item_path)

    def load_items(self):
        while True:
            try:
                item_path = self.queue.get(timeout=1)
            except queue.Empty:
                continue
            self.load_item(item_path)

    def load_item(self, item_path):
        try:
            base_path = os.path.basename(item_path)
            name = base_path if len(base_path) < 20 else base_path[len(base_path)-19:]
            if os.path.isfile(item_path):
                ext = os.path.splitext(base_path)[-1].lower()
                if ext in [".png", ".jpg", ".jpeg"]:
                    img = Image.open(item_path)
                    img.thumbnail((180,180))
                    img = ImageTk.PhotoImage(img)
                    label = tk.Label(self.images_frame, image=img, text=name, compound='top', bd=2)
                    label.full_path = item_path
                    label.sel = 0
                    label.image = img
                    label.grid(row=self.num_items//3, column=self.num_items%3)
                    label.bind("<Button-1>", lambda e: self.toggle_border(label))
                    self.bind_scroll(label)
                elif ext in [".txt", ".pdf", ".doc", ".docx"]:
                    label = tk.Label(self.images_frame, text=name, compound='top', bd=2)
                    label.full_path = item_path
                    label.sel = 0
                    label.grid(row=self.num_items//3, column=self.num_items%3)
                    label.bind("<Button-1>", lambda e: self.toggle_border(label))
                else:
                    # Handle other file types here if needed
                    pass
            elif os.path.isdir(item_path):
                dir_icon = tk.PhotoImage(file=asset_dir+'/folder.png')
                label = tk.Label(self.images_frame, image=dir_icon, text=name, compound='top', bd=2)
                label.full_path = item_path
                label.sel = 0
                label.image = dir_icon
                label.grid(row=self.num_items//3, column=self.num_items%3)
                label.bind("<Button-1>", lambda e: self.toggle_border(label))
                label.bind("<Double-Button-1>", self.on_double_click_dir)
                self.bind_scroll(label)
            self.num_items += 1
        except Exception as e:
            sys.stderr.write(f'Error loading item: {e}\n')

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
        selected_files = [label.full_path for label in self.images_frame.winfo_children() if label.sel]
        print('\n'.join(selected_files))
        root.destroy()

    def change_dir(self, new_dir):
        if os.path.isdir(new_dir):
            os.chdir(new_dir)
            self.directory_entry.delete(0, 'end')
            self.directory_entry.insert(0, new_dir)
            self.refresh_items()

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
        self.images_frame.destroy()
        self.images_frame = tk.Frame(self.canvas)
        self.canvas.create_window((0,0), window=self.images_frame, anchor='nw')
        self.images_frame.bind('<Configure>', self.on_frame_configure)
        self.bind_scroll(self.canvas)
        self.bind_scroll(self.images_frame)
        self.num_items = 0
        paths = glob.glob(os.path.join(os.getcwd(), '*'))
        for path in paths:
            self.enqueue_item(path)

root = tk.Tk()
root.geometry('610x400')
grid = FilePicker(root)
grid.frame.pack(fill='both', expand=True)
for i, path in enumerate(glob.glob('pics/*')):
    grid.enqueue_item(path)
root.wm_title('File Picker')
root.mainloop()
