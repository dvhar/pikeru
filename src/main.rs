use img::load_from_memory;
use img;
use num_cpus;
use itertools::Itertools;
mod wrapper;
mod iced_drop;
mod style;
use iced::{
    advanced::widget::Id,
    Rectangle,
    color, Background, alignment, executor, subscription,
    Application, Command, Length, Element, theme::Container,
    mouse::Event::{ButtonPressed, WheelScrolled},
    mouse::Button::{Back,Forward},
    mouse::ScrollDelta,
    keyboard::Event::{KeyPressed,KeyReleased},
    keyboard::Key,
    keyboard::key::Named::{Shift,Control},
    widget::{
        vertical_space, checkbox,
        container::{Appearance, StyleSheet,Id as CId},
        image, image::Handle, Column, Row, text, responsive,
        Scrollable, scrollable, scrollable::{Direction,Properties},
        Button, TextInput,
        column, row, mouse_area, container,
    },
    futures::{
        channel::mpsc,
        sink::SinkExt,
        StreamExt,
    },
    event::{self, Event::{Mouse,Keyboard}},
    Point,
};
use tokio::{
    fs::File, io::AsyncReadExt,
    sync::mpsc::{
        UnboundedReceiver,
        UnboundedSender,
        unbounded_channel,
    }
};
use std::{
    collections::HashMap,
    str,
    path::{PathBuf,Path},
    mem,
    process,
    process::Command as OsCmd,
    sync::Arc,
    time::{Instant,Duration},
};
use video_rs::{Decoder, Location, DecoderBuilder, Resize};
use ndarray;
use getopts::Options;
use inotify::{Inotify, WatchMask, WatchDescriptor};
use iced_aw::{menu_bar, menu_items};
use iced_aw::menu::{Item, Menu};

macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

fn main() -> iced::Result {
    let opts = Config::new();
    video_rs::init().unwrap();
    FilePicker::run(iced::Settings::with_flags(opts))
}

struct Config {
    title: String,
    path: String,
    mode: Mode,
    sort_by: i32,
    bookmarks: Vec<Bookmark>,
    cmds: Vec<Cmd>,
    thumb_size: f32,
}

impl Config {

    #[inline]
    fn saving(self: &Self) -> bool { self.mode == Mode::Save }
    fn multi(self: &Self) -> bool { self.mode == Mode::Files }
    fn dir(self: &Self) -> bool { self.mode == Mode::Dir }

    fn new() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut opts = Options::new();
        let pwd = std::env::var("PWD").unwrap();
        opts.optopt("t", "title", "Title of the filepicker window", "NAME");
        opts.optopt("m", "mode", "Mode of file selection. Default is files", "[file, files, save, dir]");
        opts.optopt("p", "path", "Initial path", "PATH");
        let matches = match opts.parse(args) {
            Ok(m) => { m },
            Err(e) => die!("bad args:{}", e),
        };

        let home = std::env::var("HOME").unwrap();
        let confpath = Path::new(&home).join(".config").join("pikeru.conf").to_string_lossy().to_string();
        let txt = std::fs::read_to_string(confpath).unwrap();
        enum S { Commands, Settings, Bookmarks }
        let mut section = S::Commands;
        let mut bookmarks = vec![];
        let mut cmds = vec![];
        let mut sort_by = 1;
        let mut thumb_size = 160.0;
        for line in txt.lines().map(|s|s.trim()).filter(|s|s.len()>0 && !s.starts_with('#')) {
            match line {
                "[Commands]" => section = S::Commands,
                "[Settings]" => section = S::Settings,
                "[Bookmarks]" => section = S::Bookmarks,
                _ => {
                    let (k, v) = str::split_once(line, '=').unwrap();
                    let (k, v) = (k.trim(), v.trim());
                    match section {
                        S::Commands => cmds.push(Cmd::new(k, v)),
                        S::Bookmarks => bookmarks.push(Bookmark::new(k,v)),
                        S::Settings => match k {
                            "thumbnail_size" => thumb_size = v.parse().unwrap(),
                            "sort_by" => sort_by = match v {
                                "name_desc" => 2,
                                "time_asc" => 3,
                                "time_desc" => 4,
                                _ => 1,
                            },
                            _ => {},
                        },
                    }
                },
            }
        }

        Config {
            mode: Mode::from(matches.opt_str("m")),
            path: matches.opt_str("p").unwrap_or(pwd),
            title: "File Picker".to_string(),
            cmds,
            bookmarks,
            sort_by,
            thumb_size,
        }
    }
}

#[derive(PartialEq)]
enum Mode {
    File,
    Files,
    Save,
    Dir,
}
impl Mode {
   fn from(opt: Option<String>) -> Self {
       match opt {
           None => Self::Files,
           Some(s) => {
               match s.as_str() {
                   "file" => Self::File,
                   "files" => Self::Files,
                   "save" => Self::Save,
                   "dir" => Self::Dir,
                   _ => Self::Files,
               }
           }
       }
   }
}

