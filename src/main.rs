use img::load_from_memory;
use img;
use img::codecs::webp::WebPEncoder;
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
    keyboard::key::Named::{Shift,Control,ArrowUp,ArrowDown,ArrowLeft,ArrowRight,Enter,Backspace},
    keyboard::key::Named,
    widget::{
        horizontal_space, vertical_space, checkbox, slider,
        container::{Appearance, StyleSheet,Id as CId},
        image, image::Handle, Column, Row, text, responsive,
        Scrollable, scrollable, scrollable::{Direction,Properties},
        Button, TextInput, Text,
        column, row, mouse_area, container,
    },
    futures::{
        sink::SinkExt,
        StreamExt,
    },
    event::{self, Status, Event::{Mouse,Keyboard}},
    Point, Size,
};
use tokio::{
    fs::File, io::AsyncReadExt,
    sync::mpsc::{
        UnboundedReceiver as UReceiver,
        UnboundedSender as USender,
        unbounded_channel,
    }
};
use std::{
    ops::{Deref,DerefMut},
    fs,
    collections::HashMap,
    collections::HashSet,
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
use inotify::{Inotify, WatchMask, WatchDescriptor, EventMask};
use iced_aw::{
    menu_bar, menu_items,
    menu::{Item, Menu},
    modal, Card
};
use md5::{Md5,Digest};
use fuzzy_matcher::{self, FuzzyMatcher};
use csv;

macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

fn main() -> iced::Result {
    let conf = Config::new();
    conf.update(false);
    video_rs::init().unwrap();
    FilePicker::run(iced::Settings::with_flags(conf))
}

struct Config {
    title: String,
    path: String,
    mode: Mode,
    sort_by: i32,
    bookmarks: Vec<Bookmark>,
    cmds: Vec<Cmd>,
    thumb_size: f32,
    dpi_scale: f64,
    window_size: Size,
    dark_theme: bool,
    cache_dir: String,
    need_update: bool,
    index_file: String,
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
        let mut cache_dir = Path::new(&home).join(".cache").join("pikeru").to_string_lossy().to_string();
        let txt = std::fs::read_to_string(confpath).unwrap();
        enum S { Commands, Settings, Bookmarks }
        let mut section = S::Commands;
        let mut bookmarks = vec![];
        let mut cmds = vec![];
        let mut sort_by = 1;
        let mut thumb_size = 160.0;
        let mut window_size: Size = Size { width: 1024.0, height: 768.0 };
        let mut dark_theme = true;
        let mut dpi_scale: f32 = 1.0;
        let mut opts_missing = 7;
        let mut index_file = "/tmp/captions.csv".to_string();
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
                            "thumbnail_size" => { opts_missing -= 1; thumb_size = v.parse().unwrap() },
                            "index_file" => { opts_missing -= 1; index_file = v.into() },
                            "dpi_scale" => { opts_missing -= 1; dpi_scale = v.parse().unwrap() },
                            "theme" => { opts_missing -= 1; dark_theme = v == "dark" },
                            "cache_dir" => { opts_missing -= 1; cache_dir = v.to_string() },
                            "window_size" => {
                                opts_missing -= 1;
                                if !match str::split_once(v, 'x') {
                                    Some(wh) => match (wh.0.parse::<f32>(), wh.1.parse::<f32>()) {
                                        (Ok(w),Ok(h)) => {window_size = Size {width: w*dpi_scale, height: h*dpi_scale}; true},
                                        (_,_) => false,
                                    }
                                    None => false,
                                } {
                                    eprintln!("window_size must have format WIDTHxHEIGHT");
                                }
                            }
                            "sort_by" => {
                                opts_missing -= 1;
                                sort_by = match v {
                                    "name_desc" => 2,
                                    "time_desc" => 3,
                                    "time_asc" => 4,
                                    _ => 1,
                                }
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
            title: matches.opt_str("t").unwrap_or("File Picker".to_string()),
            cmds,
            bookmarks,
            sort_by,
            thumb_size,
            window_size,
            dark_theme,
            cache_dir,
            index_file,
            dpi_scale: dpi_scale.into(),
            need_update: opts_missing > 0,
        }
    }

    fn update(self: &Config, force: bool) {
        if !self.need_update && !force {
            return;
        }
        let mut conf = String::from("# Commands from the cmd menu will substitute the follwong values from the selected files before running, as seen in the convert examples. All paths and filenames are already quoted for you.
# [path] is full file path
# [name] is the filename without full path
# [dir] is the current directory without trailing slash
# [part] is the filename without path or extension
# [ext] is the file extension, including the period
[Commands]\n");
        self.cmds.iter().for_each(|cmd| {
            conf.push_str(&cmd.label);
            conf.push_str(" = ");
            conf.push_str(&cmd.cmd);
            conf.push('\n');
        });
        conf.push_str("\n[Settings]\n");
        conf.push_str(format!(
                "dpi_scale = {}\nwindow_size = {}x{}\nthumbnail_size = {}\ntheme = {}\nsort_by = {}\ncache_dir = {}\nindex_file= {}\n",
                self.dpi_scale as i32,
                self.window_size.width as i32, self.window_size.height as i32,
                self.thumb_size as i32,
                if self.dark_theme { "dark" } else { "light" },
                match self.sort_by { 1=>"name_asc", 2=>"name_desc", 3=>"time_asc", 4=>"time_desc", _=>"" },
                self.cache_dir,
                self.index_file
                ).as_str());
        conf.push_str("\n[Bookmarks]\n");
        self.bookmarks.iter().for_each(|bm| {
            conf.push_str(&bm.label);
            conf.push_str(" = ");
            conf.push_str(&bm.path);
            conf.push('\n');
        });
        let home = std::env::var("HOME").unwrap();
        let confpath = Path::new(&home).join(".config").join("pikeru.conf");
        fs::write(confpath, conf.as_bytes()).unwrap();
        if !force {
            eprintln!("updated config file with new setting");
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

enum SearchEvent {
    NewItems(Vec<String>),
    NewView(Vec<usize>),
    Results(Vec<(usize, i64)>),
    Search(String),
}

#[derive(Debug, Clone)]
enum Message {
    LoadDir,
    LoadBookmark(usize),
    Select(SelType),
    OverWriteOK,
    Cancel,
    UpDir,
    DownDir,
    NewDir(bool),
    NewDirInput(String),
    Init((USender<FItem>, USender<Inochan>, USender<SearchEvent>)),
    NextItem(FItem),
    LoadThumbs,
    LeftClick(usize),
    MiddleClick(usize),
    RightClick(i64),
    PathTxtInput(String),
    SearchTxtInput(String),
    Shift(bool),
    Ctrl(bool),
    DropBookmark(usize, Point),
    HandleZones(usize, Vec<(Id, iced::Rectangle)>),
    NextImage(i64),
    Scrolled(scrollable::Viewport),
    PositionInfo(i32, Rectangle, Rectangle),
    Sort(i32),
    ArrowKey(Named),
    ShowHidden(bool),
    RunCmd(usize),
    InoDelete(String),
    InoCreate(String),
    Thumbsize(f32),
    CloseModal,
    SearchResult(Vec<(usize, i64)>),
    Dummy,
}

enum SubState {
    Starting,
    Ready((UReceiver<FItem>,UReceiver<Inochan>,UReceiver<SearchEvent>)),
}

#[derive(Debug, Clone, Default, PartialEq)]
enum FType {
    File,
    Image,
    Dir,
    #[default]
    Unknown,
    NotExist,
}

#[derive(Debug, Clone, Default)]
struct FItemb {
    path: String,
    label: String,
    ftype: FType,
    handle: Option<Handle>,
    items_idx: usize,
    display_idx: usize,
    sel: bool,
    nav_id: u8,
    view_id: u8,
    mtime: f32,
    vid: bool,
    size: u64,
    hidden: bool,
}
#[derive(Debug, Clone, Default)]
struct FItem(Box<FItemb>);
impl Deref for FItem {
    type Target = FItemb;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for FItem {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

struct Icons {
    folder: Handle,
    doc: Handle,
    unknown: Handle,
    error: Handle,
    cache_dir: String,
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

enum FModal {
    None,
    NewDir,
    OverWrite,
    Error(String),
}

#[derive(PartialEq, Debug, Clone)]
enum SelType {
    Button,
    Click,
    TxtEntr,
}

#[derive(Default)]
struct LastClicked {
    iidx: usize,
    didx: usize,
    new: bool,
    size: Option<String>,
}

struct FilePicker {
    conf: Config,
    scroll_id: scrollable::Id,
    items: Vec<FItem>,
    displayed: Vec<usize>,
    dirs: Vec<String>,
    pathbar: String,
    searchbar: String,
    thumb_sender: Option<USender<FItem>>,
    nproc: usize,
    last_loaded: usize,
    last_clicked: LastClicked,
    icons: Arc<Icons>,
    clicktimer: ClickTimer,
    ctrl_pressed: bool,
    shift_pressed: bool,
    nav_id: u8,
    view_id: u8,
    show_hidden: bool,
    view_image: (usize, Option<Handle>),
    scroll_offset: scrollable::AbsoluteOffset,
    ino_updater: Option<USender<Inochan>>,
    search_commander: Option<USender<SearchEvent>>,
    save_filename: Option<String>,
    select_button: String,
    new_dir: String,
    modal: FModal,
    dir_history: Vec<Vec<String>>,
    content_width: f32,
}

impl Application for FilePicker {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = iced::Theme;
    type Flags = Config;

    fn new(conf: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let pathstr = conf.path.clone();
        let path = Path::new(&pathstr);
        let window_size = conf.window_size;
        let startdir = if path.is_dir() {
            path.to_string_lossy()
        } else {
            path.parent().unwrap().to_string_lossy()
        };
        let ts = conf.thumb_size;
        let save_filename = if conf.saving() {
            if path.is_dir() {
                None
            } else {
                Some(path.file_name().unwrap().to_string_lossy().to_string())
            }
        } else {
            None
        };
        let select_button = match conf.mode {
            Mode::Files|Mode::File => "Open",
            Mode::Save => "Save",
            Mode::Dir => "Selecct",
        }.to_string();
        let cache_dir = conf.cache_dir.clone();
        (
            Self {
                conf,
                items: vec![],
                displayed: vec![],
                thumb_sender: None,
                nproc: num_cpus::get() * 2,
                dirs: vec![startdir.to_string()],
                last_loaded: 0,
                last_clicked: LastClicked::default(),
                pathbar: String::new(),
                searchbar: String::new(),
                icons: Arc::new(Icons::new(ts, cache_dir)),
                clicktimer: ClickTimer{ idx:0, time: Instant::now() - Duration::from_secs(1)},
                ctrl_pressed: false,
                shift_pressed: false,
                scroll_id: scrollable::Id::unique(),
                nav_id: 0,
                view_id: 0,
                show_hidden: false,
                view_image: (0, None),
                scroll_offset: scrollable::AbsoluteOffset{x: 0.0, y: 0.0},
                ino_updater: None,
                search_commander: None,
                save_filename,
                select_button,
                modal: FModal::None,
                new_dir: String::new(),
                dir_history: vec![],
                content_width: 0.0,
            },
            iced::window::resize(iced::window::Id::MAIN, window_size)
        )
    }

    fn theme(&self) -> iced::Theme {
        if self.conf.dark_theme {
            iced::Theme::Dark
        } else {
            iced::Theme::Light
        }
    }

    fn title(&self) -> String {
        self.conf.title.clone()
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::Thumbsize(size) => {
                self.conf.thumb_size = size;
            },
            Message::InoCreate(file) => {
                let mut item = FItem::new(file.as_str().into(), self.nav_id);
                let len = self.items.len();
                item.display_idx = self.displayed.len();
                self.displayed.push(len);
                self.items.push(FItem::default());
                item.items_idx = len;
                tokio::spawn(item.load(self.thumb_sender.as_ref().unwrap().clone(),
                                       self.icons.clone(), self.conf.thumb_size as u32));
            },
            Message::InoDelete(file) => {
                if let Some(i) = self.items.iter().position(|x|x.path == file) {
                    let dix = self.itod(i);
                    self.items.iter_mut().for_each(|m|{
                        if m.items_idx >= i { m.items_idx-=1 };
                        if m.display_idx >= dix { m.display_idx-=1 };
                    });
                    self.displayed.iter_mut().for_each(|m| if *m >= i { *m-=1 });
                    self.items.remove(i);
                    self.displayed.remove(dix);
                    self.update_searcher_items(self.items.iter().map(|item|item.path.clone()).collect());
                }
            },
            Message::RunCmd(i) => self.run_command(i),
            Message::Dummy => {},
            Message::ShowHidden(show) => {
                self.show_hidden = show;
                let displayed = self.items.iter().enumerate().filter_map(|(i,item)| {
                    if show || !item.hidden { Some(i)
                    } else { None }
                }).collect();
                if self.searchbar.is_empty() {
                    self.displayed = displayed;
                    return self.update(Message::Sort(self.conf.sort_by));
                } else {
                    self.update_searcher_visible(displayed);
                }
            },
            Message::Sort(i) => {
                match i {
                    1 => self.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = self.items.get_unchecked(*a);
                        let y = self.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||x.path.cmp(&y.path))
                    }),
                    2 => self.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = self.items.get_unchecked(*a);
                        let y = self.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||y.path.cmp(&x.path))
                    }),
                    3 => self.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = self.items.get_unchecked(*a);
                        let y = self.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||y.mtime.partial_cmp(&x.mtime).unwrap())
                    }),
                    4 => self.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = self.items.get_unchecked(*a);
                        let y = self.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||x.mtime.partial_cmp(&y.mtime).unwrap())
                    }),
                    _ => unreachable!(),
                };
                self.displayed.iter().enumerate().for_each(|(i,j)|unsafe{self.items.get_unchecked_mut(*j)}.display_idx = i);
                self.conf.sort_by = i;
                return self.update(Message::LoadThumbs);
            },
            Message::PositionInfo(elem, widget, viewport) => {
                match elem {
                    1 => {
                        if self.last_clicked.new {
                            self.last_clicked.new = false;
                            return self.keep_in_view(widget, viewport);
                        }
                    },
                    2 => self.content_width = widget.width,
                    _ => {},
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
            Message::Init((fichan, inochan, search_results)) => {
                let (txctl, rxctl) = unbounded_channel::<Inochan>();
                let (txsrch, search_commands) = unbounded_channel::<SearchEvent>();
                tokio::spawn(watch_inotify(rxctl, inochan));
                self.search_commander = Some(txsrch);
                self.ino_updater = Some(txctl);
                self.thumb_sender = Some(fichan);
                tokio::task::spawn(search_loop(search_commands, search_results, mem::take(&mut self.conf.index_file)));
                return self.update(Message::LoadDir);
            },
            Message::Scrolled(viewport) => self.scroll_offset = viewport.absolute_offset(),
            Message::PathTxtInput(txt) => self.pathbar = txt,
            Message::SearchTxtInput(txt) => {
                self.searchbar = txt;
                if self.searchbar.is_empty() {
                    self.displayed = self.items.iter().enumerate().filter_map(|(i,item)| {
                        if self.show_hidden || !item.path.rsplitn(2,'/').next().unwrap_or("").starts_with('.') {
                            Some(i)
                        } else {None}
                    }).collect();
                    return self.update(Message::Sort(self.conf.sort_by));
                } else {
                    self.search_commander.as_ref().unwrap().send(SearchEvent::Search(self.searchbar.clone())).unwrap();
                }
            },
            Message::SearchResult(mut res) => {
                res.sort_by(|a,b| unsafe {
                    let x = self.items.get_unchecked(a.0);
                    let y = self.items.get_unchecked(b.0);
                    y.isdir().cmp(&x.isdir()).then_with(||b.1.cmp(&a.1))
                });
                self.displayed = res.into_iter().enumerate().map(|(i,r)|{
                    self.items[r.0].display_idx = i;
                    r.0
                }).collect();
                return self.update(Message::LoadThumbs);
            }
            Message::Ctrl(pressed) => self.ctrl_pressed = pressed,
            Message::Shift(pressed) => self.shift_pressed = pressed,
            Message::ArrowKey(key) => {
                let didx = if self.items.iter().any(|item|item.sel) {
                    let maxcols = (((self.content_width-130.0) / self.conf.thumb_size)+1.0).max(1.0) as i64;
                    let i = self.last_clicked.didx as i64;
                    match key {
                        ArrowUp => i - maxcols,
                        ArrowDown => i + maxcols,
                        ArrowLeft => i - 1,
                        ArrowRight => i + 1,
                        _ => -1,
                    }
                } else { 0 };
                if self.view_image.1 != None {
                    let step = didx - (self.last_clicked.didx as i64);
                    return self.update(Message::NextImage(step));
                }
                if didx >= 0 && didx < self.displayed.len() as i64 {
                    self.click_item(self.dtoi(didx as usize), self.shift_pressed, self.ctrl_pressed, false);
                }
            },
            Message::LoadThumbs => {
                let mut max_load = self.nproc.min(self.displayed.len());
                self.view_id = self.view_id.wrapping_add(1);
                let mut di: usize = 0;
                while di < self.displayed.len() && max_load > 0 {
                    let ii = self.displayed[di];
                    if self.items[ii].not_loaded() {
                        let mut item = mem::take(&mut self.items[ii]);
                        item.view_id = self.view_id;
                        tokio::task::spawn(item.load(
                                    self.thumb_sender.as_ref().unwrap().clone(), self.icons.clone(), self.conf.thumb_size as u32));
                        max_load -= 1;
                    }
                    di += 1;
                }
                self.last_loaded = di;
            },
            Message::NextItem(doneitem) => {
                if doneitem.nav_id == self.nav_id {
                    if doneitem.view_id == self.view_id {
                        let mut prev = self.last_loaded;
                        while prev < self.displayed.len() {
                            let i = self.dtoi(prev);
                            if self.items[i].not_loaded() {
                                let mut nextitem = mem::take(&mut self.items[i]);
                                nextitem.view_id = self.view_id;
                                tokio::task::spawn(nextitem.load(
                                        self.thumb_sender.as_ref().unwrap().clone(), self.icons.clone(), self.conf.thumb_size as u32));
                                break;
                            }
                            prev += 1;
                        }
                        self.last_loaded = prev + 1;
                    }
                    let j = doneitem.items_idx;
                    self.items[j] = doneitem;
                }
            },
            Message::LoadBookmark(idx) => {
                self.dir_history.push(mem::take(&mut self.dirs));
                self.dirs = vec![self.conf.bookmarks[idx].path.clone()];
                self.scroll_offset.y = 0.0;
                return self.update(Message::LoadDir);
            },
            Message::LoadDir => {
                self.view_image = (0, None);
                self.pathbar = match &self.save_filename {
                    Some(fname) => Path::new(&self.dirs[0]).join(fname).to_string_lossy().to_string(),
                    None => self.dirs[0].clone(),
                };
                self.load_dir();
                let _ = self.update(Message::Sort(self.conf.sort_by));
                return scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::START);
            },
            Message::DownDir => {
                if let Some(dirs) = self.dir_history.pop() {
                    self.dirs = dirs;
                    self.scroll_offset.y = 0.0;
                    return self.update(Message::LoadDir);
                }
            },
            Message::UpDir => {
                let dirs = mem::take(&mut self.dirs);
                self.dirs = dirs.iter().map(|dir| {
                    let path = Path::new(dir.as_str());
                    match path.parent() {
                        Some(par) => par.as_os_str().to_str().unwrap().to_string(),
                        None => dir.clone(),
                    }
                }).unique_by(|s|s.to_owned()).collect();
                self.dir_history.push(dirs);
                return self.update(Message::LoadDir);
            },
            Message::NewDir(confirmed) => if confirmed {
                    let path = Path::new(&self.dirs[0]).join(&self.new_dir);
                    if let Err(e) = std::fs::create_dir_all(&path) {
                        let msg = format!("Error creating directory: {:?}", e);
                        self.modal = FModal::Error(msg);
                    } else {
                        self.modal = FModal::None;
                    }
                } else {
                    self.modal = FModal::NewDir;
                },
            Message::NewDirInput(dir) => self.new_dir = dir,
            Message::CloseModal => self.modal = FModal::None,
            Message::MiddleClick(iidx) => self.click_item(iidx, false, true, false),
            Message::LeftClick(iidx) => {
                match self.clicktimer.click(iidx) {
                    ClickType::Single => self.click_item(iidx, self.shift_pressed, self.ctrl_pressed, iidx == self.view_image.0),
                    ClickType::Double => {
                        self.items[iidx].sel = true;
                        return self.update(Message::Select(SelType::Click));
                    },
                }
            },
            Message::RightClick(iidx) => {
                if iidx >= 0 {
                    let iidx = iidx as usize;
                    let item = &self.items[iidx];
                    if item.ftype == FType::Image {
                        self.view_image = (item.items_idx, item.preview());
                        self.click_item(iidx, false, false, true);
                    } else {
                        self.click_item(iidx as usize, true, false, false);
                    }
                } else {
                    self.view_image = (0, None);
                    return scrollable::scroll_to(self.scroll_id.clone(), self.scroll_offset);
                }
            },
            Message::NextImage(step) => {
                if self.view_image.1 != None {
                    let mut didx = self.itod(self.view_image.0) as i64;
                    while (step<0 && didx>0) || (step>0 && didx<((self.displayed.len()-1) as i64)) {
                        didx = (didx as i64) + step;
                        if didx<0 || didx as usize>=self.displayed.len() {
                            return Command::none();
                        }
                        let di = didx as usize;
                        let ii = self.dtoi(di);
                        if self.items[ii].ftype == FType::Image {
                            let img = self.items[ii].preview();
                            if img != None {
                                self.view_image = (self.dtoi(di), img);
                                return self.update(Message::LeftClick(self.view_image.0));
                            }
                        }
                    }
                }
            },
            Message::OverWriteOK => {
                println!("{}", self.pathbar);
                process::exit(0);
            },
            Message::Select(seltype) => {
                if self.conf.saving() {
                    if !self.pathbar.is_empty() {
                        let result = Path::new(&self.pathbar);
                        if result.is_file() {
                            self.modal = FModal::OverWrite;
                        } else if result.is_dir() {
                            self.dir_history.push(mem::take(&mut self.dirs));
                            self.dirs = vec![self.pathbar.clone()];
                            self.scroll_offset.y = 0.0;
                            return self.update(Message::LoadDir);
                        } else {
                            println!("{}", self.pathbar);
                            process::exit(0);
                        }
                    }
                } else {
                    let pb =  FItem::new(PathBuf::from(&self.pathbar), self.nav_id);
                    let sels: Vec<&FItem> = match seltype {
                        SelType::TxtEntr => vec![&pb],
                        _ => self.items.iter().filter(|item| item.sel ).collect(),
                    };
                    if sels.len() != 0 {
                        match sels[0].ftype {
                            FType::Dir => {
                                if self.conf.dir() && sels.len() == 1 && seltype == SelType::Button {
                                    println!("{}", sels[0].path);
                                    process::exit(0);
                                } else {
                                    self.dirs = sels.iter().filter_map(|item| match item.ftype {
                                        FType::Dir => Some(item.path.clone()), _ => None}).collect();
                                    return self.update(Message::LoadDir);
                                }
                            },
                            FType::NotExist => {},
                            _ => {
                                println!("{}", sels.iter().map(|item|item.path.as_str()).join("\n"));
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
        self.conf.dpi_scale
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        let mut state = SubState::Starting;
        let items = subscription::channel("", 100, |mut messager| async move {
            loop {
                match &mut state {
                    SubState::Starting => {
                        let (fi_sender, fi_reciever) = unbounded_channel::<FItem>();
                        let (ino_sender, ino_receiver) = unbounded_channel::<Inochan>();
                        let (search_sender, search_receiver) = unbounded_channel::<SearchEvent>();
                        messager.send(Message::Init((fi_sender, ino_sender, search_sender))).await.unwrap();
                        state = SubState::Ready((fi_reciever,ino_receiver,search_receiver));
                    }
                    SubState::Ready((thumb_recv, ino_recv, search_recv)) => {
                        tokio::select! {
                            res = search_recv.recv() => {
                                if let Some(SearchEvent::Results(matches)) = res {
                                    messager.send(Message::SearchResult(matches)).await.unwrap();
                                }
                            },
                            item = thumb_recv.recv() => messager.send(Message::NextItem(item.unwrap())).await.unwrap(),
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
        let events = event::listen_with(|evt, stat| {
            if stat == Status::Ignored {
                match evt {
                    Mouse(ButtonPressed(Back)) => Some(Message::UpDir),
                    Mouse(ButtonPressed(Forward)) => Some(Message::DownDir),
                    Mouse(WheelScrolled{ delta: ScrollDelta::Lines{ y, ..}}) => Some(Message::NextImage(if y<0.0 {1} else {-1})),
                    Keyboard(KeyPressed{ key: Key::Named(Enter), .. }) => Some(Message::Select(SelType::Click)),
                    Keyboard(KeyPressed{ key: Key::Named(Shift), .. }) => Some(Message::Shift(true)),
                    Keyboard(KeyReleased{ key: Key::Named(Shift), .. }) => Some(Message::Shift(false)),
                    Keyboard(KeyPressed{ key: Key::Named(Control), .. }) => Some(Message::Ctrl(true)),
                    Keyboard(KeyReleased{ key: Key::Named(Control), .. }) => Some(Message::Ctrl(false)),
                    Keyboard(KeyPressed{ key: Key::Named(ArrowUp), .. }) => Some(Message::ArrowKey(ArrowUp)),
                    Keyboard(KeyPressed{ key: Key::Named(ArrowDown), .. }) => Some(Message::ArrowKey(ArrowDown)),
                    Keyboard(KeyPressed{ key: Key::Named(ArrowLeft), .. }) => Some(Message::ArrowKey(ArrowLeft)),
                    Keyboard(KeyPressed{ key: Key::Named(ArrowRight), .. }) => Some(Message::ArrowKey(ArrowRight)),
                    Keyboard(KeyPressed{ key: Key::Named(Backspace), .. }) => Some(Message::UpDir),
                    _ => None,
                }
            } else { None }
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
                    match &self.last_clicked.size {
                        Some(size) => row![Text::new(size), horizontal_space()], None => row![]
                    },
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
                                    (text("Thumbnail size"))
                                    (slider(50.0..=500.0, self.conf.thumb_size, Message::Thumbsize))
                                    )))
                    ].spacing(1.0),
                    top_button("New Dir", 80.0, Message::NewDir(false)),
                    top_button("Up Dir", 80.0, Message::UpDir),
                    top_button("Cancel", 100.0, Message::Cancel),
                    top_button(&self.select_button, 100.0, Message::Select(SelType::Button))
                ].spacing(1),
                row![
                TextInput::new("directory or file path", self.pathbar.as_str())
                    .on_input(Message::PathTxtInput)
                    .on_paste(Message::PathTxtInput)
                    .on_submit(Message::Select(SelType::TxtEntr))
                    .width(Length::FillPortion(8)),
                TextInput::new("search", self.searchbar.as_str())
                    .on_input(Message::SearchTxtInput)
                    .on_paste(Message::SearchTxtInput)
                    .width(Length::FillPortion(2))
                ]
            ].align_items(iced::Alignment::End).width(Length::Fill);
            let bookmarks = self.conf.bookmarks.iter().enumerate().fold(column![], |col,(i,bm)| {
                        col.push(Button::new(
                                    container(
                                        text(bm.label.as_str())
                                           .horizontal_alignment(alignment::Horizontal::Center)
                                           .width(Length::Fill)).id(bm.id.clone()))
                                           .style(style::side_but_theme())
                                     .on_press(Message::LoadBookmark(i)))
                    }).push(container(vertical_space()).height(Length::Fill).width(Length::Fill)
                            .id(CId::new("bookmarks"))).width(Length::Fixed(120.0));

            let content: iced::Element<'_, Self::Message> = if let Some(handle) = &self.view_image.1 {
                mouse_area(container(image(handle.clone())
                                    .width(Length::Fill)
                                    .height(Length::Fill))
                               .align_x(alignment::Horizontal::Center)
                               .align_y(alignment::Vertical::Center)
                               .width(Length::Fill).height(Length::Fill))
                    .on_right_press(Message::RightClick(-1))
                    .on_release(Message::LeftClick(self.view_image.0))
                    .into()
            } else {
                let maxcols = ((size.width-130.0) / self.conf.thumb_size).max(1.0) as usize;
                let num_rows = self.displayed.len() / maxcols + if self.displayed.len() % maxcols != 0 { 1 } else { 0 };
                let mut rows = Column::new();
                for i in 0..num_rows {
                    let start = i * maxcols;
                    let mut row = Row::new().width(Length::Fill);
                    for j in 0..maxcols {
                        let idx = start + j;
                        if idx < self.displayed.len() {
                            row = row.push(unsafe{
                                self.items.get_unchecked(*self.displayed.get_unchecked(idx))
                            }.display(&self.last_clicked, self.conf.thumb_size));
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
            let mainview = column![
                ctrlbar,
                row![bookmarks, wrapper::locator(content).on_info(|a,b|Message::PositionInfo(2,a,b))],
            ];
            match self.modal {
                FModal::None => mainview.into(),
                FModal::Error(ref msg) => modal(mainview, Some(Card::new(
                            text("Error"), text(msg)).max_width(500.0))
                    )
                    .backdrop(Message::CloseModal)
                    .on_esc(Message::CloseModal)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                FModal::OverWrite => modal(mainview, Some(Card::new(
                        text("File exists. Overwrite?"),
                        row![
                            Button::new("Overwrite").on_press(Message::OverWriteOK),
                            Button::new("Cancel").on_press(Message::CloseModal),
                        ].spacing(5.0)).max_width(500.0))
                    )
                    .backdrop(Message::CloseModal)
                    .on_esc(Message::CloseModal)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                FModal::NewDir => modal(mainview, Some(Card::new(
                        text("Enter new directory name"),
                        column![
                            TextInput::new("Untitled", self.new_dir.as_str())
                                .on_input(Message::NewDirInput)
                                .on_submit(Message::NewDir(true))
                                .on_paste(Message::NewDirInput),
                            row![
                                Button::new("Create").on_press(Message::NewDir(true)),
                                Button::new("Cancel").on_press(Message::CloseModal),
                            ].spacing(5.0)
                        ]
                        ).max_width(500.0)
                        .on_close(Message::CloseModal))
                    )
                    .backdrop(Message::CloseModal)
                    .on_esc(Message::CloseModal)
                    .align_y(alignment::Vertical::Center)
                    .into(),
            }
        }).into()
    }
}

fn menu_button(txt: &str, msg: Message) -> Element<'static, Message> {
    Button::new(container(text(txt)
                .width(Length::Fill)
                .horizontal_alignment(alignment::Horizontal::Center)))
        .style(style::top_but_theme())
        .on_press(msg).into()
}
fn top_button(txt: &str, size: f32, msg: Message) -> Element<'static, Message> {
    Button::new(text(txt)
                .width(size)
                .horizontal_alignment(alignment::Horizontal::Center))
        .style(style::top_but_theme())
        .on_press(msg).into()
}

impl FItem {

    #[inline]
    fn isdir(self: &Self) -> bool { self.ftype == FType::Dir }

    #[inline]
    fn not_loaded(self: &Self) -> bool { self.handle == None && !self.path.is_empty() }

    fn display(&self, last_clicked: &LastClicked, thumbsize: f32) -> Element<'static, Message> {
        let mut col = Column::new()
            .align_items(iced::Alignment::Center)
            .width(Length::Fixed(thumbsize));
        if let Some(h) = &self.handle {
            col = col.push(image(h.clone()));
        }
        col = col.push(text(self.label.as_str()).size(13).shaping(text::Shaping::Advanced));
        let idx = self.items_idx;
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
        }.on_release(Message::LeftClick(self.items_idx))
            .on_right_press(Message::RightClick(self.items_idx as i64))
            .on_middle_press(Message::MiddleClick(self.items_idx));
        match (last_clicked.iidx, last_clicked.new) {
            (i, true) if i == idx => {
                wrapper::locator(clickable).on_info(|a,b|Message::PositionInfo(1,a,b)).into()
            },
            (_,_) => {
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
               vid_frame(self.path.as_str(), None, None)
            },
            _ => None,
        }
    }

    fn new(pth: PathBuf, nav_id: u8) -> Self {
        let (ftype, mtime, size) = match pth.metadata() {
            Ok(metadata) => {
                if metadata.is_dir() {
                    (FType::Dir, metadata.modified().unwrap(), 0)
                } else {
                    (FType::Unknown, metadata.modified().unwrap(),
                        metadata.len())
                }
            },
            Err(_) => (FType::NotExist, std::time::SystemTime::now(), 0),

        };
        let path = pth.to_string_lossy();
        let mut label = path.rsplitn(2,'/').next().unwrap().to_string();
        let hidden = label.starts_with('.');
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
        FItem(Box::new(FItemb {
            path: path.to_string(),
            label,
            ftype,
            items_idx: 0,
            display_idx: 0,
            handle: None,
            sel: false,
            nav_id,
            view_id: 0,
            mtime: mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f32(),
            vid: false,
            size,
            hidden,
        }))
    }

    async fn prepare_cached_thumbnail(self: &Self, path: &str, vid: bool, thumbsize: u32, icons: Arc<Icons>) -> Option<Handle> {
        let mut hasher = Md5::new();
        hasher.update(path.as_bytes());
        let cache_dir = Path::new(&icons.cache_dir).join(format!("{:x}{}.webp", hasher.finalize(), thumbsize));
        if cache_dir.is_file() {
            let mut file = File::open(cache_dir).await.unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).await.unwrap_or(0);
            let img = load_from_memory(buffer.as_ref()).unwrap();
            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
            Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
        } else if vid {
            vid_frame(&path, Some(thumbsize), Some(&cache_dir))
        } else {
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
                            let mut file = std::fs::File::create_new(cache_dir).unwrap();
                            let encoder = WebPEncoder::new_lossless(&mut file);
                            encoder.encode(rgba.as_ref(), w, h, img::ExtendedColorType::Rgba8).unwrap();
                            Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
                        },
                        Err(e) => {
                            eprintln!("Error decoding image {}: {}", self.path, e);
                            None
                        },
                    }
                },
                Err(e) => {
                    eprintln!("Error reading {}: {}", self.path, e);
                    None
                },
            }
        }
    }

    async fn load(mut self, chan: USender<FItem>, icons: Arc<Icons>, thumbsize: u32) {
        if self.handle == None {
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
                            self.handle = self.prepare_cached_thumbnail(self.path.as_str(), false, thumbsize, icons.clone()).await;
                            if self.handle == None {
                                self.handle = Some(icons.error.clone());
                                FType::File
                            } else {
                                FType::Image
                            }
                        },
                        "webm"|"mkv"|"mp4"|"av1" => {
                            self.handle = self.prepare_cached_thumbnail(self.path.as_str(), true, thumbsize, icons.clone()).await;
                            if self.handle == None {
                                self.handle = Some(icons.error.clone());
                                FType::File
                            } else {
                                self.vid = true;
                                FType::Image
                            }
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
        }
        chan.send(self).unwrap();
    }
}

