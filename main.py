#!/usr/bin/env python
import glob
import tkinter as tk
from PIL import Image, ImageTk
import threading
import queue

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
        self.button_frame.grid(row=1, column=0, sticky='e')
        self.frame.grid_rowconfigure(1, weight=0)

        self.open_button = tk.Button(self.button_frame, width=10, text="Open", command=self.on_open)
        self.open_button.pack(side='right')

        self.cancel_button = tk.Button(self.button_frame, width=10, text="Cancel", command=self.on_cancel)
        self.cancel_button.pack(side='right')

        self.num_images = 0
        self.queue = queue.Queue()
        self.loading_thread = threading.Thread(target=self.load_images)
        self.loading_thread.daemon = True
        self.loading_thread.start()

    def bind_scroll(self, thing):
        thing.bind('<Button-4>', lambda e: self.canvas.yview_scroll(-2,'units'))
        thing.bind('<Button-5>', lambda e: self.canvas.yview_scroll(2,'units'))

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def enqueue_image(self, image_path):
        self.queue.put(image_path)

    def load_images(self):
        while True:
            try:
                image_path = self.queue.get(timeout=1)
            except queue.Empty:
                continue
            self.load_image(image_path)

    def load_image(self, image_path):
        img = Image.open(image_path)
        img.thumbnail((180,180))
        img = ImageTk.PhotoImage(img)
        label = tk.Label(self.images_frame, image=img, text=image_path, compound='top', bd=2)
        label.__setattr__('sel', 0)
        label.__setattr__('image', img)
        label.grid(row=self.num_images//3, column=self.num_images%3)
        label.bind("<Button-1>", lambda e: self.toggle_border(label))
        self.bind_scroll(label)
        self.num_images += 1

    def highlight_image(self, label):
        label.config(relief="solid", bg='red')

    def unhighlight_image(self, label):
        label.config(relief="flat", bg='black')

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
        selected_files = [label['text'] for label in self.images_frame.winfo_children() if label.sel]
        print('\n'.join(selected_files))
        root.destroy()

    def on_cancel(self):
        root.destroy()

root = tk.Tk()
root.geometry('610x400')
grid = FilePicker(root)
grid.frame.pack(fill='both', expand=True)
for i, path in enumerate(glob.glob('pics/*png')):
    grid.enqueue_image(path)
root.wm_title('File Picker')
root.mainloop()
