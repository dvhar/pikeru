use unicode_segmentation::UnicodeSegmentation;
use resvg;
use tiny_skia;
use iced_gif;
use iced_gif::widget::gif;
use img::load_from_memory;
use img;
use webp;
use num_cpus;
use itertools::Itertools;
mod wrapper;
mod iced_drop;
mod mouse;
use mouse::mouse_area;
mod style;
use iced::{
    advanced::widget::Id,
    Rectangle, Padding,
    color, Background, alignment, executor, subscription,
    Application, Command, Length, Element, theme::Container,
    mouse::Event::{ButtonPressed, WheelScrolled},
    mouse::Button::{Back,Forward},
    mouse::ScrollDelta,
    keyboard::Event::{KeyPressed,KeyReleased},
    keyboard::Key,
    keyboard::key::Named::{Shift,Control,ArrowUp,ArrowDown,ArrowLeft,ArrowRight,Enter,Backspace,PageUp,PageDown},
    keyboard::key::Named,
    widget::{
        horizontal_space, vertical_space, checkbox, slider,
        container::{Appearance, StyleSheet,Id as CId},
        image, image::Handle, Column, Row, text, responsive,
        Scrollable, scrollable, scrollable::{Direction,Properties},
        Button, TextInput, Text,
        column, row, container,
        svg, text_input
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
    collections::{HashMap,HashSet},
    fs, str, mem,
    path::{PathBuf,Path},
    process::{self, Command as OsCmd},
    sync::Arc,
    time::{Instant,Duration},
    cell::RefCell,
};
use video_rs::{Decoder, Location, DecoderBuilder, Resize};
use ndarray;
use getopts::Options;
use inotify::{Inotify, WatchMask, WatchDescriptor, EventMask};
use iced_aw::{
    menu_bar, menu_items,
    menu::{Item, Menu},
    modal, Card,
    ContextMenu,
};
use md5::{Md5,Digest};
use fuzzy_matcher::{self, FuzzyMatcher};
use zbus::{Result,proxy,Connection,blocking};
use ignore::{gitignore,Match};
use chrono::{DateTime,Utc};

const ROW_HEIGHT: f32 = 25.0;

macro_rules! die {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
        std::process::exit(1);
    }};
}

fn cli(flags: &getopts::Matches) {
    if flags.opt_present("c") {
        IndexProxy::pause_resume(false);
        std::process::exit(0);
    }
    if flags.opt_present("b") {
        IndexProxy::pause_resume(true);
        std::process::exit(0);
    }
    if flags.opt_present("v") {
        println!("1.5");
        std::process::exit(0);
    }
    let cmd = if flags.opt_present("d") {
        include_str!("../xdg_portal/unsetconfig.sh")
    } else if flags.opt_present("e") {
        include_str!("../xdg_portal/setconfig.sh")
    } else { "" };
    if !cmd.is_empty() {
        let res = std::process::Command::new("bash").arg("-c").arg(cmd).output();
        match res {
            Ok(out) if out.status.success() => println!("{}", unsafe{std::str::from_utf8_unchecked(&out.stdout)}),
            Ok(out) => unsafe {die!("Error:{}\n{}",
                std::str::from_utf8_unchecked(&out.stdout),
                std::str::from_utf8_unchecked(&out.stderr))},
            Err(e) => die!("Command failed:{}", e),
        }
        std::process::exit(0);
    }
}

fn main() -> iced::Result {
    let mut conf = Config::new();
    conf.update(false);
    video_rs::init().unwrap();
    let mut settings = iced::Settings::with_flags(conf);
    settings.window.level = iced::window::Level::AlwaysOnTop;
    settings.window.position = iced::window::Position::Centered;
    FilePicker::run(settings)
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
    need_update: bool,
    gitignore: String,
    respect_gitignore: bool,
    icon_view: bool,
    home: String,
}

impl Config {

    #[inline]
    fn saving(self: &Self) -> bool { self.mode == Mode::Save }
    #[inline]
    fn multi(self: &Self) -> bool { self.mode == Mode::Files }
    #[inline]
    fn dir(self: &Self) -> bool { self.mode == Mode::Dir }

    fn new() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut opts = Options::new();
        let pwd = std::env::var("PWD").unwrap();
        opts.optopt("t", "title", "Title of the filepicker window", "NAME");
        opts.optopt("m", "mode", "Mode of file selection. Default is files", "[file, files, save, dir]");
        opts.optopt("p", "path", "Initial path", "PATH");
        opts.optflag("c", "pause", "Pause the semantic search indexer");
        opts.optflag("b", "resume", "Resume the semantic search indexer");
        opts.optflag("d", "disable", "Configure xdg portal to not use pikeru as your system filepicker");
        opts.optflag("e", "enable", "Configure xdg portal to use pikeru as your system filepicker");
        opts.optflag("h", "help", "Show usage information");
        opts.optflag("v", "version", "Show pikeru version");
        let matches = match opts.parse(&args) {
            Ok(m) => m,
            Err(e) => die!("Bad args: {}", e),
        };
        if matches.opt_present("h") {
            println!("{}\n{}",opts.usage(&args[0]),
                "File picker config file is ~/.config/pikeru.conf.\nThe portal config file, which includes the semantic search indexer and postprocessor, is by default ~/.config/xdg-desktop-portal-pikeru/config.\nTo handle pdf and epub thumbnails, make sure pdftoppm and epub-thumbnailer are installed.");
            std::process::exit(0);
        }

        let home = std::env::var("HOME").unwrap();
        let confpath = Path::new(&home).join(".config").join("pikeru.conf").to_string_lossy().to_string();
        let txt = std::fs::read_to_string(confpath).unwrap_or("".to_string());
        #[derive(PartialEq)]
        enum S { Commands, Settings, Bookmarks, Ignore }
        let mut section = S::Commands;
        let mut bookmarks = vec![];
        let mut cmds = vec![Cmd::builtin("Delete"),
                            Cmd::builtin("Rename"),
                            Cmd::builtin("Cut"),
                            Cmd::builtin("Copy"),
                            Cmd::builtin("Paste"),
                        ];
        let mut respect_gitignore = true;
        let mut icon_view = true;
        let mut gitignore = ".git/\n".to_string();
        let mut sort_by = 1;
        let mut thumb_size = 160.0;
        let mut window_size: Size = Size { width: 1024.0, height: 768.0 };
        let mut dpi_scale: f32 = 1.0;
        let mut opts_missing = 6;
        for line in txt.lines().map(|s|s.trim()).filter(|s|s.len()>0 && !s.starts_with('#')) {
            match line {
                "[Commands]" => section = S::Commands,
                "[Settings]" => section = S::Settings,
                "[Bookmarks]" => section = S::Bookmarks,
                "[SearchIgnore]" => { section = S::Ignore; gitignore.clear(); },
                _ => {
                    let (k, v) = str::split_once(line, '=').unwrap_or(("",""));
                    let (k, v) = (k.trim(), v.trim());
                    match section {
                        S::Commands => cmds.push(Cmd::new(k, v)),
                        S::Bookmarks => bookmarks.push(Bookmark::new(k,v)),
                        S::Ignore => {gitignore += line; gitignore += "\n"; },
                        S::Settings => match k {
                            "thumbnail_size" => { opts_missing -= 1; thumb_size = v.parse().unwrap() },
                            "dpi_scale" => { opts_missing -= 1; dpi_scale = v.parse().unwrap() },
                            "respect_gitignore" => { opts_missing -= 1; respect_gitignore = v.parse().unwrap() },
                            "icon_view" => { opts_missing -= 1; icon_view = v.parse().unwrap() },
                            "window_size" => {
                                opts_missing -= 1;
                                if !match str::split_once(v, 'x') {
                                    Some(wh) => match (wh.0.parse::<f32>(), wh.1.parse::<f32>()) {
                                        (Ok(w),Ok(h)) => {window_size = Size {width: w, height: h}; true},
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
                                    "age_asc" => 3,
                                    "age_desc" => 4,
                                    _ => 1,
                                }
                            },
                            _ => {},
                        },
                    }
                },
            }
        }
        cli(&matches);
        let tpath = Path::new(&home).join(".cache").join("pikeru").join("thumbnails");
        if let Err(_) = tpath.metadata() {
            std::fs::create_dir_all(&tpath).unwrap();
        };
        if bookmarks.is_empty() {
            bookmarks.push(Bookmark::new("Home", &home));
            bookmarks.push(Bookmark::new("Downloads", Path::new(&home).join("Downloads").to_string_lossy().as_ref()));
            bookmarks.push(Bookmark::new("Documents", Path::new(&home).join("Documents").to_string_lossy().as_ref()));
            bookmarks.push(Bookmark::new("Pictures", Path::new(&home).join("Pictures").to_string_lossy().as_ref()));
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
            dpi_scale: dpi_scale.into(),
            gitignore,
            respect_gitignore,
            icon_view,
            need_update: opts_missing > 0,
            home,
        }
    }

    fn update(self: &mut Config, force: bool) {
        if !self.need_update && !force {
            return;
        }
        self.need_update = false;
        let mut conf = String::from("# Commands from the cmd menu will substitute the follwong values from the selected files before running,
# as seen in the convert examples. Paths and filenames are already quoted for you when using lowercase like [path],
# or unquoted when capitalized like [Path].
# [path] is full file path
# [name] is the filename without full path
# [dir] is the current directory without trailing slash
# [part] is the filename without path or extension
# [ext] is the file extension, including the period
[Commands]\n");
        self.cmds.iter().skip(5).for_each(|cmd| {
            conf.push_str(&cmd.label);
            conf.push_str(" = ");
            conf.push_str(&cmd.cmd);
            conf.push('\n');
        });
        conf.push_str("\n[Settings]\n");
        conf.push_str(format!(
                "dpi_scale = {}\nwindow_size = {}x{}\nthumbnail_size = {}\nsort_by = {}\nrespect_gitignore = {}\nicon_view = {}\n",
                self.dpi_scale,
                self.window_size.width as i32, self.window_size.height as i32,
                self.thumb_size as i32,
                match self.sort_by { 1=>"name_asc", 2=>"name_desc", 3=>"age_asc", 4=>"age_desc", _=>"" },
                self.respect_gitignore,
                self.icon_view).as_str());
        conf.push_str("\n# The SearchIgnore section uses gitignore syntax rather than ini.
# The respect_gitignore setting only toggles .gitignore files, not this section.\n[SearchIgnore]\n");
        conf.push_str(self.gitignore.as_str());
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
                   _ => die!("Invalid mode: {}. Need one of [file, files, save, dir]", s),
               }
           }
       }
   }
}

#[derive(Clone, Debug)]
enum SearchEvent {
    NewItems(Vec<String>, u8),
    AddItems(Vec<String>),
    NewView(Vec<usize>),
    AddView(Vec<usize>),
    AddSemantics(Vec<(String,String)>),
    Results(Vec<(usize, i64)>, u8, usize, String),
    Search(String),
}