struct FileIdx<'a> {
    path: String,
    data: Option<&'a str>,
}

async fn search_loop(mut commands: UReceiver<SearchEvent>, result_sender: USender<SearchEvent>, index: String) {
    let mut items = vec![];
    let mut displayed = vec![];
    let rdr = csv::Reader::from_path(index).ok();
    let captions = match rdr {
        Some(mut rdr) => rdr.records().filter_map(|r|r.ok()).fold(HashMap::new(), |mut acc,rec| {
            acc.insert(rec[0].to_string(), rec[1].to_string());
            acc
        }),
        None => Default::default(),
    };
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    loop {
        match commands.recv().await {
            Some(SearchEvent::NewItems(paths)) => items = paths.into_iter().map(|path| {
                let data = captions.get(&path).map(|x|x.as_str());
                FileIdx {
                    path,
                    data,
                }
            }).collect(),
            Some(SearchEvent::NewView(didxs)) => displayed = didxs,
            Some(SearchEvent::Search(term)) => {
                let results = displayed.iter().filter_map(|i| {
                    let item = &items[*i];
                    let name_match = matcher.fuzzy_match(item.path.as_str(), term.as_str());
                    let data_match = match item.data {
                        Some(data) => matcher.fuzzy_match(data, term.as_str()),
                        None => None,
                    };
                    match (name_match, data_match) {
                        (Some(a), Some(b)) => Some((*i, a.max(b))),
                        (Some(a), None) => Some((*i, a)),
                        (None, Some(b)) => Some((*i, b)),
                        (None, None) => None,
                    }
                }).collect::<Vec<_>>();
                result_sender.send(SearchEvent::Results(results)).unwrap();
            },
            _ => unreachable!(),
        }
    }
}