#[derive(Debug, Clone)]
enum Message {
    LoadDir,
    LoadBookmark(usize),
    Select(bool),
    Cancel,
    UpDir,
    Init((mpsc::Sender<FItem>, UnboundedSender<Inochan>)),
    NextItem(FItem),
    LeftClick(usize),
    MiddleClick(usize),
    RightClick(i64),
    TxtInput(String),
    Shift(bool),
    Ctrl(bool),
    DropBookmark(usize, Point),
    HandleZones(usize, Vec<(Id, iced::Rectangle)>),
    NextImage(i64),
    Scrolled(scrollable::Viewport),
    PositionInfo(Rectangle, Rectangle),
    Sort(i32),
    ShowHidden(bool),
    RunCmd(usize),
    InoDelete(String),
    InoCreate(String),
    Dummy,
}

enum SubState {
    Starting,
    Ready((mpsc::Receiver<FItem>,UnboundedReceiver<Inochan>)),
}

#[derive(Debug, Clone, Default, PartialEq)]
enum FType {
    File,
    Image,
    Dir,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default)]
struct FItem {
    path: String,
    label: String,
    ftype: FType,
    handle: Option<Handle>,
    idx: usize,
    sel: bool,
    nav_id: u8,
    mtime: f32,
    vid: bool,
}

struct Icons {
    folder: Handle,
    doc: Handle,
    unknown: Handle,
    error: Handle,
}

struct Bookmark {
    label: String,
    path: String,
    id: CId,
}
#[derive(Debug)]
struct Cmd {
    label: String,
    cmd: String,
}

struct FilePicker {
    conf: Config,
    scroll_id: scrollable::Id,
    items: Vec<FItem>,
    dirs: Vec<String>,
    inputbar: String,
    thumb_sender: Option<mpsc::Sender<FItem>>,
    nproc: usize,
    last_loaded: usize,
    last_clicked: Option<usize>,
    icons: Arc<Icons>,
    clicktimer: ClickTimer,
    ctrl_pressed: bool,
    shift_pressed: bool,
    nav_id: u8,
    show_hidden: bool,
    view_image: (usize, Option<Handle>),
    scroll_offset: scrollable::AbsoluteOffset,
    ino_updater: Option<UnboundedSender<Inochan>>,
    save_filename: Option<String>,
    select_button: String,
}

impl Application for FilePicker {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = iced::Theme;
    type Flags = Config;

    fn new(conf: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let pathstr = conf.path.clone();
        let path = Path::new(&pathstr);
        let startdir = if path.is_dir() {
            path.to_string_lossy()
        } else {
            path.parent().unwrap().to_string_lossy()
        };
        let ts = conf.thumb_size;
        let save_filename = if conf.saving() {
            Some(path.file_name().unwrap().to_string_lossy().to_string())
        } else {
            None
        };
        let select_button = match conf.mode {
            Mode::Files|Mode::File => "Open",
            Mode::Save => "Save",
            Mode::Dir => "Selecct",
        }.to_string();
        (
            Self {
                conf,
                items: Default::default(),
                thumb_sender: None,
                nproc: num_cpus::get() * 2,
                dirs: vec![startdir.to_string()],
                last_loaded: 0,
                last_clicked: None,
                inputbar: Default::default(),
                icons: Arc::new(Icons::new(ts)),
                clicktimer: ClickTimer{ idx:0, time: Instant::now() - Duration::from_secs(1)},
                ctrl_pressed: false,
                shift_pressed: false,
                scroll_id: scrollable::Id::unique(),
                nav_id: 0,
                show_hidden: false,
                view_image: (0, None),
                scroll_offset: scrollable::AbsoluteOffset{x: 0.0, y: 0.0},
                ino_updater: None,
                save_filename,
                select_button,
            },
            Command::none(),
        )
    }

    fn theme(&self) -> iced::Theme {
        iced::Theme::Dark
    }