#[derive(Debug, Clone)]
enum Message {
    LoadDir,
    LoadBookmark(usize),
    EditBookmark(usize),
    UpdateBookmark(usize),
    NewBmPathInput(String),
    NewBmLabelInput(String),
    Goto,
    Select(SelType),
    OverWriteOK,
    Cancel,
    UpDir,
    DownDir,
    NewDir(bool),
    Rename,
    NewPathInput(String),
    Init((USender<FItem>, USender<Inochan>, USender<SearchEvent>, USender<RecMsg>)),
    NextItem(FItem),
    LoadThumbs,
    LeftPreClick(usize),
    LeftClick(usize, bool),
    MiddleClick(usize),
    RightClick(i64),
    PathTxtInput(String),
    SearchTxtInput(String),
    Shift(bool),
    Ctrl(bool),
    DropBookmark(usize, Point),
    DeleteBookmark(usize),
    HandleZones(usize, Vec<(Id, iced::Rectangle)>),
    NextImage(i64),
    Scrolled(scrollable::Viewport),
    PositionInfo(Pos, Rectangle, Rectangle),
    Sort(i32),
    ChangeView,
    ArrowKey(Named),
    ShowHidden(bool),
    SetRecursive(bool),
    RunCmd(usize),
    InoDelete(String),
    InoCreate(String),
    Thumbsize(f32),
    CloseModal,
    SearchResult(Box<SearchEvent>),
    NextRecurse(Vec<FItem>, u8),
    PageUp,
    PageDown,
    Dummy,
}

#[derive(Clone, Debug)]
enum Pos {
    Content(bool),
    Item,
    Row(u32, usize),
}

enum SubState {
    Starting,
    Ready((UReceiver<FItem>,UReceiver<Inochan>,UReceiver<SearchEvent>,UReceiver<RecMsg>)),
}

#[derive(PartialEq)]
enum ImgType {
    Norm,
    Vid,
    Svg,
    Pdf,
    Epub,
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
    thumb_handle: Option<Handle>,
    items_idx: usize,
    display_idx: usize,
    sel: bool,
    nav_id: u8,
    mtime: u64,
    size: u64,
    view_id: u8,
    vid: bool,
    gif: bool,
    svg: bool,
    hidden: bool,
    recursed: bool,
    unicode: bool,
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
    audio: Handle,
    thumb_dir: String,
    settings: svg::Handle,
    updir: svg::Handle,
    newdir: svg::Handle,
    cmds: svg::Handle,
    goto: svg::Handle,
    cando_pdf: bool,
    cando_epub: bool,
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
    builtin: bool,
}

#[derive(PartialEq)]
enum FModal {
    None,
    NewDir,
    OverWrite,
    Rename(String),
    Error(String),
    EditBookmark(usize),
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
    nav_id: u8,
    size: Option<String>,
}

#[derive(PartialEq)]
enum RecState {
    Run,
    Stop,
}

#[proxy(
    interface = "org.freedesktop.impl.portal.SearchIndexer",
    default_service = "org.freedesktop.impl.portal.desktop.pikeru",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Indexer {
    async fn update(&mut self, path: &Vec<&str>) -> Result<()>;
    async fn pause_resume(&self, active: bool) -> Result<()>;
    async fn configure(&mut self, respect_gitignore: bool, ignore: &str) -> Result<()>;
}
struct IndexProxy<'a> {
    proxy: Option<IndexerProxy<'a>>,
    done: HashSet<String>,
    sql: Option<rusqlite::Connection>,
}
impl<'a> IndexProxy<'a> {

    async fn new() -> Self {
        let proxy = async {
            let conn = Connection::session().await.ok()?;
            let prox = IndexerProxy::new(&conn).await.ok()?;
            Some(prox)
        }.await;
        let home = std::env::var("HOME").unwrap();
        let idxfile = Path::new(&home).join(".cache").join("pikeru").join("index.db");
        let sql = match rusqlite::Connection::open(&idxfile) {
            Ok(con) => Some(con),
            Err(_) => None,
        };
        Self {
            proxy,
            done: HashSet::new(),
            sql,
        }
    }

    fn pause_resume(active: bool) {
        let conn = blocking::Connection::session().unwrap();
        let prox = IndexerProxyBlocking::new(&conn).unwrap();
        match (active, prox.pause_resume(active)) {
            (_,Err(e)) => eprintln!("Error:{}", e),
            (false, Ok(())) => eprintln!("Paused indexer"),
            (true, Ok(())) => eprintln!("Resumed indexer"),
        }
    }

    async fn configure(&mut self, respect_gitignore: bool, ignore: &str) {
        if let Some(ref mut prox) = self.proxy {
            match prox.configure(respect_gitignore, ignore).await {
                Ok(_) => {},
                Err(e) => {
                    eprintln!("{}", e);
                    self.proxy = None;
                },
            }
        } 
    }

    async fn update(&mut self, dirs: &Vec<String>) -> Vec<(String,String)> {
        let mut placeholders = String::new();
        let mut i = 0;
        let filtered = dirs.iter().filter(|p|{
                let needed = self.done.insert(p.to_string());
                match (needed, &self.sql) {
                    (true, Some(_)) => {
                        i += 1;
                        placeholders = format!("{}{}?{}", placeholders, if i>1 {","} else {""}, i);
                    },
                    (_,_) => {},
                }
                needed
            }).map(|s|s.as_str()).collect::<Vec::<&str>>();
        if filtered.is_empty() { return Vec::new(); }

        if let Some(ref mut prox) = self.proxy {
            match prox.update(&filtered).await {
                Ok(_) => {},
                Err(e) => {
                    eprintln!("{}", e);
                    self.proxy = None;
                },
            }
        } 
        if let Some(sql) = &self.sql {
            let qtext = format!("select concat(dir, '/', fname), description from descriptions where dir in ({})", placeholders);
            let mut query = match sql.prepare(qtext.as_str()) {
                Ok(q) => q,
                Err(_) => return Vec::new(),
            };
            query.query_map(rusqlite::params_from_iter(filtered.iter()), |row|{
                Ok((row.get(0).unwrap(), row.get(1).unwrap()))
            }).unwrap().map(|r|r.unwrap()).collect()
        } else {
            Vec::new()
        }
    }
}

enum Preview {
    None,
    Svg(svg::Handle),
    Image(Handle),
    Gif(iced_gif::Frames),
}

#[derive(Debug, Default)]
struct Rowsize {
    ready: bool,
    end_pos: f32,
    pos: f32,
}

#[derive(Debug)]
struct RowSizes {
    rows: Vec<Rowsize>,
    num_ready: usize,
    next_send: usize,
    last_recv: usize,
    view_counter: u32,
    cols: usize,
}
impl RowSizes {
    fn reset(self: &mut Self, samecols: bool) {
        self.rows.clear();
        self.next_send = 0;
        self.last_recv = 0;
        self.num_ready = 0;
        self.view_counter += 1;
        if !samecols {
            self.cols = 0;
        }
    }
    fn checkcols(self: &mut Self, newcols: usize) {
        if self.cols != newcols {
            self.reset(false);
            self.cols = newcols;
        }
    }
    fn new() -> Self {
        Self {
            rows: vec![],
            next_send: 0,
            last_recv: 0,
            num_ready: 0,
            view_counter: 0,
            cols: 0,
        }
    }
}

#[derive(Default)]
struct Measurements {
    total_height: f32,
    total_width: f32,
    max_cols: usize,
}

#[derive(Default)]
struct NewPath {
    full_path: String,
    basename: String,
}

struct FilePicker {
    conf: Config,
    scroll_id: scrollable::Id,
    items: Vec<FItem>,
    displayed: Vec<usize>,
    end_idx: usize,
    dirs: Vec<String>,
    pathbar: String,
    searchbar: String,
    search_running: bool,
    recurse_state: RecState,
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
    view_image: (usize, Preview),
    scroll_offset: scrollable::AbsoluteOffset,
    ino_updater: Option<USender<Inochan>>,
    search_commander: Option<USender<SearchEvent>>,
    recurse_updater: Option<USender<RecMsg>>,
    save_filename: Option<String>,
    select_button: String,
    new_path: NewPath,
    new_bm_label: String,
    new_bm_path: String,
    modal: FModal,
    dir_history: Vec<Vec<String>>,
    content_viewport: Rectangle,
    content_y: f32,
    content_height: f32,
    recursive_search: bool,
    show_goto: bool,
    enable_sel_button: bool,
    row_sizes: RefCell<RowSizes>,
    pos_state: RefCell<Measurements>,
    search_id: text_input::Id,
    clipboard_paths: Vec<String>,
    clipboard_cut: bool,
}

impl Application for FilePicker {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = iced::Theme;
    type Flags = Config;

