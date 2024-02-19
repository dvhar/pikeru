#!/usr/bin/env python
import glob
import tkinter as tk
from PIL import Image, ImageTk

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

    def bind_scroll(self, thing):
        thing.bind('<Button-4>', lambda e: self.canvas.yview_scroll(-2,'units'))
        thing.bind('<Button-5>', lambda e: self.canvas.yview_scroll(2,'units'))

    def on_frame_configure(self, event=None):
        self.canvas.configure(scrollregion=self.canvas.bbox('all'))

    def add_image(self, image_path, row, col):
        img = Image.open(image_path)
        img.thumbnail((180,180))
        img = ImageTk.PhotoImage(img)
        label = tk.Label(self.images_frame, image=img, text=image_path, compound='top', bd=2)
        label.__setattr__('sel', 0)
        label.image = img
        label.grid(row=row, column=col)
        # label.bind("<Enter>", lambda e: self.highlight_image(label))
        # label.bind("<Leave>", lambda e: self.unhighlight_image(label))
        label.bind("<Button-1>", lambda e: self.toggle_border(label))
        self.bind_scroll(label)

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

root = tk.Tk()
root.geometry('610x400')
grid = FilePicker(root)
grid.frame.pack(fill='both', expand=True)
for i, path in enumerate(glob.glob('pics/*png')):
    grid.add_image(path, i//3, i%3)
root.wm_title('File Picker')
root.mainloop()