enum Inochan {
    NewDirs(Vec<String>),
    Delete(String),
    Create(String),
}
async fn watch_inotify(mut rx: UReceiver<Inochan>, tx: USender<Inochan>) {
    let ino = Inotify::init().expect("Error initializing inotify instance");
    let evbuf = [0; 1024];
    let mut estream = ino.into_event_stream(evbuf).unwrap();
    struct Dir {
        name: String,
        created: HashSet<std::ffi::OsString>,
    }
    let mut watches = HashMap::<WatchDescriptor,Dir>::new();
    loop {
        tokio::select! {
            eopt = estream.next() => {
                match eopt {
                    Some(eres) => {
                        let ev = eres.unwrap();
                        let create_file = ev.mask == EventMask::CREATE;
                        let create_dir = ev.mask == EventMask::CREATE|EventMask::ISDIR;
                        let write_file = ev.mask.contains(EventMask::CLOSE_WRITE);
                        let deleted = ev.mask.contains(EventMask::DELETE);
                        match(ev.name, watches.get_mut(&ev.wd)) {
                            (Some(name),Some(dir)) => {
                                let path = Path::new(&dir.name).join(name.clone()).to_string_lossy().to_string();
                                    if create_dir {
                                        tx.send(Inochan::Create(path)).unwrap();
                                    } else if create_file {
                                        dir.created.insert(name);
                                    } else if write_file && dir.created.contains(&name) {
                                        dir.created.remove(&name);
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
                            watches.insert(estream.watches().add(dir,
                                                                 WatchMask::CREATE|
                                                                 WatchMask::CLOSE_WRITE|
                                                                 WatchMask::DELETE).unwrap(),
                                           Dir{name:dir.to_string(), created:Default::default()});
                        });
                    },
                    _ => {},
                }
            }
        }
    }
}

fn shquote(s: &str) -> String {
    if s.contains("\"") {
        return format!("'{}'", s);
    }
    return format!("\"{}\"", s);
}

impl FilePicker {

    #[inline]
    fn itod(self: &Self, i: usize) -> usize { self.items[i].display_idx }
    #[inline]
    fn dtoi(self: &Self, i: usize) -> usize { self.displayed[i] }

    fn run_command(self: &Self, icmd: usize) {
        let cmd = self.conf.cmds[icmd].cmd.as_str();
        self.items.iter().filter(|item| item.sel).for_each(|item| {
            let path = Path::new(item.path.as_str());
            let fname = path.file_name().unwrap().to_string_lossy();
            let part = match fname.splitn(2, '.').next() {
                Some(s) => s,
                None => &fname,
            };
            let fname = shquote(fname.as_ref());
            let part = shquote(part.as_ref());
            let dir = path.parent().unwrap();
            let filecmd = cmd.replace("[path]", shquote(&item.path).as_str())
                .replace("[dir]", &shquote(&dir.to_string_lossy()).as_str())
                .replace("[ext]", format!(".{}", &match path.extension() {
                    Some(s)=>s.to_string_lossy(),
                    None=> std::borrow::Cow::Borrowed(""),
                }).as_str())
                .replace("[name]", &fname)
                .replace("[part]", &part);
            let cwd = dir.to_owned();
            eprintln!("CMD:{}", filecmd);
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

    fn click_item(self: &mut Self, iidx: usize, shift: bool, ctrl: bool, always_sel: bool) {

        let isdir = self.items[iidx].isdir();
        self.last_clicked = LastClicked{
            new: true,
            iidx,
            didx: self.items[iidx].display_idx,
            size: if isdir { None } else {
                let bytes = self.items[iidx].size as f64;
                Some(if bytes > 1073741824.0 { format!("{:.2}GB", bytes/1073741824.0 ) }
                else if bytes > 1048576.0 { format!("{:.1}MB", bytes/1048576.0 ) }
                else if bytes > 1024.0 { format!("{:.0}KB", bytes/1024.0 ) }
                else { format!("{:.0}B", bytes) })
                }
        };
        let prevsel = self.items.iter().filter_map(|item| if item.sel { Some(item.items_idx) } else { None }).collect::<Vec<usize>>();
        while (self.conf.multi() || isdir) && shift && prevsel.len() > 0 {
            let prevdir = self.items[prevsel[0]].isdir();
            if prevdir != isdir {
                break;
            }
            let mut lo = self.items[iidx].display_idx;
            let mut hi = lo;
            prevsel.iter().for_each(|j| {
                lo = lo.min(self.items[*j].display_idx);
                hi = hi.max(self.items[*j].display_idx);
            });
            for j in lo..=hi {
                self.items[self.displayed[j]].sel = self.items[self.displayed[j]].isdir() == isdir;
            }
            return;
        }
        if always_sel || !self.items[iidx].sel {
            self.items[iidx].sel = true;
        } else if prevsel.len() == 1 || ctrl {
            self.items[iidx].sel = false;
        }
        prevsel.into_iter().filter(|j|*j != iidx).for_each(|j| {
            if !(ctrl && (self.conf.multi()||isdir)) || self.items[j].isdir() != isdir { self.items[j].sel = false; }
        });
        self.pathbar = if self.items[iidx].sel {
            self.items[iidx].path.clone()
        } else {
            self.dirs[0].clone()
        };
    }

    fn load_dir(self: &mut Self) {
        let mut ret = vec![];
        let mut displayed = vec![];
        let mut inodirs = vec![];
        self.nav_id = self.nav_id.wrapping_add(1);
        for dir in self.dirs.iter() {
            match std::fs::read_dir(dir.as_str()) {
                Ok(rd) => {
                    inodirs.push(dir.clone());
                    rd.map(|f| f.unwrap().path()).for_each(|path| {
                        ret.push(FItem::new(path.into(), self.nav_id));
                        if self.show_hidden || !ret.last().unwrap().hidden {
                            displayed.push(ret.len()-1);
                        }
                    });
                },
                Err(e) => eprintln!("Error reading dir {}: {}", dir, e),
            }
        }
        self.searchbar.clear();
        self.ino_updater.as_ref().unwrap().send(Inochan::NewDirs(inodirs)).unwrap();
        self.items = ret;
        self.items.iter_mut().enumerate().for_each(|(i,item)|item.items_idx = i);
        self.displayed = displayed;
        self.update_searcher_items(self.items.iter().map(|item|item.path.clone()).collect());
    }

    fn update_searcher_items(self: &mut Self, searchable: Vec<String>) {
        if let Some(ref mut sender) = self.search_commander {
            sender.send(SearchEvent::NewItems(searchable)).unwrap();
            sender.send(SearchEvent::NewView(self.displayed.clone())).unwrap();
        }
    }

    fn update_searcher_visible(self: &mut Self, displaylist: Vec<usize>) {
        if let Some(ref mut sender) = self.search_commander {
            sender.send(SearchEvent::NewView(displaylist)).unwrap();
            sender.send(SearchEvent::Search(self.searchbar.clone())).unwrap();
        }
    }

    fn add_bookmark(self: &mut Self, dragged: usize, target: Option<i32>) {
        let item = &self.items[dragged];
        let label = item.path.rsplitn(2,'/').next().unwrap();
        match target {
            Some(i) if i >= 0 => {
                // TODO: multi-dir bookmark?
                self.conf.bookmarks.push(Bookmark::new(label, item.path.as_str()));
                self.conf.update(true);
            },
            Some(_) => {
                self.conf.bookmarks.push(Bookmark::new(label, item.path.as_str()));
                self.conf.update(true);
            },
            None => {},
        }
    }
}

impl Icons {
    fn new(thumbsize: f32, cache_dir: String) -> Self {
        Self {
            folder: Self::init(include_bytes!("../assets/folder.png"), thumbsize),
            unknown:  Self::init(include_bytes!("../assets/unknown.png"), thumbsize),
            doc:  Self::init(include_bytes!("../assets/document.png"), thumbsize),
            error:  Self::init(include_bytes!("../assets/error.png"), thumbsize),
            cache_dir,
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

fn vid_frame(src: &str, thumbnail: Option<u32>, safepath: Option<&PathBuf>) -> Option<Handle> {
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
            if let Some(out) = safepath {
                let mut file = std::fs::File::create_new(out).unwrap();
                let encoder = WebPEncoder::new_lossless(&mut file);
                encoder.encode(rgba.as_ref(), w, h, img::ExtendedColorType::Rgba8).unwrap();
            }
            Some(Handle::from_pixels(w, h, rgba))
        },
        Err(e) => {
            eprintln!("Error decoding {}: {}", src, e);
            None
        }
    }
}