    fn new(conf: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let pathstr = conf.path.clone();
        let path = Path::new(&pathstr);
        let mut window_size = conf.window_size;
        window_size.width *= conf.dpi_scale as f32;
        window_size.height *= conf.dpi_scale as f32;
        let startdir = if path.is_dir() {
            path.to_string_lossy().to_string()
        } else {
            match path.parent() {
                Some(pth) => pth.to_string_lossy().to_string(),
                None => conf.home.as_str().to_string(),
            }
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
            Mode::Dir => "Select",
        }.to_string();
        let saving = conf.saving();
        let enable_sel_button = conf.saving() || conf.dir();
        let search_id = text_input::Id::unique();
        (
            Self {
                conf,
                items: vec![],
                end_idx: 0,
                displayed: vec![],
                thumb_sender: None,
                nproc: num_cpus::get() * 2,
                dirs: vec![startdir],
                last_loaded: 0,
                last_clicked: LastClicked::default(),
                pathbar: String::new(),
                searchbar: String::new(),
                search_running: false,
                recurse_state: RecState::Stop,
                icons: Arc::new(Icons::new(ts)),
                clicktimer: ClickTimer{ idx:0, time: Instant::now() - Duration::from_secs(1), preclicked: None},
                ctrl_pressed: false,
                shift_pressed: false,
                scroll_id: scrollable::Id::unique(),
                nav_id: 0,
                view_id: 0,
                show_hidden: false,
                view_image: (0, Preview::None),
                scroll_offset: scrollable::AbsoluteOffset{x: 0.0, y: 0.0},
                ino_updater: None,
                search_commander: None,
                recurse_updater: None,
                save_filename,
                select_button,
                modal: FModal::None,
                new_path: Default::default(),
                dir_history: vec![],
                new_bm_path: String::new(),
                new_bm_label: String::new(),
                content_viewport: Rectangle::default(),
                content_height: 0.0,
                content_y: 0.0,
                recursive_search: true,
                show_goto: false,
                enable_sel_button,
                row_sizes: RefCell::new(RowSizes::new()),
                pos_state: RefCell::new(Measurements::default()),
                search_id: search_id.clone(),
                clipboard_paths: vec![],
                clipboard_cut: false,
            },
            Command::batch({
                let mut cmds = vec![iced::window::resize(iced::window::Id::MAIN, window_size)];
                if !saving { cmds.push(text_input::focus(search_id)); }
                cmds
            })
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
            Message::Thumbsize(size) => {
                self.conf.thumb_size = (size / 10.0).round() * 10.0;
                self.conf.need_update = true;
            },
            Message::InoCreate(file) => {
                let mut item = FItem::new(file.as_str().into(), self.nav_id);
                let len = self.items.len();
                item.display_idx = self.displayed.len();
                self.displayed.push(len);
                self.items.push(FItem::default());
                self.end_idx += 1;
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
                    self.end_idx -= 1;
                    self.displayed.remove(dix);
                    self.update_searcher_items(self.items.iter().map(|item|item.path.clone()).collect());
                }
            },
            Message::RunCmd(i) => self.run_command(i),
            Message::Dummy => {},
            Message::SetRecursive(rec) => {
                self.recursive_search = rec;
                self.recurse_updater.as_ref().unwrap().send(RecMsg::SetRecursive(rec)).unwrap();
                if !rec { // reset searchable items in case already recursed
                    let items = self.items[..self.end_idx].iter().map(|item|item.path.clone()).collect::<Vec<_>>();
                    let iidxs = self.items[..self.end_idx].iter().map(|item|item.items_idx).collect::<Vec<_>>();
                    if let Some(ref mut sender) = self.search_commander {
                        sender.send(SearchEvent::NewItems(items, self.nav_id)).unwrap();
                        sender.send(SearchEvent::NewView(iidxs)).unwrap();
                    }
                }
            },
            Message::ShowHidden(show) => {
                self.show_hidden = show;
                if !show {
                    self.enable_sel_button = self.conf.saving() || self.conf.dir() || self.items.iter().any(|item|item.sel);
                }
                let end = if self.searchbar.is_empty() { self.end_idx } else { self.displayed.len() };
                let displayed = self.items[..end].iter().enumerate().filter_map(|(i,item)| {
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
            Message::ChangeView => {
                self.conf.icon_view = !self.conf.icon_view;
                self.conf.need_update = true;
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
                self.conf.need_update |= i != self.conf.sort_by;
                self.conf.sort_by = i;
                self.row_sizes.borrow_mut().reset(true);
                return self.update(Message::LoadThumbs);
            },
            Message::PositionInfo(elem, widget, viewport) => {
                match elem {
                    Pos::Item => {
                        self.content_viewport = viewport;
                        if self.last_clicked.new {
                            self.last_clicked.new = false;
                            return self.keep_in_view(widget, viewport);
                        }
                    },
                    Pos::Content(clicked_offscreen) => {
                        self.content_height = widget.height;
                        self.content_y = widget.y;
                        self.content_viewport.height = widget.height;
                        if self.last_clicked.new && self.last_clicked.nav_id == self.nav_id && clicked_offscreen {
                            self.last_clicked.new = false;
                            let ii = self.last_clicked.iidx;
                            if let Some(rect) = self.itopos(ii) {
                                return self.keep_in_view(rect, self.content_viewport);
                            }
                        }
                    },
                    Pos::Row(counter, i) => {
                        let mut rs = self.row_sizes.borrow_mut();
                        if counter == rs.view_counter {
                            if rs.rows.len() <= i {
                                rs.rows.resize_with(self.num_rows(self.pos_state.borrow().max_cols),
                                                    Rowsize::default);
                            }
                            if i > rs.last_recv+1 {
                                rs.next_send = rs.last_recv+1;
                            } else if !rs.rows[i].ready {
                                rs.last_recv = i;
                                rs.num_ready += 1;
                                let pos = widget.y - self.content_y;
                                rs.rows[i] = Rowsize {
                                    ready: true,
                                    end_pos: pos + widget.height,
                                    pos,
                                };
                            }
                        }
                    },
                }
            },
            Message::Scrolled(viewport) => {
                self.update_scroll(viewport.absolute_offset().y);
            },
            Message::PageUp => {
                let current = self.scroll_offset.y;
                let offset = scrollable::AbsoluteOffset{x:0.0, y:(current - self.content_height).max(0.0)};
                self.update_scroll(offset.y);
                return scrollable::scroll_to(self.scroll_id.clone(), offset);
            },
            Message::PageDown => {
                let current = self.scroll_offset.y;
                let end = if self.conf.icon_view {
                    match self.row_sizes.borrow().rows.last() {
                        None => self.content_height,
                        Some(r) => r.end_pos,
                    }
                } else {
                    self.displayed.len() as f32 * ROW_HEIGHT
                };
                let offset = scrollable::AbsoluteOffset{x:0.0, y:(current + self.content_height)
                    .min(end - self.content_height)};
                    self.update_scroll(offset.y);
                    return scrollable::scroll_to(self.scroll_id.clone(), offset);
            },
            Message::DropBookmark(idx, cursor_pos) => {
                return iced_drop::zones_on_point(
                    move |zones| Message::HandleZones(idx, zones),
                    cursor_pos, None, None,
                );
            }
            Message::DeleteBookmark(idx) => {
                self.rem_bookmark(idx);
                self.modal = FModal::None;
            },
            Message::EditBookmark(idx) => {
                self.modal = FModal::EditBookmark(idx);
                self.new_bm_path = self.conf.bookmarks[idx].path.clone();
                self.new_bm_label = self.conf.bookmarks[idx].label.clone();
            },
            Message::NewBmPathInput(path) => self.new_bm_path = path,
            Message::NewBmLabelInput(label) => self.new_bm_label = label,
            Message::UpdateBookmark(idx) => {
                let mut changed = false;
                if !self.new_bm_path.is_empty() {
                    changed = true;
                    self.conf.bookmarks[idx].path = mem::take(&mut self.new_bm_path);
                }
                if !self.new_bm_label.is_empty() {
                    changed = true;
                    self.conf.bookmarks[idx].label = mem::take(&mut self.new_bm_label);
                }
                self.modal = FModal::None;
                if changed {
                    self.conf.update(true);
                }
            },
            Message::LoadBookmark(idx) => {
                self.dir_history.push(mem::take(&mut self.dirs));
                self.dirs = vec![self.conf.bookmarks[idx].path.clone()];
                self.update_scroll(0.0);
                return self.update(Message::LoadDir);
            },
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
            Message::Init((fichan, inochan, search_res, more_files)) => {
                let (txino, watch_cmds) = unbounded_channel::<Inochan>();
                let (txsrch, search_cmds) = unbounded_channel::<SearchEvent>();
                let (txrec, recurse_cmds) = unbounded_channel::<RecMsg>();
                tokio::spawn(watch_inotify(watch_cmds, inochan));
                self.search_commander = Some(txsrch.clone());
                self.ino_updater = Some(txino);
                self.thumb_sender = Some(fichan);
                tokio::spawn(recursive_add(recurse_cmds, more_files, txrec.clone(), txsrch,
                                           self.conf.gitignore.clone(), self.conf.respect_gitignore));
                self.recurse_updater = Some(txrec);
                tokio::spawn(search_loop(search_cmds, search_res));
                return self.update(Message::LoadDir);
            },
            Message::PathTxtInput(txt) => self.pathbar = txt,
            Message::SearchTxtInput(txt) => {
                self.searchbar = txt;
                if self.searchbar.is_empty() {
                    self.recurse_state = RecState::Stop;
                    let mut have_sel = false;
                    self.displayed = self.items[..self.end_idx].iter().enumerate().filter_map(|(i,item)| {
                        if self.show_hidden || !item.hidden {
                            have_sel |= item.sel;
                            Some(i)
                        } else {None}
                    }).collect();
                    self.show_goto = have_sel && self.dirs.len() > 1;
                    self.enable_sel_button = self.conf.saving() || self.conf.dir() || have_sel;
                    return self.update(Message::Sort(self.conf.sort_by));
                } else if !self.search_running{
                    self.search_running = true;
                    self.search_commander.as_ref().unwrap().send(SearchEvent::Search(self.searchbar.clone())).unwrap();
                    if self.recurse_state != RecState::Run {
                        self.recurse_state = RecState::Run;
                        self.recurse_updater.as_ref().unwrap().send(RecMsg::FetchMore(self.nav_id, true)).unwrap();
                    }
                }
            },
            Message::SearchResult(res) => {
                let mut still_running = false;
                if let SearchEvent::Results(res, nav_id, num_items, term) = *res {
                    if nav_id == self.nav_id && !self.searchbar.is_empty() {
                        self.displayed = res[..1000.min(res.len())].into_iter().enumerate().map(|(di,ii)|{
                            self.items[ii.0].display_idx = di;
                            ii.0
                        }).collect();
                        let _ = self.update(Message::LoadThumbs);
                        if term != self.searchbar || num_items != self.items.len() {
                            self.search_commander.as_ref().unwrap().send(SearchEvent::Search(self.searchbar.clone())).unwrap();
                            still_running = true;
                        }
                        self.row_sizes.borrow_mut().reset(true);
                    }
                }
                self.search_running = still_running;
            },
            Message::NextRecurse(mut next_items, nav_id) => {
                if nav_id == self.nav_id {
                    let mut new_displayed = vec![];
                    let paths = next_items.iter_mut().enumerate().map(|(i,fitem)| {
                        fitem.items_idx = self.items.len() + i;
                        if self.show_hidden || !fitem.hidden {
                            new_displayed.push(fitem.items_idx);
                        }
                        fitem.path.clone()
                    }).collect();
                    let sender = self.search_commander.as_ref().unwrap();
                    self.items.append(&mut next_items);
                    sender.send(SearchEvent::AddItems(paths)).unwrap();
                    sender.send(SearchEvent::AddView(new_displayed)).unwrap();
                    if !self.search_running {
                        sender.send(SearchEvent::Search(self.searchbar.clone())).unwrap();
                    }
                    if self.recurse_state != RecState::Stop {
                        self.recurse_updater.as_ref().unwrap().send(RecMsg::FetchMore(nav_id, true)).unwrap();
                    }
                }
            },
            Message::Ctrl(pressed) => self.ctrl_pressed = pressed,
            Message::Shift(pressed) => self.shift_pressed = pressed,
            Message::ArrowKey(key) => {
                let didx = if self.items.iter().filter(|item|!item.recursed || !self.searchbar.is_empty()).any(|item|item.sel) {
                    let maxcols = self.pos_state.borrow().max_cols as i64;
                    let i = self.last_clicked.didx as i64;
                    match key {
                        ArrowUp => i - maxcols,
                        ArrowDown => i + maxcols,
                        ArrowLeft => i - 1,
                        ArrowRight => i + 1,
                        _ => -1,
                    }
                } else { 0 };
                match self.view_image.1 {
                    Preview::None => {},
                    _ => {
                        let step = didx - (self.last_clicked.didx as i64);
                        return self.update(Message::NextImage(step));
                    },
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
                        let mut item = mem::replace(&mut self.items[ii], FItem::placeholder(ii, di));
                        item.view_id = self.view_id;
                        tokio::spawn(item.load(
                                    self.thumb_sender.as_ref().unwrap().clone(), self.icons.clone(), self.conf.thumb_size as u32));
                        max_load -= 1;
                    }
                    di += 1;
                }
                self.last_loaded = di;
            },
            Message::NextItem(mut doneitem) => {
                if doneitem.nav_id == self.nav_id {
                    if doneitem.view_id == self.view_id {
                        let mut prev_di = self.last_loaded;
                        while prev_di < self.displayed.len() {
                            let i = self.dtoi(prev_di);
                            if self.items[i].not_loaded() {
                                let mut nextitem = mem::replace(&mut self.items[i], FItem::placeholder(i, prev_di));
                                nextitem.view_id = self.view_id;
                                tokio::spawn(nextitem.load(
                                        self.thumb_sender.as_ref().unwrap().clone(), self.icons.clone(), self.conf.thumb_size as u32));
                                break;
                            }
                            prev_di += 1;
                        }
                        self.last_loaded = prev_di + 1;
                    }
                    let j = doneitem.items_idx;
                    doneitem.display_idx = self.items[j].display_idx;
                    self.items[j] = doneitem;
                }
            },
            Message::Goto => {
                self.dir_history.push(mem::take(&mut self.dirs));
                self.dirs = self.items.iter().filter(|item|item.sel).map(|item|
                    Path::new(&item.path).parent().unwrap().to_string_lossy().to_string()).collect();
                self.update_scroll(0.0);
                return self.update(Message::LoadDir);
            }
            Message::LoadDir => {
                self.view_image = (0, Preview::None);
                self.update_scroll(0.0);
                self.pathbar = match &self.save_filename {
                    Some(fname) => Path::new(&self.dirs[0]).join(fname).to_string_lossy().to_string(),
                    None => self.dirs[0].clone(),
                };
                self.load_dir();
                self.show_goto = false;
                self.recurse_state = RecState::Stop;
                if let Err(e) = self.recurse_updater.as_ref().unwrap().send(RecMsg::NewNav(self.dirs.clone(), self.nav_id)) {
                    eprintln!("Recursive search error: {}", e);
                }
                let _ = self.update(Message::Sort(self.conf.sort_by));
                let mut cmds = vec![scrollable::snap_to(self.scroll_id.clone(), scrollable::RelativeOffset::START)];
                if !self.conf.saving() { cmds.push(text_input::focus(self.search_id.clone())); }
                return Command::batch(cmds);
            },
            Message::DownDir => {
                if let Some(dirs) = self.dir_history.pop() {
                    self.dirs = dirs;
                    self.update_scroll(0.0);
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
            Message::Rename => {
                if self.new_path.basename.is_empty() {
                    return Command::none()
                }
                if let Some(ref mut item) = self.items.iter_mut().find(|i|i.sel) {
                    match OsCmd::new("mv").arg(&item.path).arg(&self.new_path.full_path).output() {
                        Ok(output) if output.status.success() => {
                            (item.label, item.hidden) = make_label(&self.new_path.full_path);
                            item.path = mem::take(&mut self.new_path.full_path);
                        },
                        Err(e) => {
                            let err = format!("Error renaming {} to {}: {}", item.path, self.new_path.basename, e);
                            eprintln!("{}", err);
                            self.modal = FModal::Error(err);
                        },
                        _ => {
                            let err = format!("Error renaming {} to {}", item.path, self.new_path.basename);
                            eprintln!("{}", err);
                            self.modal = FModal::Error(err);
                        },
                    }
                }
                self.new_path.reset();
                match self.modal {
                    FModal::Error(_) => {},
                    _ => self.modal = FModal::None,
                }
            }
            Message::NewDir(confirmed) => if confirmed {
                    let path = Path::new(&self.dirs[0]).join(&self.new_path.basename);
                    if let Err(e) = std::fs::create_dir_all(&path) {
                        let msg = format!("Error creating directory: {:?}", e);
                        self.modal = FModal::Error(msg);
                    } else {
                        self.modal = FModal::None;
                    }
                } else {
                    self.new_path.reset();
                    self.modal = FModal::NewDir;
                },
            Message::NewPathInput(path) => self.new_path.update(path),
            Message::CloseModal => self.modal = FModal::None,
            Message::MiddleClick(iidx) => self.click_item(iidx, false, true, false),
            Message::LeftPreClick(iidx) => self.clicktimer.preclick(iidx),
            Message::LeftClick(iidx, always_valid) => {
                match self.clicktimer.click(iidx, always_valid) {
                    ClickType::Single => self.click_item(iidx, self.shift_pressed, self.ctrl_pressed, iidx == self.view_image.0),
                    ClickType::Double => {
                        self.items[iidx].sel = true;
                        return self.update(Message::Select(SelType::Click));
                    },
                    ClickType::Pass => {},
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
                        self.click_item(iidx, true, false, false);
                    }
                } else {
                    self.view_image = (0, Preview::None);
                    return scrollable::scroll_to(self.scroll_id.clone(), self.scroll_offset);
                }
            },
            Message::NextImage(step) => {
                match self.view_image.1 {
                    Preview::None => {},
                    _ => {
                        let mut didx = self.itod(self.view_image.0) as i64;
                        while (step<0 && didx>0) || (step>0 && didx<((self.displayed.len()-1) as i64)) {
                            didx = (didx as i64) + step;
                            if didx<0 || didx as usize>=self.displayed.len() {
                                return Command::none();
                            }
                            let di = didx as usize;
                            let ii = self.dtoi(di);
                            if self.items[ii].ftype == FType::Image {
                                match self.items[ii].preview() {
                                    Preview::None => {},
                                    pv => {
                                        self.view_image = (self.dtoi(di), pv);
                                        return self.update(Message::LeftClick(self.view_image.0, true));
                                    },
                                }
                            }
                        }
                    },
                }
            },
            Message::OverWriteOK => {
                println!("{}", self.pathbar);
                self.exit();
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
                            self.update_scroll(0.0);
                            return self.update(Message::LoadDir);
                        } else {
                            println!("{}", self.pathbar);
                            self.exit();
                        }
                    }
                } else if self.conf.dir() {
                    let sel = Path::new(match self.items.iter().find(|item|item.sel) {
                        Some(item) => &item.path,
                        None => &self.pathbar,
                    });
                    if sel.is_dir() {
                        if seltype == SelType::Click {
                            self.dirs = vec![self.items.iter().find(|it|it.sel).unwrap().path.clone()];
                            return self.update(Message::LoadDir);
                        } else {
                            println!("{}", sel.to_string_lossy());
                            self.exit();
                        }
                    } else if sel.is_file() {
                        if let Some(p) = sel.parent() {
                            println!("{}", p.to_string_lossy());
                            self.exit();
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
                                    self.exit();
                                } else {
                                    self.dirs = sels.iter().filter_map(|item| match item.ftype {
                                        FType::Dir => Some(item.path.clone()), _ => None}).collect();
                                    return self.update(Message::LoadDir);
                                }
                            },
                            FType::NotExist => {},
                            _ => {
                                println!("{}", sels.iter().map(|item|item.path.as_str()).join("\n"));
                                self.exit();
                            }
                        }
                    }
                }
            },
            Message::Cancel => self.exit(),
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
                        let (rec_sender, rec_receiver) = unbounded_channel::<RecMsg>();
                        messager.send(Message::Init((fi_sender, ino_sender, search_sender, rec_sender))).await.unwrap();
                        state = SubState::Ready((fi_reciever,ino_receiver,search_receiver, rec_receiver));
                    }
                    SubState::Ready((thumb_recv, ino_recv, search_recv, rec_recv)) => {
                        tokio::select! {
                            more = rec_recv.recv() => {
                                if let Some(RecMsg::NextItems(items, nav_id)) = more {
                                    messager.send(Message::NextRecurse(items, nav_id)).await.unwrap();
                                }
                            },
                            res = search_recv.recv() => {
                                if let Some(search_event) = res {
                                    messager.send(Message::SearchResult(Box::new(search_event))).await.unwrap();
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
                    Keyboard(KeyPressed{ key: Key::Named(PageUp), .. }) => Some(Message::PageUp),
                    Keyboard(KeyPressed{ key: Key::Named(PageDown), .. }) => Some(Message::PageDown),
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
                |(i,cmd)|{
                    if self.clipboard_paths.is_empty() && cmd.label == "Paste" {
                        Item::new(Button::new(container(text(cmd.label.as_str()).width(Length::Fill)
                                    .horizontal_alignment(alignment::Horizontal::Center)))
                            .padding(1.0)
                            .style(style::top_but_theme()))
                    } else {
                        Item::new(menu_button(cmd.label.as_str(), Message::RunCmd(i)))
                    }
                }).collect();
            let bookmarks = self.conf.bookmarks.iter().enumerate().fold(column![], |col,(i,bm)| {
                        let bm_button = container(Button::new(
                                    container(
                                        Text::new(bm.label.as_str())
                                           .size(15.0)
                                           .horizontal_alignment(alignment::Horizontal::Center)
                                           .width(Length::Fill))
                                    .padding(-3.0)
                                    .id(bm.id.clone()))
                                 .style(style::side_but_theme())
                                 .on_press(Message::LoadBookmark(i)));
                        let ctx_menu = ContextMenu::new(bm_button, move || {
                            column![
                                Button::new(Text::new("Delete"))
                                    .on_press(Message::DeleteBookmark(i))
                                    .width(Length::Fill)
                                    .style(style::top_but_theme()),
                                Button::new(Text::new("Edit"))
                                    .on_press(Message::EditBookmark(i))
                                    .width(Length::Fill)
                                    .style(style::top_but_theme()),
                            ].width(Length::Fixed(100.0)).into()
                        });
                        col.push(ctx_menu)
                    }).push(container(vertical_space()).height(Length::Fill).width(Length::Fill)
                            .id(CId::new("bookmarks"))).width(Length::Fixed(120.0));

            let mut clicked_offscreen = false;
            let mut ps = self.pos_state.borrow_mut();
            ps.max_cols = if self.conf.icon_view {
                ((size.width-140.0) / self.conf.thumb_size).max(1.0) as usize
            } else { 1 };
            let content: iced::Element<'_, Self::Message> = match &self.view_image.1 {
                Preview::Svg(handle) => {
                    mouse_area(container(svg(handle.clone())
                                        .width(Length::Fill)
                                        .height(Length::Fill))
                                   .align_x(alignment::Horizontal::Center)
                                   .align_y(alignment::Vertical::Center)
                                   .width(Length::Fill).height(Length::Fill))
                        .on_right_press(Message::RightClick(-1))
                        .on_release(Message::LeftClick(self.view_image.0, true))
                        .into()
                },
                Preview::Image(handle) => {
                    mouse_area(container(image(handle.clone())
                                        .width(Length::Fill)
                                        .height(Length::Fill))
                                   .align_x(alignment::Horizontal::Center)
                                   .align_y(alignment::Vertical::Center)
                                   .width(Length::Fill).height(Length::Fill))
                        .on_right_press(Message::RightClick(-1))
                        .on_release(Message::LeftClick(self.view_image.0, true))
                        .into()
                },
                Preview::Gif(frames) => {
                    mouse_area(container(gif(&frames)
                                        .width(Length::Fill)
                                        .height(Length::Fill))
                                   .align_x(alignment::Horizontal::Center)
                                   .align_y(alignment::Vertical::Center)
                                   .width(Length::Fill).height(Length::Fill))
                        .on_right_press(Message::RightClick(-1))
                        .on_release(Message::LeftClick(self.view_image.0, true))
                        .into()
                },
                Preview::None => {
                    if self.conf.icon_view {
                        let thumb_width = (size.width-140.0) / ps.max_cols as f32;
                        let num_rows = self.num_rows(ps.max_cols);
                        let top = self.scroll_offset.y - self.conf.thumb_size*1.1;
                        let bot = self.scroll_offset.y + self.content_height;
                        let mut rs = self.row_sizes.borrow_mut();
                        let mut rows = Column::new();
                        rs.checkcols(ps.max_cols);
                        if rs.rows.len() < num_rows {
                            rs.rows.resize_with(num_rows, Rowsize::default);
                        }
                        let first_idx = match rs.rows.iter().take_while(|r|r.ready).find_position(|r|r.pos > top) {
                            Some((i,r)) => {
                                rows = rows.push(vertical_space().height(r.pos));
                                i
                            },
                            None => 0,
                        };
                        let mut next_ready = true;
                        let mut send_max = 20;
                        let mut clicked_onscreen = false;
                        for i in first_idx..num_rows {
                            let cur_row = &rs.rows[i];
                            let past_bot = cur_row.pos > bot;
                            if num_rows <= rs.num_ready {
                                if past_bot {
                                    let last_pos = rs.rows.last().unwrap().end_pos;
                                    rows = rows.push(vertical_space().height(last_pos - cur_row.pos));
                                    break;
                                }
                            }
                            let mut row_all_ready = next_ready;
                            let mut row_none_ready = true;
                            if past_bot {
                                rows = rows.push(vertical_space().height(cur_row.end_pos - cur_row.pos));
                            } else {
                                let start = i * ps.max_cols;
                                let mut row = Row::new().width(Length::Fill);
                                for j in 0..ps.max_cols {
                                    let idx = start + j;
                                    if idx < self.displayed.len() {
                                        let item = &self.items[self.dtoi(idx)];
                                        row_all_ready &= item.thumb_handle != None;
                                        row_none_ready &= item.thumb_handle == None;
                                        let (clicked, display) = item.display_thumb(&self.last_clicked, thumb_width);
                                        clicked_onscreen |= clicked;
                                        row = row.push(display);
                                    }
                                }
                                if row_all_ready && i == rs.next_send && send_max > 0 {
                                    send_max -= 1;
                                    let counter = rs.view_counter;
                                    rows = rows.push(wrapper::locator(row).send_info(move|a,b|Message::PositionInfo(Pos::Row(counter, i),a,b), true));
                                    rs.next_send += 1;
                                } else {
                                    rows = rows.push(row);
                                }
                                if row_none_ready {
                                    break;
                                }
                            }

                            next_ready = row_all_ready;
                        }
                        clicked_offscreen = self.last_clicked.new && !clicked_onscreen;
                        Scrollable::new(rows)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .on_scroll(Message::Scrolled)
                            .direction(Direction::Vertical(Properties::new()))
                            .id(self.scroll_id.clone()).into()
                    } else {
                        // list view
                        let mut rows = Column::new();
                        let mut clicked_onscreen = false;
                        let num_total = self.displayed.len();
                        let top = self.scroll_offset.y - ROW_HEIGHT*1.1;
                        let bot = self.scroll_offset.y + self.content_height;
                        let first_idx = (top / ROW_HEIGHT).floor().max(0.0) as usize;
                        let last_idx = (bot / ROW_HEIGHT).ceil().min((num_total.max(1) - 1) as f32) as usize;
                        if first_idx > 0 {
                            rows = rows.push(vertical_space().height(ROW_HEIGHT * first_idx as f32));
                        }
                        if num_total > 0 {
                            for i in first_idx..last_idx+1 {
                                let item = &self.items[self.dtoi(i)];
                                let (clicked, displayed) = item.display_row(&self.last_clicked);
                                clicked_onscreen |= clicked;
                                rows = rows.push(displayed);
                            }
                        }
                        if last_idx+1 < num_total {
                            rows = rows.push(vertical_space().height(ROW_HEIGHT * (num_total-1-last_idx) as f32));
                        }
                        clicked_offscreen = self.last_clicked.new && !clicked_onscreen;
                        Scrollable::new(rows)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .on_scroll(Message::Scrolled)
                            .direction(Direction::Vertical(Properties::new()))
                            .id(self.scroll_id.clone()).into()
                    }
                },
            };
            let count = Text::new(format!("  {} items", self.displayed.len()));
            let ctrlbar = column![
                row![
                    match (&self.last_clicked.size, self.show_goto) {
                        (Some(size), true) => row![Text::new(size),count, horizontal_space(), top_icon(self.icons.goto.clone(), Message::Goto)],
                        (Some(size), false) => row![Text::new(size),count, horizontal_space()],
                        (None, true) => row![count, horizontal_space(), top_button("Goto Dir", 80.0, Message::Goto)],
                        (None, false) => row![count, horizontal_space()]
                    },
                    menu_bar![
                        (top_icon(self.icons.cmds.clone(), Message::Dummy), 
                            view_menu(cmd_list))
                        (top_icon(self.icons.settings.clone(), Message::Dummy),
                            view_menu(menu_items!(
                                    (menu_button(if self.conf.icon_view { "List View" } else { "Icon View" },Message::ChangeView))
                                    (menu_button("Sort A-Z",Message::Sort(1)))
                                    (menu_button("Sort Z-A",Message::Sort(2)))
                                    (menu_button("Sort Newest first",Message::Sort(3)))
                                    (menu_button("Sort Oldest first",Message::Sort(4)))
                                    (checkbox("Show Hidden", self.show_hidden).on_toggle(Message::ShowHidden))
                                    (checkbox("Recursive Search", self.recursive_search).on_toggle(Message::SetRecursive))
                                    (text(format!("Thumbnail size:{}", self.conf.thumb_size)))
                                    (slider(50.0..=500.0, self.conf.thumb_size, Message::Thumbsize))
                                    )))
                    ].spacing(1.0),
                    top_icon(self.icons.newdir.clone(), Message::NewDir(false)),
                    top_icon(self.icons.updir.clone(), Message::UpDir),
                    top_button("Cancel", 100.0, Message::Cancel),
                    if self.enable_sel_button {
                        top_button(&self.select_button, 100.0, Message::Select(SelType::Button))
                    } else {
                        top_button_off(&self.select_button, 100.0)
                    }
                ].spacing(1).height(31.0),
                row![
                TextInput::new("directory or file path", self.pathbar.as_str())
                    .on_input(Message::PathTxtInput)
                    .on_paste(Message::PathTxtInput)
                    .on_submit(Message::Select(SelType::TxtEntr))
                    .width(Length::FillPortion(8))
                    .padding(2.0),
                TextInput::new("search", self.searchbar.as_str())
                    .on_input(Message::SearchTxtInput)
                    .on_paste(Message::SearchTxtInput)
                    .width(Length::FillPortion(2))
                    .padding(2.0)
                    .id(self.search_id.clone()),
                Button::new("X").on_press(Message::SearchTxtInput("".to_string())).style(style::flat_but_theme())
                    .padding(Padding::from([2.0, 5.0]))
                ]
            ].align_items(iced::Alignment::End).width(Length::Fill);
            let send = clicked_offscreen || ps.total_width != size.width || ps.total_height != size.height;
            let mainview = column![
                ctrlbar,
                row![bookmarks, wrapper::locator(content).send_info(move|a,b|Message::PositionInfo(
                        Pos::Content(clicked_offscreen),a,b), send)],
            ];
            ps.total_width = size.width;
            ps.total_height = size.height;
            match self.modal {
                FModal::None => mainview.into(),
                FModal::EditBookmark(i) => modal(mainview, Some(Card::new(
                        Text::new("Edit bookmark"),
                        column![
                            Text::new("Label:"),
                            TextInput::new(&self.conf.bookmarks[i].label, self.new_bm_label.as_str())
                                .on_input(Message::NewBmLabelInput)
                                .on_submit(Message::UpdateBookmark(i))
                                .on_paste(Message::NewBmLabelInput),
                            Text::new("Directory path:"),
                            TextInput::new(&self.conf.bookmarks[i].path, self.new_bm_path.as_str())
                                .on_input(Message::NewBmPathInput)
                                .on_submit(Message::UpdateBookmark(i))
                                .on_paste(Message::NewBmPathInput),
                            row![
                                Button::new("Update").on_press(Message::UpdateBookmark(i)).style(style::top_but_theme()),
                                Button::new("Delete").on_press(Message::DeleteBookmark(i)).style(style::top_but_theme()),
                                Button::new("Cancel").on_press(Message::CloseModal).style(style::top_but_theme()),
                            ].spacing(5.0)
                        ]
                        ).max_width(500.0)
                        .on_close(Message::CloseModal))
                    )
                    .backdrop(Message::CloseModal)
                    .on_esc(Message::CloseModal)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                FModal::Rename(ref filename) => modal(mainview, Some(Card::new(
                        Text::new("Rename File"),
                        column![
                            TextInput::new(filename, &self.new_path.basename)
                                .on_input(Message::NewPathInput)
                                .on_submit(Message::Rename)
                                .on_paste(Message::NewPathInput),
                            row![
                                Button::new("Rename").on_press(Message::Rename).style(style::top_but_theme()),
                                Button::new("Cancel").on_press(Message::CloseModal).style(style::top_but_theme()),
                            ].spacing(5.0)
                        ]
                        ).max_width(500.0)
                        .on_close(Message::CloseModal))
                    )
                    .backdrop(Message::CloseModal)
                    .on_esc(Message::CloseModal)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                FModal::Error(ref msg) => modal(mainview, Some(Card::new(
                            Text::new("Error"), text(msg)).max_width(500.0))
                    )
                    .backdrop(Message::CloseModal)
                    .on_esc(Message::CloseModal)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                FModal::OverWrite => modal(mainview, Some(Card::new(
                        Text::new("File exists. Overwrite?"),
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
                        Text::new("Enter new directory name"),
                        column![
                            TextInput::new("Untitled", self.new_path.basename.as_str())
                                .on_input(Message::NewPathInput)
                                .on_submit(Message::NewDir(true))
                                .on_paste(Message::NewPathInput),
                            row![
                                Button::new("Create").on_press(Message::NewDir(true)).style(style::top_but_theme()),
                                Button::new("Cancel").on_press(Message::CloseModal).style(style::top_but_theme()),
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

impl NewPath {
    fn reset(&mut self) {
        self.full_path.clear();
        self.basename.clear();
    }

    fn update(&mut self, input: String) {
        self.basename = input;
        if !self.full_path.is_empty() {
            self.full_path = Path::new(&self.full_path).parent().unwrap()
                .join(&self.basename).to_string_lossy().to_string();
        }
    }
}

fn menu_button(txt: &str, msg: Message) -> Element<'static, Message> {
    Button::new(container(text(txt)
                .width(Length::Fill)
                .horizontal_alignment(alignment::Horizontal::Center)))
        .style(style::top_but_theme())
        .padding(1.0)
        .on_press(msg).into()
}
fn top_button(txt: &str, size: f32, msg: Message) -> Element<'static, Message> {
    Button::new(text(txt)
                .width(size)
                .horizontal_alignment(alignment::Horizontal::Center))
        .style(style::top_but_theme())
        .on_press(msg).into()
}
fn top_button_off(txt: &str, size: f32) -> Element<'static, Message> {
    Button::new(text(txt)
                .width(size)
                .horizontal_alignment(alignment::Horizontal::Center))
        .style(style::top_but_theme()).into()
}
fn top_icon(img: svg::Handle, msg: Message) -> Element<'static, Message> {
    Button::new(svg(img)
                .width(40.0))
        .style(style::top_but_theme())
        .on_press(msg).into()
}

fn make_label(path: &str) -> (String, bool) {
    let mut label = path.rsplitn(2,'/').next().unwrap().to_string();
    let hidden = label.starts_with('.');
    let len = label.len();
    if len > 20 {
        let mut line_len = 0;
        let mut split = 0;
        for (i, w) in label.split_word_bound_indices().rev() {
            if line_len + w.len() > 20 {
                if line_len == 0 {
                    split = len - 20.min(len);
                    while  split > 0 && (label.as_bytes()[split] & 0b11000000) == 0b10000000 {
                        split -= 1;
                    }
                }
                break;
            }
            split = i;
            line_len += w.len();
        }
        let len = split;
        line_len = 0;
        let mut start = 0;
        for (i, w) in label[..split].split_word_bound_indices().rev() {
            if line_len + w.len() > 20 {
                if line_len == 0 {
                    start = len - 20.min(len);
                    while  start > 0 && (label.as_bytes()[start] & 0b11000000) == 0b10000000 {
                        start -= 1;
                    }
                }
                break;
            }
            start = i;
            line_len += w.len();
        }
        if start == split {
            label = label[start..].to_string();
        } else {
            label = format!("{}{}\n{}", if start == 0 { "" } else { "..." },
                &label[start..split],
                &label[split..]);
        }
    }
    (label, hidden)
}

impl FItem {

    #[inline]
    fn isdir(self: &Self) -> bool { self.ftype == FType::Dir }

    #[inline]
    fn not_loaded(self: &Self) -> bool { self.thumb_handle == None && !self.path.is_empty() }

    fn display_row(&self, last_clicked: &LastClicked) -> (bool, Element<'static, Message>) {
        let mut row = Row::new();
        let idx = self.items_idx;
        row = row.push(text(self.path.rsplitn(2,'/').next().unwrap()).width(Length::FillPortion(70)));
        if !self.isdir() {
            let bytes = self.size as f64;
            let sz = if bytes > 1073741824.0 { format!("{:.2} GB", bytes/1073741824.0 ) }
            else if bytes > 1048576.0 { format!("{:.1} MB", bytes/1048576.0 ) }
            else if bytes > 1024.0 { format!("{:.0} KB", bytes/1024.0 ) }
            else { format!("{:.0} B", bytes) };
            row = row.push(container(Text::new(sz)).padding(Padding{
                right: 15.0, left: 5.0, top: 0.0, bottom: 0.0
            }));
        }
        let systime = std::time::UNIX_EPOCH + Duration::from_secs(self.mtime);
        let datetime: DateTime<Utc> = systime.into();
        let iso_8601_string = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
        row = row.push(container(Text::new(iso_8601_string)).padding(Padding{
            right: 15.0, left: 0.0, top: 0.0, bottom: 0.0
        }));
        let clickable = match (self.isdir(), self.sel) {
            (true, true) => {
                let dr = iced_drop::droppable(row).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).height(ROW_HEIGHT).width(Length::Fill).style(get_sel_theme()))
            },
            (true, false) => {
                let dr = iced_drop::droppable(row).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).height(ROW_HEIGHT).width(Length::Fill))
            },
            (false, true) => {
                mouse_area(container(row).height(ROW_HEIGHT).width(Length::Fill).style(get_sel_theme()))
            },
            (false, false) => {
                mouse_area(container(row).height(ROW_HEIGHT).width(Length::Fill))
            },
        }.on_release(Message::LeftClick(self.items_idx, false))
            .on_press(Message::LeftPreClick(self.items_idx))
            .on_right_press(Message::RightClick(self.items_idx as i64))
            .on_middle_press(Message::MiddleClick(self.items_idx));
        match (last_clicked.iidx, last_clicked.new) {
            (i, true) if i == idx => {
                (true, wrapper::locator(clickable).send_info(move|a,b|Message::PositionInfo(Pos::Item,a,b), true).into())
            },
            (_,_) => {
                (false, clickable.into())
            },
        }
    }

    fn display_thumb(&self, last_clicked: &LastClicked, thumbsize: f32) -> (bool, Element<'static, Message>) {
        let mut col = Column::new()
            .align_items(iced::Alignment::Center)
            .width(Length::Fixed(thumbsize));
        match &self.thumb_handle {
            Some(h) => col = col.push(image(h.clone())),
            _ => {},
        }
        let shape = if self.unicode { text::Shaping::Advanced } else { text::Shaping::Basic };
        col = col.push(text(self.label.as_str()).size(13).shaping(shape));
        let idx = self.items_idx;
        let clickable = match (self.isdir(), self.sel) {
            (true, true) => {
                let dr = iced_drop::droppable(col).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).style(get_sel_theme()).padding(1.0))
            },
            (true, false) => {
                let dr = iced_drop::droppable(col).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).padding(1.0))
            },
            (false, true) => {
                mouse_area(container(col).style(get_sel_theme()).padding(1.0))
            },
            (false, false) => {
                mouse_area(container(col).padding(1.0))
            },
        }.on_release(Message::LeftClick(self.items_idx, false))
            .on_press(Message::LeftPreClick(self.items_idx))
            .on_right_press(Message::RightClick(self.items_idx as i64))
            .on_middle_press(Message::MiddleClick(self.items_idx));
        match (last_clicked.iidx, last_clicked.new) {
            (i, true) if i == idx => {
                (true, wrapper::locator(clickable).send_info(move|a,b|Message::PositionInfo(Pos::Item,a,b), true).into())
            },
            (_,_) => {
                (false, clickable.into())
            },
        }
    }

    fn preview(self: &Self) -> Preview {
        if self.svg {
            Preview::Svg(svg::Handle::from_path(&self.path))
        } else if self.vid {
            match vid_frame(self.path.as_str(), None, None) {
                None => Preview::None,
                Some(a) => Preview::Image(a),
            }
        } else if self.ftype == FType::Image {
            match std::fs::read(self.path.as_str()) {
                Ok(data) => {
                    if self.gif {
                        match iced_gif::Frames::from_bytes(data) {
                            Ok(f) => return Preview::Gif(f),
                            Err(_) => return Preview::None,
                        };
                    } else {
                        match load_from_memory(data.as_ref()) {
                            Ok(img) => {
                                let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
                                Preview::Image(Handle::from_pixels(w, h, rgba.as_raw().clone()))
                            },
                            Err(e) => {
                                eprintln!("Error decoding image {}:{}", self.path, e);
                                Preview::None
                            },
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading image {}:{}", self.path, e);
                    Preview::None
                },
            }
        } else {
            Preview::None
        }
    }

    fn placeholder(ii: usize, di: usize) -> Self {
        let mut ret = FItem(Box::new(Default::default()));
        ret.items_idx = ii;
        ret.display_idx = di;
        ret
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
        let (label, hidden) = make_label(&path);
        let unicode = label.bytes().any(|c| c & 0b10000000 != 0);
        FItem(Box::new(FItemb {
            path: path.to_string(),
            label,
            ftype,
            items_idx: 0,
            display_idx: 0,
            thumb_handle: None,
            sel: false,
            nav_id,
            view_id: 0,
            mtime: mtime.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            vid: false,
            gif: false,
            svg: false,
            size,
            hidden,
            recursed: false,
            unicode,
        }))
    }

    async fn prepare_cached_thumbnail(
            self: &Self,
            path: &str,
            imgtype: ImgType,
            thumbsize: u32,
            icons: Arc<Icons>) -> Option<Handle> {
        let mut hasher = Md5::new();
        let p = Path::new(path);
        let fmetadata = p.metadata().unwrap();
        let fmtime = fmetadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let fsize = fmetadata.len();
        let fdir = p.parent().unwrap_or(Path::new(""));
        hasher.update(path.as_bytes());
        hasher.update(fmtime.to_le_bytes());
        hasher.update(fsize.to_le_bytes());
        let cache_path = Path::new(&icons.thumb_dir).join(if imgtype == ImgType::Pdf || imgtype == ImgType::Epub {
            format!("{:x}{}.jpg", hasher.finalize(), thumbsize)
        } else {
            format!("{:x}{}.webp", hasher.finalize(), thumbsize)
        });
        let cmtime = match cache_path.metadata() {
            Ok(md) => md.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            Err(_) => 0,
        };
        if cache_path.is_file() && cmtime >= fmtime {
            let mut file = File::open(cache_path).await.unwrap();
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).await.unwrap_or(0);
            let img = load_from_memory(buffer.as_ref()).unwrap();
            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
            return Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
        }
        if (imgtype == ImgType::Pdf && !icons.cando_pdf) || (imgtype == ImgType::Epub && !icons.cando_epub) {
            return Some(icons.doc.clone());
        }
        if fdir.to_string_lossy() == icons.thumb_dir {
            let mut buffer = Vec::new();
            let mut file = File::open(self.path.as_str()).await.unwrap();
            file.read_to_end(&mut buffer).await.unwrap_or(0);
            let img = load_from_memory(buffer.as_ref()).unwrap();
            let thumb = img.thumbnail(thumbsize, thumbsize);
            let (w,h,rgba) = (thumb.width(), thumb.height(), thumb.into_rgba8());
            Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
        } else if imgtype == ImgType::Vid {
            vid_frame(&path, Some(thumbsize), Some(&cache_path))
        } else if imgtype == ImgType::Epub {
            match OsCmd::new("epub-thumbnailer")
                .arg(self.path.as_str()).arg(&cache_path).arg(format!("{}",thumbsize))
                .output() {
                Ok(_) => {
                    match File::open(&cache_path).await {
                        Ok(mut file) => {
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await.unwrap_or(0);
                            let img = load_from_memory(buffer.as_ref()).unwrap();
                            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
                            Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
                        },
                        Err(_) => {
                            None
                        },
                    }
                },
                Err(_) => {
                    None
                },
            }
        } else if imgtype == ImgType::Pdf {
            match OsCmd::new("pdftoppm")
                .arg("-jpeg").arg("-f").arg("1").arg("-singlefile").arg("-scale-to").arg(format!("{}",thumbsize))
                .arg(self.path.as_str()).arg(cache_path.to_string_lossy().trim_end_matches(".jpg"))
                .output() {
                Ok(_) => {
                    match File::open(&cache_path).await {
                        Ok(mut file) => {
                            let mut buffer = Vec::new();
                            file.read_to_end(&mut buffer).await.unwrap_or(0);
                            let img = load_from_memory(buffer.as_ref()).unwrap();
                            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
                            Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
                        },
                        Err(_) => {
                            None
                        },
                    }
                },
                Err(_) => {
                    None
                },
            }
        } else {
            let file = File::open(self.path.as_str()).await;
            match file {
                Ok(mut file) => {
                    let mut buffer = Vec::new();
                    file.read_to_end(&mut buffer).await.unwrap_or(0);
                    if imgtype == ImgType::Svg {
                        let opts = resvg::usvg::Options::default();
                        match resvg::usvg::Tree::from_data(buffer.as_ref(), &opts) {
                            Ok(tree) => {
                                let (w, h) = (tree.size().width(), tree.size().height());
                                let scale = thumbsize as f32 / w.max(h);
                                let w = (w * scale) as u32;
                                let h = (h * scale) as u32;
                                let numpix = w * h * 4;
                                let transforem = tiny_skia::Transform::from_scale(scale, scale);
                                let mut pixels = vec![0; numpix as usize];
                                let mut pixmap = tiny_skia::PixmapMut::from_bytes(&mut pixels, w, h).unwrap();
                                resvg::render(&tree, transforem, &mut pixmap);
                                let encoder = webp::Encoder::from_rgba(pixels.as_ref(), w, h);
                                let wp = encoder.encode_simple(false, 50.0).unwrap();
                                std::fs::write(cache_path, &*wp).unwrap();
                                Some(Handle::from_pixels(w, h, pixels))
                            },
                            Err(e) => {
                                eprintln!("Error decoding svg {}: {}", self.path, e);
                                None
                            },
                        }
                    } else {
                        let img = load_from_memory(buffer.as_ref());
                        match img {
                            Ok(img) => {
                                let thumb = img.thumbnail(thumbsize, thumbsize);
                                let (w,h,rgba) = (thumb.width(), thumb.height(), thumb.into_rgba8());
                                let encoder = webp::Encoder::from_rgba(rgba.as_ref(), w, h);
                                let wp = encoder.encode_simple(false, 50.0).unwrap();
                                std::fs::write(cache_path, &*wp).unwrap();
                                Some(Handle::from_pixels(w, h, rgba.as_raw().clone()))
                            },
                            Err(e) => {
                                eprintln!("Error decoding image {}: {}", self.path, e);
                                None
                            },
                        }
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
        if self.thumb_handle == None {
            match self.ftype {
                FType::Dir => {
                    self.thumb_handle = Some(icons.folder.clone());
                },
                _ => {
                    let ext = match self.path.rsplitn(2,'.').next() {
                        Some(s) => s.to_lowercase(),
                        None => "".to_string(),
                    };
                    let ext = ext.as_str();
                    self.ftype = match ext {
                        "svg" => {
                            self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Svg, thumbsize, icons.clone()).await;
                            if self.thumb_handle == None {
                                self.thumb_handle = Some(icons.error.clone());
                                FType::File
                            } else {
                                self.svg = true;
                                FType::Image
                            }
                        },
                        "png"|"jpg"|"jpeg"|"bmp"|"tiff"|"gif"|"webp" => {
                            self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Norm, thumbsize, icons.clone()).await;
                            if self.thumb_handle == None {
                                self.thumb_handle = Some(icons.error.clone());
                                FType::File
                            } else {
                                if ext == "gif" {
                                    self.gif = true;
                                }
                                FType::Image
                            }
                        },
                        "webm"|"mkv"|"mp4"|"m4b"|"av1"|"avi"|"avif"|"flv"|"wmv"|"m4v"|"mpeg"|"mov"|"jxl" => {
                            self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Vid, thumbsize, icons.clone()).await;
                            if self.thumb_handle == None {
                                self.thumb_handle = Some(icons.error.clone());
                                FType::File
                            } else {
                                self.vid = true;
                                FType::Image
                            }
                        },
                        "pdf" => {
                            self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Pdf, thumbsize, icons.clone()).await;
                            if self.thumb_handle == None {
                                self.thumb_handle = Some(icons.doc.clone());
                                FType::File
                            } else {
                                self.vid = true;
                                FType::Image
                            }
                        },
                        "epub" => {
                            self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Epub, thumbsize, icons.clone()).await;
                            if self.thumb_handle == None {
                                self.thumb_handle = Some(icons.doc.clone());
                                FType::File
                            } else {
                                self.vid = true;
                                FType::Image
                            }
                        },
                        "txt"|"doc"|"docx"|"xls"|"xlsx" => {
                            self.thumb_handle = Some(icons.doc.clone());
                            FType::File
                        },
                        "mp3"|"wav"|"ogg"|"flac"|"aac"|"wma"|"aiff"|"alac"|"opus"|"m4a" => {
                            self.thumb_handle = Some(icons.audio.clone());
                            FType::File
                        },
                        _ => {
                            self.thumb_handle = Some(icons.unknown.clone());
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
    text: Option<&'a str>,
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
    let mut last_moved: u32 = 0;
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
                        eprintln!("{:?}", ev);
                        let mut moved_file = false;
                        if ev.cookie != last_moved {
                            moved_file = ev.mask.contains(EventMask::MOVED_TO);
                        }
                        if ev.mask.contains(EventMask::MOVED_FROM) {
                            last_moved = ev.cookie;
                        }
                        let create_file = ev.mask == EventMask::CREATE;
                        let create_dir = ev.mask == EventMask::CREATE|EventMask::ISDIR;
                        let write_file = ev.mask.contains(EventMask::CLOSE_WRITE);
                        let deleted = ev.mask.contains(EventMask::DELETE) || ev.mask.contains(EventMask::MOVED_FROM);
                        eprintln!("delete:{}", deleted);
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
                                        eprintln!("deleted");
                                        tx.send(Inochan::Delete(path)).unwrap();
                                    } else if moved_file {
                                        tx.send(Inochan::Create(path)).unwrap();
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
                                                                 WatchMask::MOVED_TO|
                                                                 WatchMask::MOVED_FROM|
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

    #[inline]
    fn update_scroll(self: &mut Self, y: f32) {
        self.scroll_offset.y = y;
        self.content_viewport.y = y + self.content_y;
    }

    #[inline]
    fn itopos(self: &Self, i: usize) -> Option<Rectangle> {
        if !self.conf.icon_view {
            let di = self.itod(i);
            let mut rect = Rectangle::default();
            rect.y = di as f32 * ROW_HEIGHT + self.content_y;
            rect.height = ROW_HEIGHT;
            return Some(rect)
        }
        let rs = self.row_sizes.borrow();
        let ri = self.itod(i) / self.pos_state.borrow().max_cols;
        if rs.rows.len() > ri {
            let r = &rs.rows[ri];
            if r.ready {
                let mut rect = Rectangle::default();
                rect.y = r.pos + self.content_y;
                rect.height = r.end_pos - r.pos;
                return Some(rect)
            }
        }
        None
    }

    #[inline]
    fn num_rows(self: &Self, maxcols: usize) -> usize {
        self.displayed.len() / maxcols + if self.displayed.len() % maxcols != 0 { 1 } else { 0 }
    }

    fn run_command(self: &mut Self, icmd: usize) {
        let cmd = &self.conf.cmds[icmd];
        if cmd.builtin && cmd.label == "Paste" {
            if self.dirs.len() != 1 {
                self.modal = FModal::Error("Cannot paste when multiple directories are open".into());
                return;
            }
            self.clipboard_paths.iter_mut().for_each(|path| {
                tokio::spawn(paste(mem::take(path), self.dirs[0].clone(), self.clipboard_cut));
            });
            self.clipboard_paths.clear();
            return;
        }
        self.items.iter().filter(|item| item.sel).for_each(|item| {
            if cmd.builtin {
                match cmd.label.as_str() {
                    "Delete" => if let Err(e) = OsCmd::new("rm").arg("-rf").arg(&item.path).output() {
                        eprintln!("Error deleting {}: {}", item.path, e);
                    },
                    "Rename" => {
                        if self.modal == FModal::None {
                            self.new_path.basename = item.path.rsplitn(2, '/').next().unwrap().to_string();
                            self.new_path.full_path = item.path.clone();
                            self.modal = FModal::Rename(item.path.clone());
                        } else {
                            self.modal = FModal::Error("Select only one file to rename".into());
                        }
                    },
                    "Cut" => {
                        self.clipboard_paths.push(item.path.clone());
                        self.clipboard_cut = true;
                    },
                    "Copy" => {
                        self.clipboard_paths.push(item.path.clone());
                        self.clipboard_cut = false;
                    },
                    &_ => {},
                }
            } else {
                let path = Path::new(item.path.as_str());
                let fname = path.file_name().unwrap().to_string_lossy();
                let part = match fname.splitn(2, '.').next() {
                    Some(s) => s,
                    None => &fname,
                };
                let quoted_fname = shquote(fname.as_ref());
                let quoted_part = shquote(part.as_ref());
                let dir = path.parent().unwrap();
                let filecmd = cmd.cmd.replace("[path]", shquote(&item.path).as_str())
                    .replace("[dir]", &shquote(&dir.to_string_lossy()).as_str())
                    .replace("[Dir]", dir.to_string_lossy().as_ref())
                    .replace("[ext]", format!(".{}", &match path.extension() {
                        Some(s)=>s.to_string_lossy(),
                        None=> std::borrow::Cow::Borrowed(""),
                    }).as_str())
                    .replace("[name]", &quoted_fname)
                    .replace("[Name]", &fname)
                    .replace("[part]", &quoted_part)
                    .replace("[Part]", part);
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
            }
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
            let offset = scrollable::AbsoluteOffset{x:0.0, y:abspos - self.content_y};
            self.update_scroll(offset.y);
            return scrollable::scroll_to(self.scroll_id.clone(), offset);
        }
        Command::none()
    }

    fn click_item(self: &mut Self, ii: usize, shift: bool, ctrl: bool, always_sel: bool) {

        let isdir = self.items[ii].isdir();
        self.last_clicked = LastClicked{
            new: true,
            nav_id: self.nav_id,
            iidx: ii,
            didx: self.items[ii].display_idx,
            size: if isdir { None } else {
                let bytes = self.items[ii].size as f64;
                Some(if bytes > 1073741824.0 { format!("{:.2}GB", bytes/1073741824.0 ) }
                else if bytes > 1048576.0 { format!("{:.1}MB", bytes/1048576.0 ) }
                else if bytes > 1024.0 { format!("{:.0}KB", bytes/1024.0 ) }
                else { format!("{:.0}B", bytes) })
                }
        };
        let prevsel = self.items.iter().filter_map(|item| {
            if item.sel {
                Some(item.items_idx)
            } else {
                None
            }
        }).collect::<Vec<usize>>();
        if self.conf.dir() && !isdir {
            prevsel.into_iter().for_each(|j|self.items[j].sel = false);
            return;
        }
        while (self.conf.multi() || isdir) && shift && prevsel.len() > 0 {
            let prevdir = self.items[prevsel[0]].isdir();
            if prevdir != isdir {
                break;
            }
            let mut lo = self.items[ii].display_idx;
            let mut hi = lo;
            prevsel.iter().for_each(|j| {
                lo = lo.min(self.items[*j].display_idx);
                hi = hi.max(self.items[*j].display_idx);
            });
            self.show_goto = false;
            for j in lo..=hi {
                let sel = self.items[self.displayed[j]].isdir() == isdir;
                let item = &mut self.items[self.displayed[j]];
                if sel && item.recursed {
                    self.show_goto = true;
                }
                item.sel = sel;
            }
            return;
        }
        self.show_goto = false;
        if always_sel || !self.items[ii].sel {
            self.items[ii].sel = true;
            self.show_goto = self.items[ii].recursed || self.dirs.len() > 1;
        } else if prevsel.len() == 1 || ctrl {
            self.items[ii].sel = false;
        }
        let mut any_selected = false;
        prevsel.into_iter().filter(|j|*j != ii).for_each(|j| {
            if !(ctrl && (self.conf.multi()||isdir)) || self.items[j].isdir() != isdir {
                self.items[j].sel = false;
            } else {
                self.show_goto |= self.items[j].recursed || self.dirs.len() > 1;
                any_selected = true;
            }
        });
        self.pathbar = if self.items[ii].sel {
            any_selected = true;
            self.items[ii].path.clone()
        } else {
            self.last_clicked.size = None;
            self.dirs[0].clone()
        };
        self.enable_sel_button = any_selected || self.conf.saving() || self.conf.dir();
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
        self.end_idx = self.items.len();
        self.items.iter_mut().enumerate().for_each(|(i,item)|item.items_idx = i);
        self.displayed = displayed;
        self.enable_sel_button = self.conf.saving() || self.conf.dir();
        self.update_searcher_items(self.items.iter().map(|item|item.path.clone()).collect());
    }

    fn update_searcher_items(self: &mut Self, searchable: Vec<String>) {
        if let Some(ref mut sender) = self.search_commander {
            sender.send(SearchEvent::NewItems(searchable, self.nav_id)).unwrap();
            sender.send(SearchEvent::NewView(self.displayed.clone())).unwrap();
        }
    }

    fn update_searcher_visible(self: &mut Self, displaylist: Vec<usize>) {
        if let Some(ref mut sender) = self.search_commander {
            sender.send(SearchEvent::NewView(displaylist)).unwrap();
            sender.send(SearchEvent::Search(self.searchbar.clone())).unwrap();
        }
    }

    fn rem_bookmark(self: &mut Self, idx: usize) {
        self.conf.bookmarks.remove(idx);
        self.conf.update(true);
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

    fn exit(self: &mut Self) {
        self.conf.update(false);
        process::exit(0);
    }
}

async fn paste(path: String, dest: String, cut: bool) {
    let _ = tokio::process::Command::new(if cut { "mv" } else { "cp" })
        .arg(path).arg(dest).output().await;
}

impl Icons {
    fn new(thumbsize: f32) -> Self {
        let home = std::env::var("HOME").unwrap();
        let tpath = Path::new(&home).join(".cache").join("pikeru").join("thumbnails");
        let cando_pdf = std::process::Command::new("which")
            .arg("pdftoppm").output().map_or(false, |output| output.status.success());
        let cando_epub = std::process::Command::new("which")
            .arg("epub-thumbnailer").output().map_or(false, |output| output.status.success());
        Self {
            folder: Self::prerender_svg(include_bytes!("../assets/folder7.svg"), thumbsize),
            unknown:  Self::prerender_svg(include_bytes!("../assets/file6.svg"), thumbsize),
            doc:  Self::prerender_svg(include_bytes!("../assets/document.svg"), thumbsize),
            error:  Self::prerender_svg(include_bytes!("../assets/error.svg"), thumbsize),
            audio:  Self::prerender_svg(include_bytes!("../assets/music4.svg"), thumbsize),
            thumb_dir: tpath.to_string_lossy().to_string(),
            settings: svg::Handle::from_memory(include_bytes!("../assets/settings2.svg")),
            updir: svg::Handle::from_memory(include_bytes!("../assets/up2.svg")),
            newdir: svg::Handle::from_memory(include_bytes!("../assets/newdir2.svg")),
            cmds: svg::Handle::from_memory(include_bytes!("../assets/cmd2.svg")),
            goto: svg::Handle::from_memory(include_bytes!("../assets/goto2.svg")),
            cando_pdf,
            cando_epub,
        }
    }
    fn prerender_svg(img_bytes: &[u8], thumbsize: f32) -> Handle {
        let opts = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(img_bytes.as_ref(), &opts).unwrap();
        let (w, h) = (tree.size().width(), tree.size().height());
        let scale = thumbsize as f32 * 0.8 / w.max(h);
        let w = (w * scale) as u32;
        let h = (h * scale) as u32;
        let numpix = w * h * 4;
        let transforem = tiny_skia::Transform::from_scale(scale, scale);
        let mut pixels = vec![0; numpix as usize];
        let mut pixmap = tiny_skia::PixmapMut::from_bytes(&mut pixels, w, h).unwrap();
        resvg::render(&tree, transforem, &mut pixmap);
        Handle::from_pixels(w, h, pixels)
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
            builtin: false,
        }
    }

    fn builtin(label: &str) -> Self {
        Cmd {
            label: label.into(),
            cmd: Default::default(),
            builtin: true,
        }
    }
}

enum ClickType {
    Single,
    Double,
    Pass,
}
struct ClickTimer {
    idx: usize,
    time: Instant,
    preclicked: Option<(usize, Instant)>,
}
impl ClickTimer {
    fn preclick(self: &mut Self, idx: usize) {
        self.preclicked = Some((idx, Instant::now()));
    }
    fn click(self: &mut Self, idx: usize, always_valid: bool) -> ClickType {
        if !always_valid {
            match self.preclicked {
                None => return ClickType::Pass,
                Some((pidx, inst)) => {
                    if pidx != idx || inst.elapsed().as_secs() > 1 {
                        return ClickType::Pass;
                    }
                },
            };
        }
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

fn vid_frame(src: &str, thumbnail: Option<u32>, savepath: Option<&PathBuf>) -> Option<Handle> {
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
            if let Some(out) = savepath {
                let encoder = webp::Encoder::from_rgba(rgba.as_ref(), w, h);
                let wp = encoder.encode_simple(false, 50.0).unwrap();
                std::fs::write(out, &*wp).unwrap();
            }
            Some(Handle::from_pixels(w, h, rgba))
        },
        Err(e) => {
            eprintln!("Error decoding {}: {}", src, e);
            None
        }
    }
}

async fn search_loop(mut commands: UReceiver<SearchEvent>,
                     result_sender: USender<SearchEvent>) {
    let mut items = vec![];
    let mut displayed = vec![];
    let mut nav_id = 0;
    let semantics = RefCell::new(HashMap::<String,&'static str>::new());
    let matcher = fuzzy_matcher::skim::SkimMatcherV2::default();
    loop {
        match commands.recv().await {
            Some(SearchEvent::NewItems(paths, nid)) => {
                items = paths.into_iter().map(|path| {
                    nav_id = nid;
                    let sem = semantics.borrow();
                    let text = sem.get(&path).copied();
                    FileIdx {
                        path,
                        text,
                    }
                }).collect();
            },
            Some(SearchEvent::AddItems(paths)) => {
                let sem = semantics.borrow();
                let mut new_items = paths.into_iter().map(|path| {
                    let text = sem.get(&path).copied();
                    FileIdx {
                        path,
                        text,
                    }
                }).collect::<Vec<FileIdx<'_>>>();
                items.append(&mut new_items);
            },
            Some(SearchEvent::NewView(didxs)) => {
                displayed = didxs;
            }
            Some(SearchEvent::AddView(mut didxs)) => {
                displayed.append(&mut didxs);
            }
            Some(SearchEvent::AddSemantics(new_sem)) => {
                let mut sem = semantics.borrow_mut();
                new_sem.into_iter().for_each(|ps|{sem.insert(ps.0, Box::leak(ps.1.into_boxed_str()));});
                items.iter_mut().for_each(|item|{
                    if item.text == None {
                        item.text = sem.get(&item.path).copied();
                    }
                });
            },
            Some(SearchEvent::Search(term)) => {
                let mut results = displayed.iter().filter_map(|i| {
                    let item = &items[*i];
                    let path = Path::new(item.path.as_str());
                    let name_match = matcher.fuzzy_match(match path.file_name() {
                        Some(p) => p.to_string_lossy(),
                        None => path.to_string_lossy(),
                    }.as_ref(), term.as_str());
                    let sem_match = match item.text {
                        Some(text) => matcher.fuzzy_match(text, term.as_str()),
                        None => None,
                    };
                    match (name_match, sem_match) {
                        (Some(a), Some(b)) => Some((*i, a.max(b))),
                        (Some(a), None) => Some((*i, a)),
                        (None, Some(b)) => Some((*i, b)),
                        (None, None) => None,
                    }
                }).collect::<Vec<_>>();
                results.sort_by(|a,b|b.1.cmp(&a.1));
                result_sender.send(SearchEvent::Results(results, nav_id, items.len(), term)).unwrap();
            },
            _ => unreachable!(),
        }
    }
}

enum RecMsg {
    NewNav(Vec::<String>, u8),
    FetchMore(u8, bool),
    NextItems(Vec<FItem>, u8),
    SetRecursive(bool),
}

async fn recursive_add(mut updates: UReceiver<RecMsg>,
                       results: USender<RecMsg>,
                       selfy: USender<RecMsg>,
                       semchan: USender<SearchEvent>,
                       gitignore_txt: String,
                       respect_gitignore: bool) {
    let mut nav_id = 0;
    let mut recursive = true;
    let mut dirs = vec![];
    let mut ignores: Vec<Vec<Arc<gitignore::Gitignore>>> = vec![];
    let mut indexer = IndexProxy::new().await;
    indexer.configure(respect_gitignore, gitignore_txt.as_str()).await;
    let mut ig_builder = gitignore::GitignoreBuilder::new("");
    gitignore_txt.lines().for_each(|line|{ig_builder.add_line(None, line).unwrap();});
    let top_ignore = Arc::new(ig_builder.build().unwrap());
    loop {
        match updates.recv().await {
            Some(RecMsg::SetRecursive(rec)) => {
                recursive = rec;
            },
            Some(RecMsg::NewNav(new_dirs, nid)) => {
                dirs = new_dirs;
                ignores = dirs.iter().map(|_|vec![top_ignore.clone()]).collect();
                nav_id = nid;
                selfy.send(RecMsg::FetchMore(nid, false)).unwrap();
            }
            Some(RecMsg::FetchMore(nid, get_items)) => {
                if nid != nav_id {
                    continue;
                }
                let mut new_items = vec![];
                let mut next_dirs = vec![];
                let mut next_ignores = vec![];
                let semantics = indexer.update(&dirs).await;
                if !semantics.is_empty() {
                    semchan.send(SearchEvent::AddSemantics(semantics)).unwrap();
                }
                if !recursive {
                    continue;
                }
                for (i, dir) in dirs.iter().enumerate() {
                    match std::fs::read_dir(dir.as_str()) {
                        Ok(rd) => {
                            if respect_gitignore {
                                let local_ignore = Path::new(&dir).join(".gitignore");
                                if local_ignore.is_file() {
                                    ignores[i].push(Arc::new(gitignore::Gitignore::new(local_ignore).0));
                                }
                            }
                            rd.filter_map(|r| match r { Ok(d) => Some(d.path()), Err(_) => None, }).for_each(|path| {
                                let ignored = ignores[i].iter().any(|g|match g.matched(&path, path.is_dir()) {
                                    Match::Ignore(_) => true,
                                    _ => false,
                                });
                                if ignored {
                                    return;
                                }
                                if path.is_dir() {
                                    next_dirs.push(path.to_string_lossy().to_string());
                                    next_ignores.push(ignores[i].clone());
                                }
                                if get_items {
                                    let mut item = FItem::new(path.into(), nid);
                                    item.recursed = true;
                                    new_items.push(item);
                                }
                            });
                        },
                        Err(e) => eprintln!("Error reading dir {}: {}", dir, e),
                    }
                }
                dirs = next_dirs;
                ignores = next_ignores;
                if !new_items.is_empty() {
                    results.send(RecMsg::NextItems(new_items, nid)).unwrap();
                }
            },
            _ => {},
        };
    }
}