    fn title(&self) -> String {
        self.conf.title.clone()
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::InoCreate(file) => {
                let len = self.items.len();
                self.items.push(FItem::default());
                let mut item = FItem::new(file.as_str().into(), self.nav_id);
                item.idx = len;
                item.nav_id = self.nav_id;
                let sender = self.thumb_sender.as_ref().unwrap().clone();
                let icons = self.icons.clone();
                let ts = self.conf.thumb_size as u32;
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
                    item.load(sender, icons, ts).await;
                });
            },
            Message::InoDelete(file) => {
                if let Some(i) = self.items.iter().position(|x|x.path == file) {
                    self.items.remove(i);
                }
                self.items.iter_mut().enumerate().for_each(|(i,item)|item.idx = i);
            },
            Message::RunCmd(i) => self.run_command(i),
            Message::Dummy => {},
            Message::ShowHidden(show) => {
                self.show_hidden = show;
                return self.update(Message::LoadDir);
            },
            Message::Sort(i) => {
                self.items.sort_unstable_by(
                    match i {
                        1 => |a:&FItem,b:&FItem|b.isdir().cmp(&a.isdir()).then_with(||a.path.cmp(&b.path)),
                        2 => |a:&FItem,b:&FItem|b.isdir().cmp(&a.isdir()).then_with(||b.path.cmp(&a.path)),
                        3 => |a:&FItem,b:&FItem|b.isdir().cmp(&a.isdir()).then_with(||b.mtime.partial_cmp(&a.mtime).unwrap()),
                        4 => |a:&FItem,b:&FItem|b.isdir().cmp(&a.isdir()).then_with(||a.mtime.partial_cmp(&b.mtime).unwrap()),
                        _ => unreachable!(),
                    });
                self.items.iter_mut().enumerate().for_each(|(i,item)|item.idx = i);
                self.conf.sort_by = i;
            },
            Message::PositionInfo(widget, viewport) => {
                if let Some(_) = self.last_clicked {
                    self.last_clicked = None;
                    return self.keep_in_view(widget, viewport);
                }
            },
            Message::DropBookmark(idx, cursor_pos) => {
                return iced_drop::zones_on_point(
                    move |zones| Message::HandleZones(idx, zones),
                    cursor_pos, None, None,
                );
            }
            Message::HandleZones(idx, zones) => {
                if zones.len() > 0 {
                    let targets: Vec<_> = self.conf.bookmarks.iter().enumerate().filter_map(|(i, bm)| {
                        if zones[0].0 == bm.id.clone().into() {
                            Some(i)
                        } else {None}
                    }).collect();
                    let target = if targets.len() > 0 {
                        Some(targets[0] as i32)
                    } else if zones[0].0 == Id::new("bookmarks") {
                        Some(-1)
                    } else { None };
                    self.add_bookmark(idx, target);
                }
            }
            Message::Init((fichan, inochan)) => {
                let (txctl, rxctl) = unbounded_channel::<Inochan>();
                tokio::spawn(watch_inotify(rxctl, inochan));
                self.ino_updater = Some(txctl);
                self.thumb_sender = Some(fichan);
                return self.update(Message::LoadDir);
            },
            Message::Scrolled(viewport) => self.scroll_offset = viewport.absolute_offset(),
            Message::TxtInput(txt) => self.inputbar = txt,
            Message::Ctrl(pressed) => self.ctrl_pressed = pressed,
            Message::Shift(pressed) => self.shift_pressed = pressed,
            Message::NextItem(doneitem) => {
                if doneitem.nav_id == self.nav_id {
                    self.last_loaded += 1;
                    if self.last_loaded < self.items.len() &&  self.items[self.last_loaded].handle == None {
                        let nextitem = mem::take(&mut self.items[self.last_loaded]);
                        tokio::task::spawn(nextitem.load(
                                self.thumb_sender.as_ref().unwrap().clone(), self.icons.clone(), self.conf.thumb_size as u32));
                    }
                    let i = doneitem.idx;
                    self.items[i] = doneitem;
                }
            },
            Message::LoadBookmark(idx) => {
                self.dirs = vec![self.conf.bookmarks[idx].path.clone()];
                self.scroll_offset.y = 0.0;
                return self.update(Message::LoadDir);
            },
            Message::LoadDir => {
                self.view_image = (0, None);
                self.inputbar = match &self.save_filename {
                    Some(fname) => Path::new(&self.dirs[0]).join(fname).to_string_lossy().to_string(),
                    None => self.dirs[0].clone(),
                };
                self.load_dir();
                let _ = self.update(Message::Sort(self.conf.sort_by));
                self.last_loaded = self.nproc.min(self.items.len());
                for i in 0..self.last_loaded {
                    let item = mem::take(&mut self.items[i]);
                    tokio::task::spawn(item.load(
                                self.thumb_sender.as_ref().unwrap().clone(), self.icons.clone(), self.conf.thumb_size as u32));
                }
                return scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::START);
            },
            Message::UpDir => {
                self.dirs = self.dirs.iter().map(|dir| Path::new(dir.as_str()).parent().unwrap()
                                                 .as_os_str().to_str().unwrap().to_string())
                    .unique_by(|s|s.to_owned()).collect();
                return self.update(Message::LoadDir);
            },
            Message::MiddleClick(idx) => self.click_item(idx, false, true),
            Message::LeftClick(idx) => {
                match self.clicktimer.click(idx) {
                    ClickType::Single => self.click_item(idx, self.shift_pressed, self.ctrl_pressed),
                    ClickType::Double => {
                        self.items[idx].sel = true;
                        return self.update(Message::Select(false));
                    },
                }
            },
            Message::RightClick(idx) => {
                if idx >= 0 {
                    let idx = idx as usize;
                    let item = &self.items[idx];
                    if item.ftype == FType::Image {
                        self.view_image = (item.idx, item.preview());
                        self.click_item(idx, false, false);
                        self.items[idx].sel = true;
                    } else {
                        self.click_item(idx as usize, true, false);
                    }
                } else {
                    self.view_image = (0, None);
                    return scrollable::scroll_to(self.scroll_id.clone(), self.scroll_offset);
                }
            },
            Message::NextImage(step) => {
                if self.view_image.1 != None {
                    let mut i = self.view_image.0;
                    while (step<0 && i>0) || (step>0 && i<self.items.len()-1) {
                        i = ((i as i64) + step) as usize;
                        if self.items[i as usize].ftype == FType::Image {
                            let img = self.items[i].preview();
                            if img != None {
                                self.view_image = (i as usize, img);
                                return self.update(Message::LeftClick(i as usize));
                            }
                        }
                    }
                }
            }
            Message::Select(button) => {
                let sels: Vec<&FItem> = self.items.iter().filter(|item| item.sel ).collect();
                if sels.len() != 0 {
                    match sels[0].ftype {
                        FType::Dir => {
                            if self.conf.dir() && sels.len() == 1 && button {
                                println!("{}", sels[0].path);
                                process::exit(0);
                            } else {
                                self.dirs = sels.iter().filter_map(|item| match item.ftype {
                                    FType::Dir => Some(item.path.clone()), _ => None}).collect();
                                return self.update(Message::LoadDir);
                            }
                        },
                        _ => {
                            if self.conf.saving() {
                                if !self.inputbar.is_empty() {
                                    let result = Path::new(&self.inputbar);
                                    if !result.is_dir() {
                                        println!("{}", self.inputbar);
                                        process::exit(0);
                                    }
                                }
                            } else {
                                sels.iter().for_each(|item| println!("{}", item.path));
                                process::exit(0);
                            }
                        }
                    }
                }
            },
            Message::Cancel => process::exit(0),
        }
        Command::none()
    }

    fn scale_factor(self: &Self) -> f64 {
        1.0
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        let mut state = SubState::Starting;
        let items = subscription::channel("", 100, |mut messager| async move {
            loop {
                match &mut state {
                    SubState::Starting => {
                        let (fi_sender, fi_reciever) = mpsc::channel(100);
                        let (ino_sender, ino_receiver) = unbounded_channel::<Inochan>();
                        messager.send(Message::Init((fi_sender, ino_sender))).await.unwrap();
                        state = SubState::Ready((fi_reciever,ino_receiver));
                    }
                    SubState::Ready((thumb_recv, ino_recv)) => {
                        tokio::select! {
                            item = thumb_recv.select_next_some() => messager.send(Message::NextItem(item)).await.unwrap(),
                            evt = ino_recv.recv() => {
                                match evt {
                                    Some(Inochan::Delete(file)) => messager.send(Message::InoDelete(file)).await.unwrap(),
                                    Some(Inochan::Create(file)) => messager.send(Message::InoCreate(file)).await.unwrap(),
                                    _ => {},
                                }
                            }
                        }
                    },
                }
            }
        });
        let events = event::listen_with(|evt, _| {
            match evt {
                Mouse(ButtonPressed(Back)) => Some(Message::UpDir),
                Mouse(ButtonPressed(Forward)) => None,
                Mouse(WheelScrolled{ delta: ScrollDelta::Lines{ y, ..}}) => Some(Message::NextImage(if y<0.0 {1} else {-1})),
                Keyboard(KeyPressed{ key: Key::Named(Shift), .. }) => Some(Message::Shift(true)),
                Keyboard(KeyReleased{ key: Key::Named(Shift), .. }) => Some(Message::Shift(false)),
                Keyboard(KeyPressed{ key: Key::Named(Control), .. }) => Some(Message::Ctrl(true)),
                Keyboard(KeyReleased{ key: Key::Named(Control), .. }) => Some(Message::Ctrl(false)),
                _ => None,
            }
        });
        subscription::Subscription::batch(vec![items, events/*, native*/])
    }

    fn view(&self) -> iced::Element<'_, Self::Message> {
        responsive(|size| {
            let view_menu = |items| Menu::new(items).max_width(180.0).offset(15.0).spacing(3.0);
            let cmd_list = self.conf.cmds.iter().enumerate().map(
                |(i,cmd)|Item::new(menu_button(cmd.label.as_str(), Message::RunCmd(i)))).collect();
            let ctrlbar = column![
                row![
                    menu_bar![
                        (top_button("Cmd", 80.0, Message::Dummy), 
                            view_menu(cmd_list))
                        (top_button("View", 80.0, Message::Dummy),
                            view_menu(menu_items!(
                                    (menu_button("Sort A-Z",Message::Sort(1)))
                                    (menu_button("Sort Z-A",Message::Sort(2)))
                                    (menu_button("Sort Newest first",Message::Sort(3)))
                                    (menu_button("Sort Oldest first",Message::Sort(4)))
                                    (checkbox("Show Hidden", self.show_hidden).on_toggle(Message::ShowHidden))
                                    )))
                    ].spacing(1.0),
                    top_button("New Dir", 80.0, Message::Cancel),
                    top_button("Up Dir", 80.0, Message::UpDir),
                    top_button("Cancel", 100.0, Message::Cancel),
                    top_button(&self.select_button, 100.0, Message::Select(true))
                ].spacing(1),
                TextInput::new("directory or file path", self.inputbar.as_str())
                    .on_input(Message::TxtInput)
                    .on_paste(Message::TxtInput),
            ].align_items(iced::Alignment::End).width(Length::Fill);
            let bookmarks = self.conf.bookmarks.iter().enumerate().fold(column![], |col,(i,bm)| {
                        col.push(Button::new(
                                    container(
                                        text(bm.label.as_str())
                                           .horizontal_alignment(alignment::Horizontal::Center)
                                           .width(Length::Fill)).id(bm.id.clone()))
                                           .style(style::get_but_theme())
                                     .on_press(Message::LoadBookmark(i)))
                    }).push(container(vertical_space()).height(Length::Fill).width(Length::Fill)
                            .id(CId::new("bookmarks"))).width(Length::Fixed(120.0));

            let content: iced::Element<'_, Self::Message> = if let Some(handle) = &self.view_image.1 {
                mouse_area(container(image(handle.clone())
                                    .width(Length::Fill)
                                    .height(Length::Fill))
                               .align_x(alignment::Horizontal::Center)
                               .align_y(alignment::Vertical::Center)
                               .width(Length::Fill).height(Length::Fill)
                    ).on_right_press(Message::RightClick(-1))
                    .into()
            } else {
                let maxcols = ((size.width-130.0) / self.conf.thumb_size).max(1.0) as usize;
                let num_rows = self.items.len() / maxcols + if self.items.len() % maxcols != 0 { 1 } else { 0 };
                let mut rows = Column::new();
                for i in 0..num_rows {
                    let start = i * maxcols;
                    let mut row = Row::new().width(Length::Fill);
                    for j in 0..maxcols {
                        let idx = start + j;
                        if idx < self.items.len() {
                            row = row.push(unsafe{self.items.get_unchecked(idx)}.display(self.last_clicked, self.conf.thumb_size));
                        }
                    }
                    rows = rows.push(row);
                }
                Scrollable::new(rows)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .on_scroll(Message::Scrolled)
                    .direction(Direction::Vertical(Properties::new()))
                    .id(self.scroll_id.clone()).into()
            };
            column![
                ctrlbar,
                row![bookmarks, content],
            ].into()
        }).into()
    }
}

fn menu_button(txt: &str, msg: Message) -> Element<'static, Message> {
    Button::new(container(text(txt)
                .width(Length::Fill)
                .horizontal_alignment(alignment::Horizontal::Center)))
        .style(style::get_but_theme())
        .on_press(msg).into()
}
fn top_button(txt: &str, size: f32, msg: Message) -> Element<'static, Message> {
    Button::new(text(txt)
                .width(size)
                .horizontal_alignment(alignment::Horizontal::Center))
        .style(style::get_but_theme())
        .on_press(msg).into()
}

impl FItem {

    fn display(&self, last_clicked: Option<usize>, thumbsize: f32) -> Element<'static, Message> {
        let mut col = Column::new()
            .align_items(iced::Alignment::Center)
            .width(Length::Fixed(thumbsize));
        if let Some(h) = &self.handle {
            col = col.push(image(h.clone()));
        }
        col = col.push(text(self.label.as_str()).size(13));
        let idx = self.idx;
        let clickable = match (self.isdir(), self.sel) {
            (true, true) => {
                let dr = iced_drop::droppable(col).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).style(get_sel_theme()))
            },
            (true, false) => {
                let dr = iced_drop::droppable(col).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(dr)
            },
            (false, true) => {
                mouse_area(container(col).style(get_sel_theme()))
            },
            (false, false) => {
                mouse_area(col)
            },
        }.on_release(Message::LeftClick(self.idx))
            .on_right_press(Message::RightClick(self.idx as i64))
            .on_middle_press(Message::MiddleClick(self.idx));
        match last_clicked {
            Some(i) if i == idx => {
                wrapper::locator(clickable).on_info(Message::PositionInfo).into()
            },
            _ => {
                clickable.into()
            },
        }
    }

    fn preview(self: &Self) -> Option<Handle> {
        match (&self.ftype, self.vid) {
            (FType::Image, false) => {
               let fmt = img::ImageFormat::from_path(&self.path).unwrap();
               match img::load(std::io::BufReader::new(std::fs::File::open(&self.path).ok()?), fmt) {
                   Ok(img) => {
                        Some(Handle::from_pixels(img.width(), img.height(), img.into_rgba8().as_raw().clone()))
                   },
                   Err(_) => {
                       match std::fs::read(self.path.as_str()) {
                           Ok(data) => {
                                match load_from_memory(data.as_ref()) {
                                   Ok(img) => {
                                        let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
                                        Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
                                   },
                                   Err(e) => {
                                       eprintln!("Error decoding image {}:{}", self.path, e);
                                       None
                                   },
                                }
                           }
                           Err(e) => {
                               eprintln!("Error reading image {}:{}", self.path, e);
                               None
                           },
                       }
                   },
               }
            },
            (FType::Image, true) => {
               vid_frame(self.path.as_str(), None)
            },
            _ => None,
        }
    }

    fn isdir(self: &Self) -> bool {
        return self.ftype == FType::Dir;
    }

    fn new(pth: PathBuf, nav_id: u8) -> Self {
        let md = pth.metadata().unwrap();
        let ftype = if md.is_dir() {
            FType::Dir
        } else {
            FType::Unknown
        };
        let mtime = md.modified().unwrap();
        let path = pth.to_string_lossy();
        let mut label = path.rsplitn(2,'/').next().unwrap().to_string();
        if label.len() > 20 {
            let mut start = label.len()-40.min(label.len());
            while  (label.as_bytes()[start] & 0b11000000) == 0b10000000 {
                start += 1;
            }
            let mut split = label.len()-20;
            while  (label.as_bytes()[split] & 0b11000000) == 0b10000000 {
                split += 1;
            }
            if start == split {
                label = label[start..].to_string();
            } else {
                label = format!("{}{}\n{}", if start == 0 { "" } else { "..." }, &label[start..split], &label[split..]);
            }
        }
        FItem {
            path: path.to_string(),
            label,
            ftype,
            idx: 0,
            handle: None,
            sel: false,
            nav_id,
            mtime: mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f32(),
            vid: false,
        }
    }

    async fn load(mut self, mut chan: mpsc::Sender<FItem>, icons: Arc<Icons>, thumbsize: u32) {
        match self.ftype {
            FType::Dir => {
                self.handle = Some(icons.folder.clone());
            },
            _ => {
                let ext = match self.path.rsplitn(2,'.').next() {
                    Some(s) => s,
                    None => "",
                };
                self.ftype = match ext.to_lowercase().as_str() {
                    "png"|"jpg"|"jpeg"|"bmp"|"tiff"|"gif"|"webp" => {
                        let file = File::open(self.path.as_str()).await;
                        match file {
                            Ok(mut file) => {
                                let mut buffer = Vec::new();
                                file.read_to_end(&mut buffer).await.unwrap_or(0);
                                let img = load_from_memory(buffer.as_ref());
                                match img {
                                    Ok(img) => {
                                        let thumb = img.thumbnail(thumbsize, thumbsize);
                                        let (w,h,rgba) = (thumb.width(), thumb.height(), thumb.into_rgba8());
                                        self.handle = Some(Handle::from_pixels(w, h, rgba.as_raw().clone()));
                                        FType::Image
                                    },
                                    Err(e) => {
                                        eprintln!("Error decoding image {}: {}", self.path, e);
                                        self.handle = Some(icons.error.clone());
                                        FType::File
                                    },
                                }
                            },
                            Err(e) => {
                                eprintln!("Error reading {}: {}", self.path, e);
                                self.handle = Some(icons.error.clone());
                                FType::File
                            },
                        }
                    },
                    "webm"|"mkv"|"mp4"|"av1" => {
                        self.handle = vid_frame(self.path.as_str(), Some(thumbsize));
                        self.vid = true;
                        FType::Image
                    },
                    "txt"|"pdf"|"doc"|"docx"|"xls"|"xlsx" => {
                        self.handle = Some(icons.doc.clone());
                        FType::File
                    },
                    _ => {
                        self.handle = Some(icons.unknown.clone());
                        FType::File
                    },
                };
            }
        }
        chan.send(self).await.unwrap();
    }
}

enum Inochan {
    NewDirs(Vec<String>),
    Delete(String),
    Create(String),
}
async fn watch_inotify(mut rx: UnboundedReceiver<Inochan>, tx: UnboundedSender<Inochan>) {
    let ino = Inotify::init().expect("Error initializing inotify instance");
    let evbuf = [0; 1024];
    let mut estream = ino.into_event_stream(evbuf).unwrap();
    let mut watches = HashMap::<WatchDescriptor,String>::new();
    loop {
        tokio::select! {
            eopt = estream.next() => {
                match eopt {
                    Some(eres) => {
                        let ev = eres.unwrap();
                        let created = ev.mask == inotify::EventMask::CREATE;
                        let deleted = ev.mask == inotify::EventMask::DELETE;
                        match(ev.name, watches.get(&ev.wd)) {
                            (Some(name),Some(dir)) => {
                                let path = Path::new(dir).join(name).to_string_lossy().to_string();
                                    if created {
                                        tx.send(Inochan::Create(path)).unwrap();
                                    } else if deleted {
                                        tx.send(Inochan::Delete(path)).unwrap();
                                    }
                            },
                            _ => {},
                        }
                    },
                    None => {},
                }
            }
            dirs = rx.recv() => {
                match dirs {
                    Some(Inochan::NewDirs(ls)) => {
                        watches.iter().for_each(|(wd,_)| estream.watches().remove(wd.clone()).unwrap());
                        watches.clear();
                        ls.iter().for_each(|dir|{
                            watches.insert(estream.watches().add(dir, WatchMask::CREATE|WatchMask::DELETE).unwrap(), dir.to_string());
                        });
                    },
                    _ => {},
                }
            }
        }
    }
}

impl FilePicker {

    fn run_command(self: &Self, icmd: usize) {
        let cmd = self.conf.cmds[icmd].cmd.as_str();
        self.items.iter().filter(|item| item.sel).for_each(|item| {
            let path = Path::new(item.path.as_str());
            let fname = path.file_name().unwrap().to_string_lossy();
            let part = match fname.splitn(2, '.').next() {
                Some(s) => s,
                None => &fname,
            };
            let dir = path.parent().unwrap();
            let filecmd = cmd.replace("[path]", format!("'{}'", &item.path).as_str())
                .replace("[dir]", format!("'{}'", &dir.to_string_lossy()).as_str())
                .replace("[ext]", format!(".{}", &match path.extension() {
                    Some(s)=>s.to_string_lossy(),
                    None=> std::borrow::Cow::Borrowed(""),
                }).as_str())
                .replace("[name]", format!("'{}'", &fname).as_str())
                .replace("[part]", format!("'{}'", &part).as_str());
            let cwd = dir.to_owned();
            tokio::task::spawn_blocking(move || {
                match OsCmd::new("bash").arg("-c").arg(filecmd).current_dir(cwd).output() {
                    Ok(output) => eprintln!("{}{}",
                                            unsafe{std::str::from_utf8_unchecked(&output.stdout)},
                                            unsafe{std::str::from_utf8_unchecked(&output.stderr)}),
                    Err(e) => eprintln!("Error running command: {}", e)
                };
            });
        });
    }

    fn keep_in_view(self: &mut Self, w: Rectangle, v: Rectangle) -> Command<Message> {
        let wbot = w.y + w.height;
        let abspos = if w.y < v.y {
            w.y
        } else if wbot > v.y + v.height {
           wbot - v.height
        } else { -1.0 };
        if abspos >= 0.0 {
            let offset = scrollable::AbsoluteOffset{x:0.0, y:abspos - 61.6}; //TODO: calculate top position
            return scrollable::scroll_to(self.scroll_id.clone(), offset);
        }
        Command::none()
    }

    fn click_item(self: &mut Self, i: usize, shift: bool, ctrl: bool) {

        self.last_clicked = Some(i);
        let isdir = self.items[i].isdir();
        let prevsel = self.items.iter().filter_map(|item| if item.sel { Some(item.idx) } else { None }).collect::<Vec<usize>>();
        while (self.conf.multi() || isdir) && shift && prevsel.len() > 0 {
            let prevdir = self.items[prevsel[0]].isdir();
            if prevdir != isdir {
                break;
            }
            let mut lo = self.items[i].idx;
            let mut hi = lo;
            prevsel.iter().for_each(|j| {
                lo = lo.min(self.items[*j].idx);
                hi = hi.max(self.items[*j].idx);
            });
            for j in lo..=hi {
                self.items[j].sel = self.items[j].isdir() == isdir;
            }
            return;
        }
        if !self.items[i].sel {
            self.items[i].sel = true;
        } else if prevsel.len() == 1 || ctrl {
            self.items[i].sel = false;
        }
        prevsel.into_iter().for_each(|j| {
            if !(ctrl && (self.conf.multi()||isdir)) || self.items[j].isdir() != isdir { self.items[j].sel = false; }
        });
        if self.items[i].sel {
            self.inputbar = self.items[i].path.clone();
        } else {
            self.inputbar = self.dirs[0].clone();
        }
    }

    fn load_dir(self: &mut Self) {
        let mut ret = vec![];
        self.nav_id = self.nav_id.wrapping_add(1);
        for dir in self.dirs.iter() {
            let entries: Vec<_> = std::fs::read_dir(dir.as_str()).unwrap().map(|f| f.unwrap().path()).collect();
            entries.iter().filter(|path|{ self.show_hidden ||
                !path.as_os_str().to_str().map(|s|s.rsplitn(2,'/').next().unwrap_or("").starts_with('.')).unwrap_or(false)
            }).for_each(|path| {
                ret.push(FItem::new(path.into(), self.nav_id));
            });
        }
        self.ino_updater.as_ref().unwrap().send(Inochan::NewDirs(self.dirs.clone())).unwrap();
        self.items = ret;
    }

    fn add_bookmark(self: &mut Self, dragged: usize, target: Option<i32>) {
        let item = &self.items[dragged];
        let label = item.path.rsplitn(2,'/').next().unwrap();
        match target {
            Some(i) if i >= 0 => {
                // TODO: multi-dir bookmark?
                self.conf.bookmarks.push(Bookmark::new(label, item.path.as_str()));
            },
            Some(_) => {
                self.conf.bookmarks.push(Bookmark::new(label, item.path.as_str()));
            },
            None => {},
        }
    }
}

impl Icons {
    fn new(thumbsize: f32) -> Self {
        Self {
            folder: Self::init(include_bytes!("../assets/folder.png"), thumbsize),
            unknown:  Self::init(include_bytes!("../assets/unknown.png"), thumbsize),
            doc:  Self::init(include_bytes!("../assets/document.png"), thumbsize),
            error:  Self::init(include_bytes!("../assets/error.png"), thumbsize),
        }
    }
    fn init(img_bytes: &[u8], thumbsize: f32) -> Handle {
        let img = load_from_memory(img_bytes).unwrap();
        let thumb = img.thumbnail((thumbsize * 0.9) as u32, (thumbsize * 0.9) as u32);
        let (w,h,rgba) = (thumb.width(), thumb.height(), thumb.into_rgba8());
        Handle::from_pixels(w, h, rgba.as_raw().clone())
    }
}

impl Bookmark {
    fn new(label: &str, path: &str) -> Self {
        Bookmark {
            label: label.into(),
            path: path.into(),
            id: CId::new(label.to_string()),
        }
    }
}

impl Cmd {
    fn new(label: &str, cmd: &str) -> Self {
        Cmd {
            label: label.into(),
            cmd: cmd.into(),
        }
    }
}

enum ClickType {
    Single,
    Double,
}
struct ClickTimer {
    idx: usize,
    time: Instant,
}
impl ClickTimer {
    fn click(self: &mut Self, idx: usize) -> ClickType {
        let time = Instant::now();
        let ret = if idx != self.idx || time - self.time > Duration::from_millis(300) {
            ClickType::Single
        } else {
            ClickType::Double
        };
        self.idx = idx;
        self.time = time;
        ret
    }
}

struct SelectedTheme;
impl StyleSheet for SelectedTheme {
    type Style = iced::Theme;
    fn appearance(&self, _style: &Self::Style) -> Appearance {
        let mut appearance = Appearance {
            ..Appearance::default()
        };
        appearance.background = Some(Background::Color(color!(0x990000)));
        appearance
    }
}
fn get_sel_theme() -> Container {
    Container::Custom(
        Box::new(SelectedTheme) as Box<dyn StyleSheet<Style = iced::Theme>>
    )
}

fn vid_frame(src: &str, thumbnail: Option<u32>) -> Option<Handle> {
    let mut decoder = if let Some(thumbsize) = thumbnail {
        DecoderBuilder::new(Location::File(src.into()))
            .with_resize(Resize::Fit(thumbsize, thumbsize)).build().ok()?
    } else {
        Decoder::new(Location::File(src.into())).ok()?
    };
    let (w, h) = decoder.size_out();
    let decoded = decoder.decode_iter().next()?;
    match decoded {
        Ok(frame) => {
            let rgb = frame.1.slice(ndarray::s![.., .., ..]).to_slice()?;
            let mut rgba = vec![255; rgb.len() * 4 / 3];
            for i in 0..rgb.len() / 3 { unsafe {
                let i3 = i * 3;
                let i4 = i * 4;
                *rgba.get_unchecked_mut(i4) = *rgb.get_unchecked(i3);
                *rgba.get_unchecked_mut(i4+1) = *rgb.get_unchecked(i3+1);
                *rgba.get_unchecked_mut(i4+2) = *rgb.get_unchecked(i3+2);
            }}
            Some(Handle::from_pixels(w, h, rgba))
        },
        Err(e) => {
            eprintln!("Error decoding {}: {}", src, e);
            None
        }
    }
}
