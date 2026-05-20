use unicode_segmentation::UnicodeSegmentation;
use std::fmt;
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
mod theme;
use iced::{
    advanced::widget::Id,
    Rectangle, Padding, Color,
    alignment,
    Task, Length, Element,
    widget::{
        slider,
        Id as WidgetId,
        Id as CId,
        image, image::Handle, Column, Row, text, responsive,
        Scrollable, scrollable, scrollable::Direction,
        TextInput, Text, Checkbox, Button,
        column, row, container,
        stack, opaque, center,
        svg,
        rule,
        space,
    },
    futures::{
        sink::SinkExt,
        StreamExt,
    },
    event::{self, Status},
    keyboard::Modifiers,
    keyboard::key::Named,
    mouse::{ScrollDelta, Button as MouseButton},
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
use mime_guess;
use getopts::Options;
use inotify::{Inotify, WatchMask, WatchDescriptor, EventMask};
use iced_aw::{
    MenuBar, Menu,
    ContextMenu,
    Spinner,
};
use iced_aw::menu::Item;
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
        IndexProxy::clear_queue();
        std::process::exit(0);
    }
    if flags.opt_present("v") {
        println!("1.16");
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
    let resizeable = conf.resizeable_flag.unwrap_or(conf.resizeable.is_true());
    video_rs::init().unwrap();

    let win_size = conf.window_size;
    iced::application(
        move || boot(conf.clone()),
        update,
        view,
    )
    .subscription(|_| pikeru_subscription())
    .theme(iced::Theme::Dark)
    .window(
        iced::window::Settings {
            position: iced::window::Position::Centered,
            resizable: resizeable,
            ..iced::window::Settings::default()
        }
    )
    .window_size(win_size)
    .run()
}

#[derive(PartialEq, Clone, Copy)]
enum TriBool {
    True,
    False,
    OnlyNotPortal,
}

impl fmt::Display for TriBool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriBool::True => write!(f, "true"),
            TriBool::False => write!(f, "false"),
            TriBool::OnlyNotPortal => write!(f, "sometimes"),
        }
    }
}
impl TriBool {
    fn is_true(&self) -> bool {
        if *self == TriBool::OnlyNotPortal {
            return !matches!(std::env::var("PK_XDG"), Ok(_))
        }
        *self == TriBool::True
    }
}

#[derive(PartialEq, Clone, Copy)]
enum DelConfirm {
    Always,
    Never,
    Key,
    Click,
}
impl DelConfirm {
    fn need_confirm(&self, by_key: bool) -> bool {
        match self {
            DelConfirm::Always => true,
            DelConfirm::Never => false,
            DelConfirm::Key => by_key,
            DelConfirm::Click => !by_key,
        }
    }
}
impl fmt::Display for DelConfirm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DelConfirm::Always => write!(f, "true"),
            DelConfirm::Never => write!(f, "false"),
            DelConfirm::Key => write!(f, "key"),
            DelConfirm::Click => write!(f, "click"),
        }
    }
}

#[derive(Clone)]
struct Config {
    title: String,
    id: String,
    path: String,
    mode: Mode,
    sort_by: i32,
    bookmarks: Vec<Bookmark>,
    cmds: Vec<Cmd>,
    terminal: String,
    thumb_size: f32,
    dpi_scale: f64,
    window_size: Size,
    need_update: bool,
    gitignore: String,
    respect_gitignore: bool,
    icon_view: bool,
    home: String,
    resizeable: TriBool,
    resizeable_flag: Option::<bool>,
    keep_open: bool,
    forget_changes: bool,
    do_index: bool,
    show_hidden: bool,
    show_sidebar: bool,
    icon_theme: Option<String>,
    font_name: Option<String>,
    delete_confirmation: DelConfirm,
    auto_icon_threshold: Option<usize>,
    command_confirmation: bool,
}

impl Config {

    #[inline]
    fn saving(self: &Self) -> bool { self.mode == Mode::Save }
    #[inline]
    fn multi(self: &Self) -> bool { self.mode == Mode::Files }
    #[inline]
    fn dir(self: &Self) -> bool { self.mode == Mode::Dir }

    fn new() -> Config {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut opts = Options::new();
        let pwd = std::env::var("PWD").unwrap();
        opts.optopt("t", "title", "Title of the filepicker window", "NAME");
        opts.optopt("i", "id", "ID of the filepicker window (default 'pikeru')", "ID");
        opts.optopt("m", "mode", "Mode of file selection. Default is files", "[file, files, save, dir]");
        opts.optopt("p", "path", "Initial path", "PATH");
        opts.optopt("g", "geom", "window size", "WxH");
        opts.optflag("c", "clear", "Clear the semantic search indexer queue");
        opts.optflag("d", "disable", "Configure xdg portal to not use pikeru as your system filepicker");
        opts.optflag("e", "enable", "Configure xdg portal to use pikeru as your system filepicker");
        opts.optflag("u", "unresizeable", "Make window unresizable to avoid tiling it on tiling window managers");
        opts.optflag("r", "resizeable", "Make window resizable");
        opts.optflag("l", "list", "Start in list view mode");
        opts.optflag("n", "icon", "Start in icon view mode");
        opts.optflag("k", "keep", "Keep window open to select more files");
        opts.optflag("f", "forget", "Don't update the config with any changed settings");
        opts.optflag("x", "noindex", "Don't update the semantic search index with visited directories");
        opts.optflag("s", "noside", "No sidebar");
        opts.optflag("h", "help", "Show usage information");
        opts.optflag("v", "version", "Show pikeru version");
        let matches = match opts.parse(&args) {
            Ok(m) => m,
            Err(e) => die!("Bad args: {}", e),
        };
        if matches.opt_present("h") {
            let cando_pdf = std::process::Command::new("which").arg("pdftoppm").output().map_or(false, |output| output.status.success());
            let cando_epub = std::process::Command::new("which").arg("epub-thumbnailer").output().map_or(false, |output| output.status.success());
            let extra_thumbs = match (cando_pdf, cando_epub) {
                (true,true) => "",
                (true,false) => "To handle epub thumbnails, install epub-thumbnailer.",
                (false,true) => "To handle pdf thumbnails, install pdftoppm.",
                (false,false) => "To handle pdf and epub thumbnails, install pdftoppm and epub-thumbnailer.",
            };
            println!("{}\n{}\n{}\n{}",opts.usage("pikeru"),
                "Keybindings when an input box is not focused:
  h/j/k/l      Navigate left/down/up/right (vim-style)
  Arrow keys   Navigate (same as h/j/k/l)
  v            Toggle between icon and list view
  i            Focus the filepath input
  s or /       Focus the search bar
  n            Create new directory
  t            Open terminal in current directory
  y            Copy selected file(s)
  p            Paste from clipboard
  1            Sort by name (ascending)
  2            Sort by name (descending)
  3            Sort by age (oldest first)
  4            Sort by age (newest first)
  Delete       Delete selected file(s)
  Tab          Cycle through bookmarks (forward)
  Shift+Tab    Cycle through bookmarks (backward)
  Backspace    Go up one directory
  Enter        Open/enter selected item
  Space        Toggle image preview
  q            Exit",
                "\nFile picker config file is ~/.config/pikeru.conf.\nThe portal config file which includes the semantic search indexer and postprocessor, is by default ~/.config/xdg-desktop-portal-pikeru/config.",
                extra_thumbs);
            std::process::exit(0);
        }

        let home = std::env::var("HOME").unwrap();
        let confpath = Path::new(&home).join(".config").join("pikeru.conf").to_string_lossy().to_string();
        let tpath = Path::new(&home).join(".cache").join("pikeru").join("thumbnails");
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
                            Cmd::builtin("Terminal"),
                        ];
        let mut respect_gitignore = true;
        let mut icon_view = true;
        let mut gitignore = format!("{}\n{}/*\n", ".git/".to_string(), tpath.to_string_lossy());
        let mut sort_by = 1;
        let mut thumb_size = 160.0;
        let mut window_size: Size = Size { width: 1024.0, height: 768.0 };
        let mut dpi_scale: f32 = 1.0;
        let mut show_hidden = false;
        let mut delete_confirmation = DelConfirm::Key;
        let mut icon_theme: Option<String> = None;
        let mut font_name: Option<String> = None;
        let mut terminal = String::new();
        let mut auto_icon_threshold: Option<usize> = None;
        let mut command_confirmation: bool = false;
        let mut opts_missing = 13;
        let mut resizeable = match std::env::var("XDG_CURRENT_DESKTOP").unwrap_or("".to_string()).to_lowercase().as_str() {
            "i3"|"sway"|"dwm"|"dwl"|"hyprland"|"bspwm"|"awesome"|"xmonad"|"qtile"|"spectrwm"|"herbstluftwm"|"notion" => TriBool::OnlyNotPortal,
            _ => TriBool::True,
        };
        let mut resizeable_flag: Option::<bool> = None;
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
                        S::Commands => {
                            if k == "Terminal" {
                                terminal = v.to_string();
                            } else {
                                cmds.push(Cmd::new(k, v));
                            }
                        },
                        S::Bookmarks => bookmarks.push(Bookmark::new(k,v)),
                        S::Ignore => {gitignore += line; gitignore += "\n"; },
                        S::Settings => match k {
                            "thumbnail_size" => { opts_missing -= 1; thumb_size = v.parse().unwrap() },
                            "dpi_scale" => { opts_missing -= 1; dpi_scale = v.parse().unwrap() },
                            "respect_gitignore" => { opts_missing -= 1; respect_gitignore = v.parse().unwrap() },
                            "show_hidden" => { opts_missing -= 1; show_hidden = v.parse().unwrap() },
                            "delete_confirmation" => {
                                opts_missing -= 1;
                                delete_confirmation = match v.to_lowercase().as_str() {
                                    "true" => DelConfirm::Always,
                                    "key" => DelConfirm::Key,
                                    "click" => DelConfirm::Click,
                                    _ => DelConfirm::Never,
                                }
                            },
                            "icon_view" => { opts_missing -= 1; icon_view = v.parse().unwrap() },
                            "resizeable" => {
                                opts_missing -= 1;
                                resizeable = match v.to_lowercase().as_str() {
                                    "true" => TriBool::True,
                                    "false" => TriBool::False,
                                    _ => TriBool::OnlyNotPortal,
                                }
                            },
                            "window_size" => {
                                opts_missing -= 1;
                                if !match str::split_once(v, 'x') {
                                    Some(wh) => match (wh.0.parse::<f32>(), wh.1.parse::<f32>()) {
                                        (Ok(w),Ok(h)) => {window_size = Size {width: w, height: h}; true},
                                        (_,_) => false,
                                    }
                                    None => false,
                                } { eprintln!("window_size must have format WIDTHxHEIGHT"); }
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
                            "icon_theme" => {
                                opts_missing -= 1;
                                icon_theme = if v.is_empty() { None } else { Some(v.to_string()) };
                            },
                            "font_name" => {
                                opts_missing -= 1;
                                font_name = if v.is_empty() { None } else { Some(v.to_string()) };
                            },
                            "auto_icon_threshold" => {
                                opts_missing -= 1;
                                auto_icon_threshold = if v.is_empty() { None } else { Some(v.parse().unwrap_or(0)) };
                            },
                            "command_confirmation" => {
                                opts_missing -= 1;
                                command_confirmation = v.parse().unwrap_or(false);
                            },
                            _ => {},
                        },
                    }
                },
            }
        }
        cli(&matches);
        if let Some(g) = matches.opt_str("g") {
            if !match str::split_once(g.as_str(), 'x') {
                Some(wh) => match (wh.0.parse::<f32>(), wh.1.parse::<f32>()) {
                    (Ok(w),Ok(h)) => {window_size = Size {width: w, height: h}; true},
                    (_,_) => false,
                }
                None => false,
            } { eprintln!("geometry flag must have format WIDTHxHEIGHT"); }
        }

        if let Err(_) = tpath.metadata() {
            std::fs::create_dir_all(&tpath).unwrap();
        };
        if bookmarks.is_empty() {
            bookmarks.push(Bookmark::new("Home", &home));
            bookmarks.push(Bookmark::new("Downloads", Path::new(&home).join("Downloads").to_string_lossy().as_ref()));
            bookmarks.push(Bookmark::new("Documents", Path::new(&home).join("Documents").to_string_lossy().as_ref()));
            bookmarks.push(Bookmark::new("Pictures", Path::new(&home).join("Pictures").to_string_lossy().as_ref()));
        }
        if matches.opt_present("u") {
            resizeable_flag = Some(false);
        } else if matches.opt_present("r") {
            resizeable_flag = Some(true);
        }
        if matches.opt_present("l") {
            icon_view = false;
        } else if matches.opt_present("n") {
            icon_view = true;
        }
        Config {
            mode: Mode::from(matches.opt_str("m")),
            path: matches.opt_str("p").unwrap_or(pwd),
            title: matches.opt_str("t").unwrap_or("File Picker".to_string()),
            id: matches.opt_str("i").unwrap_or("pikeru".to_string()),
            keep_open: matches.opt_present("k"),
            forget_changes: matches.opt_present("f"),
            do_index: !matches.opt_present("x"),
            cmds,
            terminal,
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
            resizeable,
            resizeable_flag,
            show_hidden,
            delete_confirmation,
            show_sidebar: !matches.opt_present("s"),
            icon_theme,
            font_name,
            auto_icon_threshold,
            command_confirmation,
        }
    }

    fn update(self: &mut Config, force: bool) {
        if self.forget_changes || !self.need_update && !force {
            return;
        }
        self.need_update = false;
        let mut conf = String::from("# Commands from the cmd menu will substitute the follwong values from the selected files before running.
# Paths and filenames are already quoted for you when using lowercase like [path],
# or unquoted when capitalized like [Path].
# [path] is full file path
# [name] is the filename without full path
# [dir] is the current directory without trailing slash
# [part] is the filename without path or extension
# [ext] is the file extension, including the period
# Terminal = <command> sets the terminal emulator to open in the current directory.
#   Leave blank (no Terminal entry or 'Terminal =') to auto-detect a suitable terminal.
[Commands]\n");
        conf.push_str("Terminal = ");
        conf.push_str(&self.terminal);
        conf.push('\n');
        self.cmds.iter().skip(6).for_each(|cmd| {
            conf.push_str(&cmd.label);
            conf.push_str(" = ");
            conf.push_str(&cmd.cmd);
            conf.push('\n');
        });
        conf.push_str("\n[Settings]\n");
        conf.push_str(format!(
"dpi_scale = {}
window_size = {}x{}
thumbnail_size = {}
sort_by = {}
respect_gitignore = {}
icon_view = {}
show_hidden = {}
# delete_confirmation can be true|false|key|click
delete_confirmation = {}
# command_confirmation: ask before running user-specified commands on files
command_confirmation = {}
icon_theme = {}
font_name = {}
# resizeable can be true|false|sometimes. \"sometimes\" is only unresizeable when launched by the xdg portal.
# This makes tiling window managers give it a floating window instead of tiling it.
resizeable = {}
# auto_icon_threshold: if the number of visible image files is >= this value, automatically switch to icon view. leave blank to disable.
auto_icon_threshold = {}
",
                self.dpi_scale,
                self.window_size.width as i32, self.window_size.height as i32,
                self.thumb_size as i32,
                match self.sort_by { 1=>"name_asc", 2=>"name_desc", 3=>"age_asc", 4=>"age_desc", _=>"" },
                self.respect_gitignore,
                self.icon_view,
                self.show_hidden,
                self.delete_confirmation,
                self.command_confirmation,
                self.icon_theme.as_deref().unwrap_or(""),
                self.font_name.as_deref().unwrap_or(""),
                self.resizeable,
                self.auto_icon_threshold.map_or("".to_string(), |n| n.to_string())
                    ).as_str());
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

#[derive(PartialEq, Clone)]
enum Mode {
    File,
    Files,
    Save,
    Dir,
}
impl Mode {
   fn from(opt: Option<String>) -> Mode {
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
    Delete,
    DeleteConfirmOK,
    CommandConfirmOK,
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
    IconThemeSelected(String),
    FontSelected(String),
    ToggleThemePane,
    StartDiscovery,
    DiscoveryComplete((Vec<String>, Vec<(String, PathBuf)>)),
    SearchResult(Box<SearchEvent>),
    NextRecurse(Vec<FItem>, u8),
    RecurseDone,
    PageUp,
    PageDown,
    Spacebar,
    FocusFilepath,
    FocusSearch,
    CycleBookmark,
    CycleBookmarkBack,
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
    PdfEpub,
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
    mtime: i64,
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
    // Generic bucket icons (used when no themed icon is found)
    folder: Handle,
    doc: Handle,
    unknown: Handle,
    error: Handle,
    audio: Handle,
    // Per-MIME-type icons loaded from the icon theme
    archive: Handle,
    code: Handle,
    pdf_icon: Handle,
    epub_icon: Handle,
    video: Handle,
    image: Handle,
    // UI icons (bundled)
    thumb_dir: String,
    settings: svg::Handle,
    updir: svg::Handle,
    newdir: svg::Handle,
    cmds: svg::Handle,
    goto: svg::Handle,
    cando_pdf: bool,
    cando_epub: bool,
    // Icon theme name for dynamic lookups
    theme_name: Option<String>,
}

#[derive(Clone)]
struct Bookmark {
    label: String,
    path: String,
    id: WidgetId,
}

#[derive(Debug, Clone)]
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
    DeleteConfirm(Vec<String>),
    CommandConfirm(String),
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
    async fn clear_queue(&self) -> Result<()>;
    async fn configure(&mut self, respect_gitignore: bool, ignore: &str) -> Result<()>;
}
struct IndexProxy<'a> {
    proxy: Option<IndexerProxy<'a>>,
    done: HashSet<String>,
    sql: Option<rusqlite::Connection>,
}
impl<'a> IndexProxy<'a> {

    async fn new(do_index: bool) -> IndexProxy<'a> {
        let proxy = if do_index {
            async {
                let conn = Connection::session().await.ok()?;
                let prox = IndexerProxy::new(&conn).await.ok()?;
                Some(prox)
            }.await
        } else { None };
        let home = std::env::var("HOME").unwrap();
        let idxfile = Path::new(&home).join(".cache").join("pikeru").join("index.db");
        let sql = match rusqlite::Connection::open(&idxfile) {
            Ok(con) => Some(con),
            Err(_) => None,
        };
        IndexProxy {
            proxy,
            done: HashSet::new(),
            sql,
        }
    }

    fn clear_queue() {
        let conn = match blocking::Connection::session() {
            Ok(s) => s,
            Err(e) => die!("Error: {:?}", e),
        };
        let prox = match IndexerProxyBlocking::new(&conn) {
            Ok(s) => s,
            Err(e) => die!("Error: {:?}", e),
        };
        match prox.clear_queue() {
            Err(e) => eprintln!("Error:{}", e),
            Ok(_) => eprintln!("Cleared indexing queue."),
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
            let qmap = query.query_map(rusqlite::params_from_iter(filtered.iter()), |row|{
                Ok((row.get(0)?, row.get(1)?))
            });
            match qmap {
                Ok(q) => q.filter_map(|r|r.ok()).collect(),
                Err(_) => Vec::new(),
            }
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
        self.view_counter = self.view_counter.wrapping_add(1);
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
    scroll_id: WidgetId,
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
    goto_paths: Vec<String>,
    show_theme_pane: bool,
    discovering_themes_and_fonts: bool,
    enable_sel_button: bool,
    row_sizes: RefCell<RowSizes>,
    pos_state: RefCell<Measurements>,
    icon_themes: Option<Vec<String>>,
    font_names: Option<Vec<(String, PathBuf)>>,
    font: Option<iced::Font>,
    search_id: WidgetId,
    filepath_id: WidgetId,
    unfocus_id: WidgetId,
    new_dir_id: WidgetId,
    rename_id: WidgetId,
    clipboard_paths: Vec<String>,
    clipboard_cut: bool,
    pending_delete_paths: Vec<String>,
    pending_cmd: Option<usize>,
}

/// Boot the application (replacement for Application::new in iced 0.14).
fn boot(conf: Config) -> (FilePicker, iced::Task<Message>) {
    let pathstr = conf.path.clone();
    let path = Path::new(&pathstr);
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
                if let Some(fname) = path.file_name() {
                    Some(fname.to_string_lossy().to_string())
                } else { None }
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
        let search_id = WidgetId::unique();
        let filepath_id = WidgetId::unique();
        let unfocus_id = WidgetId::unique();
        let new_dir_id = WidgetId::unique();
        let rename_id = WidgetId::unique();
        let icon_theme = conf.icon_theme.clone();
        (
            FilePicker {
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
                icons: Arc::new(Icons::new(ts, icon_theme)),
                clicktimer: ClickTimer{ idx:0, time: Instant::now() - Duration::from_secs(1), preclicked: None},
                ctrl_pressed: false,
                shift_pressed: false,
                scroll_id: WidgetId::unique(),
                nav_id: 0,
                view_id: 0,
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
                goto_paths: vec![],
                show_theme_pane: false,
                discovering_themes_and_fonts: false,
                enable_sel_button,
                row_sizes: RefCell::new(RowSizes::new()),
                pos_state: RefCell::new(Measurements::default()),
                icon_themes: None,
                font_names: None,
                font: None,
                search_id: search_id.clone(),
                filepath_id: filepath_id.clone(),
                unfocus_id,
                new_dir_id: new_dir_id.clone(),
                rename_id: rename_id.clone(),
                clipboard_paths: vec![],
                clipboard_cut: false,
                pending_delete_paths: vec![],
                pending_cmd: None,
            },
            Task::batch({
                // TODO: iced 0.14 removed window::Id::MAIN, need to track window_id
                let mut cmds: Vec<Task<Message>> = vec![];
                if saving {
                    cmds.push(iced::widget::operation::focus(filepath_id));
                } else {
                    cmds.push(iced::widget::operation::focus(search_id));
                }
                cmds
            })
        )
    }
    /// Update function (replacement for Application::update in iced 0.14).
fn update(state: &mut FilePicker, message: Message) -> iced::Task<Message> {
        match message {
            Message::Thumbsize(size) => {
                state.conf.thumb_size = (size / 10.0).round() * 10.0;
                state.conf.need_update = true;
                state.row_sizes.borrow_mut().reset(false);
            },
            Message::InoCreate(file) => {
                let mut item = FItem::new(file.as_str().into(), state.nav_id);
                let len = state.items.len();
                item.display_idx = state.displayed.len();
                item.items_idx = len;
                state.displayed.push(len);
                if let Some(ts) = state.thumb_sender.as_ref() {
                    tokio::spawn(item.clone().load(ts.clone(), state.icons.clone(), state.conf.thumb_size as u32));
                }
                state.items.push(item);
                state.end_idx += 1;
            },
            Message::InoDelete(file) => {
                if let Some(i) = state.items.iter().position(|x|x.path == file) {
                    let dix = state.itod(i);
                    state.items.iter_mut().for_each(|m|{
                        if m.items_idx >= i { m.items_idx-=1 };
                        if m.display_idx >= dix { m.display_idx-=1 };
                    });
                    state.displayed.iter_mut().for_each(|m| if *m >= i { *m-=1 });
                    state.items.remove(i);
                    state.end_idx -= 1;
                    state.displayed.remove(dix);
                    state.update_searcher_items(state.items.iter().map(|item|item.path.clone()).collect());
                }
            },
            Message::RunCmd(i) => return state.run_command(i),
            Message::Delete => {
                let selected: Vec<String> = state.items.iter().filter(|item| item.sel).map(|item| item.path.clone()).collect();
                if state.conf.delete_confirmation.need_confirm(true) {
                    state.pending_delete_paths = selected;
                    state.modal = FModal::DeleteConfirm(state.pending_delete_paths.clone());
                    return Task::none();
                }
                for path in &selected {
                    if let Err(e) = OsCmd::new("rm").arg("-rf").arg(path).output() {
                        eprintln!("Error deleting {}: {}", path, e);
                    }
                }
            },
            Message::DeleteConfirmOK => {
                let paths = mem::take(&mut state.pending_delete_paths);
                for path in &paths {
                    if let Err(e) = OsCmd::new("rm").arg("-rf").arg(path).output() {
                        eprintln!("Error deleting {}: {}", path, e);
                    }
                }
                state.modal = FModal::None;
            },
            Message::CommandConfirmOK => {
                state.modal = FModal::None;
                if let Some(cmd_idx) = state.pending_cmd {
                    let cmd = state.run_command(cmd_idx);
                    state.pending_cmd = None;
                    return cmd;
                }
            },
            Message::Dummy => {},
            Message::IconThemeSelected(theme) => {
                let theme = if theme == "None" { None } else { Some(theme) };
                state.conf.icon_theme = theme.clone();
                state.conf.need_update = true;
                // Reload icons with the new theme
                state.icons = Arc::new(Icons::new(state.conf.thumb_size, theme));
                // Re-load current directory to refresh folder icons
                return update(state, Message::LoadDir);
            }
            Message::FontSelected(font_name) => {
                let font_name = if font_name == "System default" {
                    None
                } else {
                    Some(font_name)
                };
                state.conf.font_name = font_name.clone();
                state.conf.need_update = true;

                if let Some(name) = font_name {
                    if let Some((_, path)) = state.font_names.iter().flat_map(|f| f.iter()).find(|(n, _)| n == &name) {
                        match std::fs::read(path) {
                            Ok(bytes) => {
                                if let Some(internal_name) = theme::get_font_internal_name(path) {
                                    // Leak the internal name so we can reference it as &'static str
                                    let static_name: &'static str = Box::leak(internal_name.into_boxed_str());
                                    let load_cmd = iced::font::load(bytes);
                                    state.font = Some(iced::Font::with_name(static_name));
                                    return Task::batch(vec![
                                        load_cmd.map(move |result| {
                                            match result {
                                                Ok(_) => Message::Dummy,
                                                Err(_) => {
                                                    eprintln!("Failed to load font '{}'", name);
                                                    Message::Dummy
                                                }
                                            }
                                        }),
                                    ]);
                                } else { eprintln!("Font '{}' has no internal name table", name); }
                            }
                            Err(e) => { eprintln!("Failed to read font file {}: {}", path.display(), e); }
                        }
                    } else { eprintln!("Font '{}' not found in available fonts", name); }
                } else {
                    state.font = None;
                }
                return Task::none();
            }
            Message::StartDiscovery => {
                if (state.icon_themes.is_none() || state.font_names.is_none()) && !state.discovering_themes_and_fonts {
                    state.discovering_themes_and_fonts = true;
                    let existing_themes = state.icon_themes.clone();
                    let existing_fonts = state.font_names.clone();
                    return Task::perform(
                        async move { theme::discover_themes_async(existing_themes, existing_fonts).await },
                        Message::DiscoveryComplete,
                    );
                }
                return Task::none();
            }
            Message::ToggleThemePane => {
                state.show_theme_pane = !state.show_theme_pane;
                return Task::none();
            }
            Message::DiscoveryComplete((themes, fonts)) => {
                state.icon_themes = Some(themes);
                state.font_names = Some(fonts);
                state.discovering_themes_and_fonts = false;
                return Task::none();
            }
            Message::SetRecursive(rec) => {
                state.recursive_search = rec;
                if let Some(rs) = state.recurse_updater.as_ref() {
                    if !matches!(rs.send(RecMsg::SetRecursive(rec)), Ok(_)) {
                        if !rec { // reset searchable items in case already recursed
                            let items = state.items[..state.end_idx].iter().map(|item|item.path.clone()).collect::<Vec<_>>();
                            let iidxs = state.items[..state.end_idx].iter().map(|item|item.items_idx).collect::<Vec<_>>();
                            if let Some(ref mut sender) = state.search_commander {
                                let a = sender.send(SearchEvent::NewItems(items, state.nav_id));
                                let b = sender.send(SearchEvent::NewView(iidxs));
                                match (a,b) { (Ok(_),Ok(_)) => {}, _ => state.search_commander = None, };
                            }
                        }
                    } else { state.recurse_updater = None; }
                }
            },
            Message::ShowHidden(show) => {
                state.conf.show_hidden = show;
                state.conf.need_update = true;
                if !show {
                    state.enable_sel_button = state.conf.saving() || state.conf.dir() || state.items.iter().any(|item|item.sel);
                }
                let end = if state.searchbar.is_empty() { state.end_idx } else { state.displayed.len() };
                let displayed = state.items[..end].iter().enumerate().filter_map(|(i,item)| {
                    if show || !item.hidden { Some(i)
                    } else { None }
                }).collect();
                if state.searchbar.is_empty() {
                    state.displayed = displayed;
                    return update(state, Message::Sort(state.conf.sort_by));
                } else {
                    state.update_searcher_visible(displayed);
                }
            },
            Message::ChangeView => {
                state.conf.icon_view = !state.conf.icon_view;
                state.conf.need_update = true;
            },
            Message::Sort(i) => {
                match i {
                    1 => state.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = state.items.get_unchecked(*a);
                        let y = state.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||x.path.cmp(&y.path))
                    }),
                    2 => state.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = state.items.get_unchecked(*a);
                        let y = state.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||y.path.cmp(&x.path))
                    }),
                    3 => state.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = state.items.get_unchecked(*a);
                        let y = state.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||y.mtime.partial_cmp(&x.mtime).unwrap())
                    }),
                    4 => state.displayed.sort_by(|a:&usize,b:&usize| unsafe {
                        let x = state.items.get_unchecked(*a);
                        let y = state.items.get_unchecked(*b);
                        y.isdir().cmp(&x.isdir()).then_with(||x.mtime.partial_cmp(&y.mtime).unwrap())
                    }),
                    _ => unreachable!(),
                };
                state.displayed.iter().enumerate().for_each(|(i,j)|unsafe{state.items.get_unchecked_mut(*j)}.display_idx = i);
                state.conf.need_update |= i != state.conf.sort_by;
                state.conf.sort_by = i;
                state.row_sizes.borrow_mut().reset(true);
                return update(state, Message::LoadThumbs);
            },
            Message::PositionInfo(elem, widget, viewport) => {
                match elem {
                    Pos::Item => {
                        state.content_viewport = viewport;
                        if state.last_clicked.new {
                            state.last_clicked.new = false;
                            return state.keep_in_view(widget, viewport);
                        }
                    },
                    Pos::Content(clicked_offscreen) => {
                        state.content_height = widget.height;
                        state.content_y = widget.y;
                        state.content_viewport.height = widget.height;
                        if state.last_clicked.new && state.last_clicked.nav_id == state.nav_id && clicked_offscreen {
                            state.last_clicked.new = false;
                            let ii = state.last_clicked.iidx;
                            if let Some(rect) = state.itopos(ii) {
                                return state.keep_in_view(rect, state.content_viewport);
                            }
                        }
                    },
                    Pos::Row(counter, i) => {
                        let mut rs = state.row_sizes.borrow_mut();
                        if counter == rs.view_counter {
                            if rs.rows.len() <= i {
                                rs.rows.resize_with(state.num_rows(state.pos_state.borrow().max_cols),
                                                    Rowsize::default);
                            }
                            if i > rs.last_recv+1 {
                                rs.next_send = rs.last_recv+1;
                            } else if !rs.rows[i].ready {
                                rs.last_recv = i;
                                rs.num_ready += 1;
                                let pos = widget.y - state.content_y;
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
                state.update_scroll(viewport.absolute_offset().y);
            },
            Message::PageUp => {
                let current = state.scroll_offset.y;
                let offset = scrollable::AbsoluteOffset{x:0.0, y:(current - state.content_height).max(0.0)};
                state.update_scroll(offset.y);
                return iced::widget::operation::scroll_to(state.scroll_id.clone(), offset);
            },
            Message::PageDown => {
                let current = state.scroll_offset.y;
                let end = if state.conf.icon_view {
                    state.row_sizes.borrow().rows.last().map_or(state.content_height, |r|r.end_pos)
                } else {
                    state.displayed.len() as f32 * ROW_HEIGHT
                };
                let mut newpos = current + state.content_height;
                let max = end - state.content_height;
                if max >= 0.0 { newpos = newpos.min(max); }
                let offset = scrollable::AbsoluteOffset{x:0.0, y:newpos};
                state.update_scroll(offset.y);
                return iced::widget::operation::scroll_to(state.scroll_id.clone(), offset);
            },
            Message::FocusFilepath => {
                return iced::widget::operation::focus(state.filepath_id.clone());
            }
            Message::FocusSearch => {
                return iced::widget::operation::focus(state.search_id.clone());
            }
            Message::CycleBookmark => {
                if state.conf.bookmarks.is_empty() {
                    return Task::none();
                }
                let current = state.dirs.first().map(|s| s.as_str());
                let idx = if let Some(dir) = current {
                    state.conf.bookmarks.iter().position(|bm| bm.path == dir)
                        .map(|i| (i + 1) % state.conf.bookmarks.len())
                        .unwrap_or(0)
                } else {
                    0
                };
                return update(state, Message::LoadBookmark(idx));
            }
            Message::CycleBookmarkBack => {
                if state.conf.bookmarks.is_empty() {
                    return Task::none();
                }
                let current = state.dirs.first().map(|s| s.as_str());
                let idx = if let Some(dir) = current {
                    state.conf.bookmarks.iter().position(|bm| bm.path == dir)
                        .map(|i| if i == 0 { state.conf.bookmarks.len() - 1 } else { i - 1 })
                        .unwrap_or(0)
                } else {
                    0
                };
                return update(state, Message::LoadBookmark(idx));
            }
            Message::Spacebar => {
                match state.view_image.1 {
                    Preview::None => {
                        if let Some(sel) = state.items.iter().find(|&item|item.sel) {
                            if sel.ftype == FType::Image {
                                state.view_image = (sel.items_idx, sel.preview());
                            }
                        }
                    }, _ => {
                        state.view_image = (0, Preview::None);
                        return iced::widget::operation::scroll_to(state.scroll_id.clone(), state.scroll_offset);
                    }
                }
            },
            Message::DropBookmark(idx, cursor_pos) => {
                return iced_drop::zones_on_point(
                    move |zones| Message::HandleZones(idx, zones),
                    cursor_pos, None, None,
                );
            }
            Message::DeleteBookmark(idx) => {
                state.rem_bookmark(idx);
                state.modal = FModal::None;
            },
            Message::EditBookmark(idx) => {
                state.modal = FModal::EditBookmark(idx);
                state.new_bm_path = state.conf.bookmarks[idx].path.clone();
                state.new_bm_label = state.conf.bookmarks[idx].label.clone();
            },
            Message::NewBmPathInput(path) => state.new_bm_path = path,
            Message::NewBmLabelInput(label) => state.new_bm_label = label,
            Message::UpdateBookmark(idx) => {
                let mut changed = false;
                if !state.new_bm_path.is_empty() {
                    changed = true;
                    state.conf.bookmarks[idx].path = mem::take(&mut state.new_bm_path);
                }
                if !state.new_bm_label.is_empty() {
                    changed = true;
                    state.conf.bookmarks[idx].label = mem::take(&mut state.new_bm_label);
                }
                state.modal = FModal::None;
                if changed {
                    state.conf.update(true);
                }
            },
            Message::LoadBookmark(idx) => {
                state.dir_history.push(mem::take(&mut state.dirs));
                state.dirs = vec![state.conf.bookmarks[idx].path.clone()];
                state.update_scroll(0.0);
                return update(state, Message::LoadDir);
            },
            Message::HandleZones(idx, zones) => {
                if zones.len() > 0 {
                    let targets: Vec<_> = state.conf.bookmarks.iter().enumerate().filter_map(|(i, bm)| {
                        if zones[0].0 == bm.id.clone().into() {
                            Some(i)
                        } else {None}
                    }).collect();
                    let target = if targets.len() > 0 {
                        Some(targets[0] as i32)
                    } else if zones[0].0 == Id::new("bookmarks") {
                        Some(-1)
                    } else { None };
                    state.add_bookmark(idx, target);
                }
            }
            Message::Init((fichan, inochan, search_res, more_files)) => {
                let (txino, watch_cmds) = unbounded_channel::<Inochan>();
                let (txsrch, search_cmds) = unbounded_channel::<SearchEvent>();
                let (txrec, recurse_cmds) = unbounded_channel::<RecMsg>();
                tokio::spawn(watch_inotify(watch_cmds, inochan));
                state.search_commander = Some(txsrch.clone());
                state.ino_updater = Some(txino);
                state.thumb_sender = Some(fichan);
                tokio::spawn(recursive_add(recurse_cmds, more_files, txrec.clone(), txsrch,
                                           state.conf.gitignore.clone(), state.conf.respect_gitignore, state.conf.do_index));
                state.recurse_updater = Some(txrec);
                tokio::spawn(search_loop(search_cmds, search_res));
                return update(state, Message::LoadDir);
            },
            Message::PathTxtInput(txt) => state.pathbar = txt,
            Message::SearchTxtInput(txt) => {
                state.searchbar = txt;
                if state.searchbar.is_empty() {
                    state.search_running = false;
                    state.recurse_state = RecState::Stop;
                    let mut have_sel = false;
                    state.displayed = state.items[..state.end_idx].iter().enumerate().filter_map(|(i,item)| {
                        if state.conf.show_hidden || !item.hidden {
                            have_sel |= item.sel;
                            Some(i)
                        } else {None}
                    }).collect();
                    state.show_goto = have_sel && state.dirs.len() > 1;
                    state.enable_sel_button = state.conf.saving() || state.conf.dir() || have_sel;
                    return update(state, Message::Sort(state.conf.sort_by));
                } else if !state.search_running{
                    if let Some(sc) = state.search_commander.as_ref() {
                        if matches!(sc.send(SearchEvent::Search(state.searchbar.clone())), Ok(_)) {
                            state.search_running = true;
                            if state.recurse_state != RecState::Run {
                                if let Some(ru) = state.recurse_updater.as_ref() {
                                    if let Ok(_) = ru.send(RecMsg::FetchMore(state.nav_id, true)) {
                                        state.recurse_state = RecState::Run;
                                    }
                                }
                            }
                        } else { state.search_commander = None; }
                    }
                }
            },
            Message::SearchResult(res) => {
                let mut still_running = false;
                if let SearchEvent::Results(res, nav_id, num_items, term) = *res {
                    if nav_id == state.nav_id && !state.searchbar.is_empty() {
                        state.displayed = res[..1000.min(res.len())].into_iter().enumerate().map(|(di,ii)|{
                            state.items[ii.0].display_idx = di;
                            ii.0
                        }).collect();
                        let _ = update(state, Message::LoadThumbs);
                        if term != state.searchbar || num_items != state.items.len() {
                            if let Some(sc) = state.search_commander.as_ref() {
                                if matches!(sc.send(SearchEvent::Search(state.searchbar.clone())), Ok(_)) {
                                    still_running = true;
                                } else { state.search_commander = None; }
                            }
                        }
                        state.row_sizes.borrow_mut().reset(true);
                    }
                }
                state.search_running = still_running;
            },
            Message::NextRecurse(mut next_items, nav_id) => {
                if nav_id == state.nav_id {
                    let mut new_displayed = vec![];
                    let paths = next_items.iter_mut().enumerate().map(|(i,fitem)| {
                        fitem.items_idx = state.items.len() + i;
                        if state.conf.show_hidden || !fitem.hidden {
                            new_displayed.push(fitem.items_idx);
                        }
                        fitem.path.clone()
                    }).collect();
                    if let Some(sender) = state.search_commander.as_ref() {
                        state.items.append(&mut next_items);
                        let a = sender.send(SearchEvent::AddItems(paths));
                        let b = sender.send(SearchEvent::AddView(new_displayed));
                        match (a,b) {
                            (Ok(_),Ok(_)) => {
                                if !state.search_running {
                                    _ = sender.send(SearchEvent::Search(state.searchbar.clone()));
                                }
                                if state.recurse_state != RecState::Stop {
                                    if let Some(ru) = state.recurse_updater.as_ref() {
                                        if matches!(ru.send(RecMsg::FetchMore(nav_id, true)), Err(_)) {
                                            state.recurse_updater = None;
                                        }
                                    }
                                }
                            },
                            _ => state.search_commander = None,
                        }
                    }
                }
            },
            Message::RecurseDone => {
                state.recurse_state = RecState::Stop;
            },
            Message::Ctrl(pressed) => state.ctrl_pressed = pressed,
            Message::Shift(pressed) => state.shift_pressed = pressed,
            Message::ArrowKey(key) => {
                let didx = if state.items.iter().filter(|item|!item.recursed || !state.searchbar.is_empty()).any(|item|item.sel) {
                    let maxcols = state.pos_state.borrow().max_cols as i64;
                    let i = state.last_clicked.didx as i64;
                    match key {
                        Named::ArrowUp => i - maxcols,
                        Named::ArrowDown => i + maxcols,
                        Named::ArrowLeft => i - 1,
                        Named::ArrowRight => i + 1,
                        _ => -1,
                    }
                } else { 0 };
                match state.view_image.1 {
                    Preview::None => {},
                    _ => {
                        let step = didx - (state.last_clicked.didx as i64);
                        return update(state, Message::NextImage(step));
                    },
                }
                if didx >= 0 && didx < state.displayed.len() as i64 {
                    state.click_item(state.dtoi(didx as usize), state.shift_pressed, state.ctrl_pressed, false);
                }
            },
            Message::LoadThumbs => {
                let mut max_load = state.nproc.min(state.displayed.len());
                state.view_id = state.view_id.wrapping_add(1);
                let mut di: usize = 0;
                while di < state.displayed.len() && max_load > 0 {
                    let ii = state.displayed[di];
                    if state.items[ii].not_loaded() {
                        if let Some(ts) = state.thumb_sender.as_ref() {
                            let mut item = mem::replace(&mut state.items[ii], FItem::placeholder(ii, di));
                            item.view_id = state.view_id;
                            tokio::spawn(item.load(ts.clone(), state.icons.clone(), state.conf.thumb_size as u32));
                            max_load -= 1;
                        }
                    }
                    di += 1;
                }
                state.last_loaded = di;
            },
            Message::NextItem(mut doneitem) => {
                if doneitem.nav_id == state.nav_id {
                    if doneitem.view_id == state.view_id {
                        let mut prev_di = state.last_loaded;
                        while prev_di < state.displayed.len() {
                            let i = state.dtoi(prev_di);
                            if state.items[i].not_loaded() {
                                if let Some(ts) = state.thumb_sender.as_ref() {
                                    let mut nextitem = mem::replace(&mut state.items[i], FItem::placeholder(i, prev_di));
                                    nextitem.view_id = state.view_id;
                                    tokio::spawn(nextitem.load(ts.clone(), state.icons.clone(), state.conf.thumb_size as u32));
                                }
                                break;
                            }
                            prev_di += 1;
                        }
                        state.last_loaded = prev_di + 1;
                    }
                    let j = doneitem.items_idx;
                    doneitem.display_idx = state.items[j].display_idx;
                    state.items[j] = doneitem;
                }
            },
            Message::Goto => {
                state.goto_paths = state.items.iter().filter(|item|item.sel).map(|item|item.path.clone()).collect();
                state.dir_history.push(mem::take(&mut state.dirs));
                state.dirs = state.items.iter().filter(|item|item.sel).filter_map(|item|
                    Some(Path::new(&item.path).parent()?.to_string_lossy().to_string())).collect();
                state.update_scroll(0.0);
                return update(state, Message::LoadDir);
            }
            Message::LoadDir => {
                state.view_image = (0, Preview::None);
                state.last_clicked.new = false;
                state.update_scroll(0.0);
                state.pathbar = match &state.save_filename {
                    Some(fname) => Path::new(&state.dirs[0]).join(fname).to_string_lossy().to_string(),
                    None => state.dirs[0].clone(),
                };
                state.load_dir();
                state.show_goto = false;
                state.search_running = false;
                state.recurse_state = RecState::Stop;
                if let Some(ru) = state.recurse_updater.as_ref() {
                    if matches!(ru.send(RecMsg::NewNav(state.dirs.clone(), state.nav_id)), Err(_)) {
                        state.recurse_updater = None;
                    }
                }
                let _ = update(state, Message::Sort(state.conf.sort_by));
                // After Goto, select the previously selected files in the new directory
                let goto_indices: Vec<usize> = state.goto_paths.iter().filter_map(|path|
                    state.items.iter().position(|item| item.path == *path)).collect();
                for ii in goto_indices {
                    state.click_item(ii, false, false, false);
                }
                state.goto_paths.clear();
                let mut cmds = vec![iced::widget::operation::snap_to(state.scroll_id.clone(), scrollable::RelativeOffset::START)];
                if state.conf.saving() {
                    cmds.push(iced::widget::operation::focus(state.filepath_id.clone()));
                    let mut extlen = Path::new(state.pathbar.as_str()).extension().map_or(0, |s|s.len());
                    let pathlen = state.pathbar.chars().count();
                    if extlen > 0 && pathlen > extlen+1 { extlen += 1; }
                    cmds.push(iced::widget::operation::move_cursor_to(state.filepath_id.clone(), pathlen-extlen));
                } else {
                    cmds.push(iced::widget::operation::focus(state.search_id.clone()));
                }
                return Task::batch(cmds);
            },
            Message::DownDir => {
                if let Some(dirs) = state.dir_history.pop() {
                    state.dirs = dirs;
                    state.update_scroll(0.0);
                    return update(state, Message::LoadDir);
                }
            },
            Message::UpDir => {
                let dirs = mem::take(&mut state.dirs);
                state.dirs = dirs.iter().map(|dir| {
                    let path = Path::new(dir.as_str());
                    match path.parent() {
                        Some(par) => par.as_os_str().to_str().unwrap_or(dir.as_str()).to_string(),
                        None => dir.clone(),
                    }
                }).unique_by(|s|s.to_owned()).collect();
                state.dir_history.push(dirs);
                return update(state, Message::LoadDir);
            },
            Message::Rename => {
                if state.new_path.basename.is_empty() {
                    return Task::none()
                }
                if let Some(ref mut item) = state.items.iter_mut().find(|i|i.sel) {
                    match OsCmd::new("mv").arg(&item.path).arg(&state.new_path.full_path).output() {
                        Ok(output) if output.status.success() => {
                            (item.label, item.hidden) = make_label(&state.new_path.full_path);
                            item.path = mem::take(&mut state.new_path.full_path);
                        },
                        Err(e) => {
                            let err = format!("Error renaming {} to {}: {}", item.path, state.new_path.basename, e);
                            eprintln!("{}", err);
                            state.modal = FModal::Error(err);
                        },
                        _ => {
                            let err = format!("Error renaming {} to {}", item.path, state.new_path.basename);
                            eprintln!("{}", err);
                            state.modal = FModal::Error(err);
                        },
                    }
                }
                state.new_path.reset();
                match state.modal {
                    FModal::Error(_) => {},
                    _ => state.modal = FModal::None,
                }
            }
            Message::NewDir(confirmed) => if confirmed {
                    let path = Path::new(&state.dirs[0]).join(&state.new_path.basename);
                    if let Err(e) = std::fs::create_dir_all(&path) {
                        let msg = format!("Error creating directory: {:?}", e);
                        state.modal = FModal::Error(msg);
                    } else {
                        state.modal = FModal::None;
                    }
                } else {
                    state.new_path.reset();
                    state.modal = FModal::NewDir;
                    return iced::widget::operation::focus(state.new_dir_id.clone());
                },
            Message::NewPathInput(path) => state.new_path.update(path),
            Message::CloseModal => state.modal = FModal::None,
            Message::MiddleClick(iidx) => state.click_item(iidx, false, true, false),
            Message::LeftPreClick(iidx) => state.clicktimer.preclick(iidx),
            Message::LeftClick(iidx, always_valid) => {
                match state.clicktimer.click(iidx, always_valid) {
                    ClickType::Single => state.click_item(iidx, state.shift_pressed, state.ctrl_pressed, iidx == state.view_image.0),
                    ClickType::Double => {
                        state.items[iidx].sel = true;
                        return update(state, Message::Select(SelType::Click));
                    },
                    ClickType::Pass => {},
                }
                return iced::widget::operation::focus(state.unfocus_id.clone());
            },
            Message::RightClick(iidx) => {
                if iidx >= 0 {
                    let iidx = iidx as usize;
                    let item = &state.items[iidx];
                    if item.ftype == FType::Image {
                        state.view_image = (item.items_idx, item.preview());
                        state.click_item(iidx, false, false, true);
                    } else {
                        state.click_item(iidx, true, false, false);
                    }
                } else {
                    state.view_image = (0, Preview::None);
                    return iced::widget::operation::scroll_to(state.scroll_id.clone(), state.scroll_offset);
                }
                return iced::widget::operation::focus(state.unfocus_id.clone());
            },
            Message::NextImage(step) => {
                match state.view_image.1 {
                    Preview::None => {},
                    _ => {
                        let mut didx = state.itod(state.view_image.0) as i64;
                        while (step<0 && didx>0) || (step>0 && didx<((state.displayed.len()-1) as i64)) {
                            didx = (didx as i64) + step;
                            if didx<0 || didx as usize>=state.displayed.len() {
                                return Task::none();
                            }
                            let di = didx as usize;
                            let ii = state.dtoi(di);
                            if state.items[ii].ftype == FType::Image {
                                match state.items[ii].preview() {
                                    Preview::None => {},
                                    pv => {
                                        state.view_image = (state.dtoi(di), pv);
                                        return update(state, Message::LeftClick(state.view_image.0, true));
                                    },
                                }
                            }
                        }
                    },
                }
            },
            Message::OverWriteOK => {
                println!("{}", state.pathbar);
                state.exit();
            },
            Message::Select(seltype) => {
                if state.conf.saving() {
                    if !state.pathbar.is_empty() {
                        let result = Path::new(&state.pathbar);
                        if result.is_file() {
                            state.modal = FModal::OverWrite;
                        } else if result.is_dir() {
                            state.dir_history.push(mem::take(&mut state.dirs));
                            state.dirs = vec![state.pathbar.clone()];
                            state.update_scroll(0.0);
                            return update(state, Message::LoadDir);
                        } else {
                            println!("{}", state.pathbar);
                            state.exit();
                        }
                    }
                } else if state.conf.dir() {
                    let sel = Path::new(match state.items.iter().find(|item|item.sel) {
                        Some(item) => &item.path,
                        None => &state.pathbar,
                    });
                    if sel.is_dir() {
                        if seltype == SelType::Click {
                            state.dirs = vec![state.items.iter().find(|it|it.sel).unwrap().path.clone()];
                            return update(state, Message::LoadDir);
                        } else {
                            println!("{}", sel.to_string_lossy());
                            state.exit();
                        }
                    } else if sel.is_file() {
                        if let Some(p) = sel.parent() {
                            println!("{}", p.to_string_lossy());
                            state.exit();
                        }
                    }
                } else {
                    let pb =  FItem::new(PathBuf::from(&state.pathbar), state.nav_id);
                    let sels: Vec<&FItem> = match seltype {
                        SelType::TxtEntr => vec![&pb],
                        _ => state.items.iter().filter(|item| item.sel ).collect(),
                    };
                    if sels.len() != 0 {
                        match sels[0].ftype {
                            FType::Dir => {
                                if state.conf.dir() && sels.len() == 1 && seltype == SelType::Button {
                                    println!("{}", sels[0].path);
                                    state.exit();
                                } else {
                                    state.dirs = sels.iter().filter_map(|item| match item.ftype {
                                        FType::Dir => Some(item.path.clone()), _ => None}).collect();
                                    return update(state, Message::LoadDir);
                                }
                            },
                            FType::NotExist => {},
                            _ => {
                                println!("{}", sels.iter().map(|item|item.path.as_str()).join("\n"));
                                state.exit();
                            }
                        }
                    }
                }
            },
            Message::Cancel => {
                state.conf.update(false);
                process::exit(0);
            },
        }
        Task::none()
    }

    fn pikeru_subscription() -> iced::Subscription<Message> {
        let items = iced::Subscription::run(|| {
            use iced::futures::stream;
            use iced::futures::channel::mpsc;
            use iced::futures::StreamExt;
            let (mut sender, receiver) = mpsc::channel(100);
            let runner = stream::once(async move {
                let mut state = SubState::Starting;
                loop {
                    match &mut state {
                        SubState::Starting => {
                            let (fi_sender, fi_reciever) = unbounded_channel::<FItem>();
                            let (ino_sender, ino_receiver) = unbounded_channel::<Inochan>();
                            let (search_sender, search_receiver) = unbounded_channel::<SearchEvent>();
                            let (rec_sender, rec_receiver) = unbounded_channel::<RecMsg>();
                            sender.send(Message::Init((fi_sender, ino_sender, search_sender, rec_sender))).await.unwrap();
                            state = SubState::Ready((fi_reciever,ino_receiver,search_receiver, rec_receiver));
                        }
                        SubState::Ready((thumb_recv, ino_recv, search_recv, rec_recv)) => {
                            tokio::select! {
                                more = rec_recv.recv() => {
                                    if let Some(msg) = more {
                                        match msg {
                                            RecMsg::NextItems(items, nav_id) => {
                                                sender.send(Message::NextRecurse(items, nav_id)).await.unwrap();
                                            }
                                            RecMsg::Done(_nav_id) => {
                                                sender.send(Message::RecurseDone).await.unwrap();
                                            }
                                            _ => {}
                                        }
                                    }
                                },
                                res = search_recv.recv() => {
                                    if let Some(search_event) = res {
                                        sender.send(Message::SearchResult(Box::new(search_event))).await.unwrap();
                                    }
                                },
                                item = thumb_recv.recv() => sender.send(Message::NextItem(item.unwrap())).await.unwrap(),
                                evt = ino_recv.recv() => {
                                    match evt {
                                        Some(Inochan::Delete(file)) => sender.send(Message::InoDelete(file)).await.unwrap(),
                                        Some(Inochan::Create(file)) => sender.send(Message::InoCreate(file)).await.unwrap(),
                                        _ => {},
                                    }
                                }
                            }
                        },
                    }
                }
            }).filter_map(|_| async { None });
            stream::select(receiver, runner)
        });
        let events = event::listen_with(pikeru_event_filter);
        iced::Subscription::batch(vec![items, events/*, native*/])
    }

    /// View function (replacement for Application::view in iced 0.14).
fn view<'a>(state: &'a FilePicker) -> iced::Element<'a, Message> {
        responsive(|size| {
            let view_menu = |items| Menu::new(items).max_width(180.0).offset(15.0).spacing(3.0);
            let cmd_list = state.conf.cmds.iter().enumerate().map(
                |(i,cmd)|{
                    if state.clipboard_paths.is_empty() && cmd.label == "Paste" {
                        Item::new(Button::new(container(text(cmd.label.as_str()))
                                    .width(Length::Fill)
                                    .align_x(alignment::Horizontal::Center))
                            .padding(1.0)
                            .style(style::top_but_style()))
                    } else {
                        Item::new(menu_button(cmd.label.as_str(), Message::RunCmd(i)))
                    }
                }).collect();
            let (sidebar, sidebar_width) = if state.conf.show_sidebar {
                let font = state.font;
                (Some(state.conf.bookmarks.iter().enumerate().fold(column![], |col,(i,bm)| {
                        let mut txt = Text::new(bm.label.as_str())
                            .size(15.0);
                        if let Some(f) = font { txt = txt.font(f); }
                        let bm_button = container(Button::new(
                                    container(txt)
                                        .align_x(alignment::Horizontal::Center)
                                        .width(Length::Fill)
                                        .padding(-3.0)
                                        .id(bm.id.clone()))
                                 .style(style::side_but_style())
                                 .on_press(Message::LoadBookmark(i)));
                        let ctx_menu = ContextMenu::new(bm_button, move || {
                            column![
                                Button::new(Text::new("Delete"))
                                    .on_press(Message::DeleteBookmark(i))
                                    .width(Length::Fill)
                                    .style(style::top_but_style()),
                                Button::new(Text::new("Edit"))
                                    .on_press(Message::EditBookmark(i))
                                    .width(Length::Fill)
                                    .style(style::top_but_style()),
                            ].width(Length::Fixed(100.0)).into()
                        });
                        col.push(ctx_menu)
                    }).push(container(space::vertical()).height(Length::Fill).width(Length::Fill)
                            .id(CId::new("bookmarks"))).width(Length::Fixed(120.0))), 140.0)
            } else { (None, 10.0) };
            // Auto-switch to icon view if image count meets threshold
            let icon_view = state.conf.icon_view ||
                if let Some(threshold) = state.conf.auto_icon_threshold {
                    threshold == 0 || state.displayed.iter()
                        .filter_map(|&idx| state.items.get(idx))
                        .filter(|item| { matches!(item.ftype, FType::Image | FType::PdfEpub) })
                        .nth(threshold - 1).is_some()
                } else {
                    false
                };
            let mut clicked_offscreen = false;
            let mut ps = state.pos_state.borrow_mut();
            ps.max_cols = if icon_view {
                ((size.width-sidebar_width) / (state.conf.thumb_size+2.0)).max(1.0) as usize
            } else { 1 };
            let content: iced::Element<'_, Message> = match &state.view_image.1 {
                Preview::Svg(handle) => {
                    mouse_area(container(svg(handle.clone())
                                        .width(Length::Fill)
                                        .height(Length::Fill))
                                   .align_x(alignment::Horizontal::Center)
                                   .align_y(alignment::Vertical::Center)
                                   .width(Length::Fill).height(Length::Fill))
                        .on_right_press(Message::RightClick(-1))
                        .on_release(Message::LeftClick(state.view_image.0, true))
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
                        .on_release(Message::LeftClick(state.view_image.0, true))
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
                        .on_release(Message::LeftClick(state.view_image.0, true))
                        .into()
                },
                Preview::None => {
                    if icon_view {
                        let thumb_width = (size.width-sidebar_width) / ps.max_cols as f32;
                        let num_rows = state.num_rows(ps.max_cols);
                        let top = state.scroll_offset.y - state.conf.thumb_size*1.1;
                        let bot = state.scroll_offset.y + state.content_height;
                        let mut rs = state.row_sizes.borrow_mut();
                        let mut rows = Column::new();
                        rs.checkcols(ps.max_cols);
                        if rs.rows.len() < num_rows {
                            rs.rows.resize_with(num_rows, Rowsize::default);
                        }
                        let first_idx = match rs.rows.iter().take_while(|r|r.ready).find_position(|r|r.pos > top) {
                            Some((i,r)) => {
                                rows = rows.push(space::vertical().height(r.pos));
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
                            if num_rows <= rs.num_ready && past_bot {
                                let last_pos = rs.rows.last().unwrap().end_pos;
                                rows = rows.push(space::vertical().height(last_pos - cur_row.pos));
                                break;
                            }
                            let mut row_all_ready = next_ready;
                            let mut row_none_ready = true;
                            if past_bot {
                                rows = rows.push(space::vertical().height(cur_row.end_pos - cur_row.pos));
                            } else {
                                let start = i * ps.max_cols;
                                let mut row = Row::new().width(Length::Fill);
                                for j in 0..ps.max_cols {
                                    let idx = start + j;
                                    if idx < state.displayed.len() {
                                        let item = &state.items[state.dtoi(idx)];
                                        row_all_ready &= item.thumb_handle != None;
                                        row_none_ready &= item.thumb_handle == None;
                                        let (clicked, display) = item.display_thumb(&state.last_clicked, thumb_width, state.font);
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
                        clicked_offscreen = state.last_clicked.new && !clicked_onscreen;
                        Scrollable::new(rows)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .on_scroll(Message::Scrolled)
                            .direction(Direction::Vertical(scrollable::Scrollbar::new()))
                            .id(state.scroll_id.clone()).into()
                    } else {
                        // list view
                        let mut rows = Column::new();
                        let mut clicked_onscreen = false;
                        let num_total = state.displayed.len();
                        let top = state.scroll_offset.y - ROW_HEIGHT*1.1;
                        let bot = state.scroll_offset.y + state.content_height;
                        let first_idx = (top / ROW_HEIGHT).floor().max(0.0) as usize;
                        let last_idx = (bot / ROW_HEIGHT).ceil().min((num_total.max(1) - 1) as f32) as usize;
                        if first_idx > 0 {
                            rows = rows.push(space::vertical().height(ROW_HEIGHT * first_idx as f32));
                        }
                        if num_total > 0 {
                            for i in first_idx..last_idx+1 {
                                let item = &state.items[state.dtoi(i)];
                                let (clicked, displayed) = item.display_row(&state.last_clicked, state.font);
                                clicked_onscreen |= clicked;
                                rows = rows.push(displayed);
                            }
                        }
                        if last_idx+1 < num_total {
                            rows = rows.push(space::vertical().height(ROW_HEIGHT * (num_total-1-last_idx) as f32));
                        }
                        clicked_offscreen = state.last_clicked.new && !clicked_onscreen;
                        Scrollable::new(rows)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .on_scroll(Message::Scrolled)
                            .direction(Direction::Vertical(scrollable::Scrollbar::new()))
                            .id(state.scroll_id.clone()).into()
                    }
                },
            };
            let count = Text::new(format!("  {} items", state.displayed.len()));
            let ctrlbar = column![
                row![
                    match (&state.last_clicked.size, state.show_goto) {
                        (Some(size), true) => row![Text::new(size),count, space::horizontal(), top_icon(state.icons.goto.clone(), Message::Goto)],
                        (Some(size), false) => row![Text::new(size),count, space::horizontal()],
                        (None, true) => row![count, space::horizontal(), top_icon(state.icons.goto.clone(), Message::Goto)],
                        (None, false) => row![count, space::horizontal()]
                    },
                    if state.search_running || matches!(state.recurse_state, RecState::Run) {
                        Element::from(Spinner::new().width(16).height(16).circle_radius(1.5))
                    } else {
                        Element::from(space::horizontal().width(16))
                    },
                    Element::<Message>::from(MenuBar::new(vec![
                        Item::with_menu(top_icon(state.icons.cmds.clone(), Message::Dummy), view_menu(cmd_list)),
                        Item::with_menu(top_icon(state.icons.settings.clone(), Message::StartDiscovery), view_menu(state.build_settings_menu())),
                    ]).spacing(1.0)),
                    top_icon(state.icons.newdir.clone(), Message::NewDir(false)),
                    top_icon(state.icons.updir.clone(), Message::UpDir),
                    top_button("Cancel", 100.0, Message::Cancel),
                    if state.enable_sel_button {
                        top_button(&state.select_button, 100.0, Message::Select(SelType::Button))
                    } else {
                        top_button_off(&state.select_button, 100.0)
                    }
                ].spacing(1).height(31.0),
                row![
                {
                    let mut input: TextInput<'_, Message, iced::Theme, iced::Renderer> = TextInput::new("directory or file path", state.pathbar.as_str())
                        .on_input(Message::PathTxtInput)
                        .on_paste(Message::PathTxtInput)
                        .on_submit(Message::Select(SelType::TxtEntr))
                        .width(Length::FillPortion(8))
                        .padding(2.0)
                        .id(state.filepath_id.clone());
                    if let Some(f) = state.font { input = input.font(f); }
                    Element::from(input)
                },
                {
                    let mut input: TextInput<'_, Message, iced::Theme, iced::Renderer> = TextInput::new("search", state.searchbar.as_str())
                        .on_input(Message::SearchTxtInput)
                        .on_paste(Message::SearchTxtInput)
                        .width(Length::FillPortion(2))
                        .padding(2.0)
                        .id(state.search_id.clone());
                    if let Some(f) = state.font { input = input.font(f); }
                    Element::from(input)
                },
                Button::new("X").on_press(Message::SearchTxtInput("".to_string())).style(style::flat_but_style())
                    .padding(Padding::from([2.0, 5.0]))
                ]
            ].align_x(alignment::Horizontal::Right).width(Length::Fill);
            let send = clicked_offscreen || ps.total_width != size.width || ps.total_height != size.height || state.content_height == 0.0;
            let mainview = column![
                ctrlbar,
                    {
                        let mut r = Row::new();
                        if let Some(sb) = sidebar { r = r.push(sb); }
                        let content_with_locator = wrapper::locator(content).send_info(move|a,b|Message::PositionInfo(
                               Pos::Content(clicked_offscreen),a,b), send);
                        r = r.push(content_with_locator);
                        // Show theme pane on the right if enabled
                        if state.show_theme_pane {
                            r = r.push(container(state.build_theme_pane()).width(Length::Fixed(250.0)));
                        } else if !icon_view && state.items.iter().any(|item| item.sel) {
                            // In list mode, show preview pane only when files are selected
                            r = r.push(container(state.build_preview_pane()).width(Length::Fixed(250.0)));
                        }
                        r
                    }
            ];
            ps.total_width = size.width;
            ps.total_height = size.height;
            match state.modal {
                FModal::None => mainview.into(),
                FModal::EditBookmark(i) => modal_overlay(mainview.into(),
                    container(column![
                        Text::new("Edit bookmark").size(24),
                        column![
                            Text::new("Label").size(12),
                            TextInput::new(&state.conf.bookmarks[i].label, state.new_bm_label.as_str())
                                .on_input(Message::NewBmLabelInput)
                                .on_submit(Message::UpdateBookmark(i))
                                .on_paste(Message::NewBmLabelInput)
                                .padding(5),
                        ].spacing(5),
                        column![
                            Text::new("Directory path").size(12),
                            TextInput::new(&state.conf.bookmarks[i].path, state.new_bm_path.as_str())
                                .on_input(Message::NewBmPathInput)
                                .on_submit(Message::UpdateBookmark(i))
                                .on_paste(Message::NewBmPathInput)
                                .padding(5),
                        ].spacing(5),
                        row![
                            Button::new("Update").on_press(Message::UpdateBookmark(i)).style(style::top_but_style()),
                            Button::new("Delete").on_press(Message::DeleteBookmark(i)).style(style::top_but_style()),
                            Button::new("Cancel").on_press(Message::CloseModal).style(style::top_but_style()),
                        ].spacing(5.0),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
                FModal::Rename(ref filename) => modal_overlay(mainview.into(),
                    container(column![
                        Text::new("Rename File").size(24),
                        column![
                            Text::new("Filename").size(12),
                            TextInput::new(filename, &state.new_path.basename)
                                .id(state.rename_id.clone())
                                .on_input(Message::NewPathInput)
                                .on_submit(Message::Rename)
                                .on_paste(Message::NewPathInput)
                                .padding(5),
                        ].spacing(5),
                        row![
                            Button::new("Rename").on_press(Message::Rename).style(style::top_but_style()),
                            Button::new("Cancel").on_press(Message::CloseModal).style(style::top_but_style()),
                        ].spacing(5.0),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
                FModal::Error(ref msg) => modal_overlay(mainview.into(),
                    container(column![
                        Text::new("Error").size(24),
                        text(msg),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
                FModal::OverWrite => modal_overlay(mainview.into(),
                    container(column![
                        Text::new("File exists. Overwrite?").size(24),
                        row![
                            Button::new("Overwrite").on_press(Message::OverWriteOK),
                            Button::new("Cancel").on_press(Message::CloseModal),
                        ].spacing(5.0),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
                FModal::NewDir => modal_overlay(mainview.into(),
                    container(column![
                        Text::new("Enter new directory name").size(24),
                        column![
                            Text::new("Directory name").size(12),
                            TextInput::new("Untitled", state.new_path.basename.as_str())
                                .id(state.new_dir_id.clone())
                                .on_input(Message::NewPathInput)
                                .on_submit(Message::NewDir(true))
                                .on_paste(Message::NewPathInput)
                                .padding(5),
                        ].spacing(5),
                        row![
                            Button::new("Create").on_press(Message::NewDir(true)).style(style::top_but_style()),
                            Button::new("Cancel").on_press(Message::CloseModal).style(style::top_but_style()),
                        ].spacing(5.0),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
                FModal::CommandConfirm(ref label) => modal_overlay(mainview.into(),
                    container(column![
                        Text::new(format!("Run \"{}\"?" , label)).size(24),
                        row![
                            Button::new("Run").on_press(Message::CommandConfirmOK),
                            Button::new("Cancel").on_press(Message::CloseModal),
                        ].spacing(5.0),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
                FModal::DeleteConfirm(ref paths) => modal_overlay(mainview.into(),
                    container(column![
                        Text::new(format!("Delete {} file{}?", paths.len(), if paths.len() == 1 { "" } else { "s" })).size(24),
                        row![
                            Button::new("Delete").on_press(Message::DeleteConfirmOK),
                            Button::new("Cancel").on_press(Message::CloseModal),
                        ].spacing(5.0),
                    ].spacing(10))
                    .padding(10)
                    .style(container::rounded_box)
                    .width(500.0)
                    .into(),
                Message::CloseModal),
            }
        }).into()
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

/// Create a modal overlay using native iced widgets (based on iced's modal example).
/// Replaces iced_aw::Card for modal dialogs.
fn modal_overlay<'a, Message: Clone + 'a>(
    background: Element<'a, Message>,
    modal_content: Element<'a, Message>,
    backdrop_msg: Message,
) -> Element<'a, Message> {
    stack![
        background,
        opaque(
            mouse_area(
                center(
                    container(
                        modal_content
                    )
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.8).into()),
                        ..container::Style::default()
                    })
                )
            )
            .on_press(backdrop_msg)
        )
    ]
    .into()
}

fn menu_button(txt: &str, msg: Message) -> Element<'_, Message> {
    Button::new(container(text(txt))
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center))
        .style(style::top_but_style())
        .padding(1.0)
        .on_press(msg).into()
}

/// Create a checkbox for toggling settings in the menu.
fn menu_button_checkbox(txt: &str, checked: bool, msg: Message) -> Element<'_, Message> {
    Checkbox::new(checked)
        .label(txt)
        .on_toggle(move |_| msg.clone())
        .width(Length::Fill)
        .into()
}

/// Create a theme selection button with a visual indicator for the selected theme.
fn theme_button<'a>(selected: bool, theme_name: &'a str) -> Element<'a, Message> {
    Button::new(container(text(theme_name))
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .padding([0.0, 10.0]))
        .style(if selected { style::top_but_style() } else { style::flat_but_style() })
        .padding(1.0)
        .on_press(Message::IconThemeSelected(theme_name.to_string())).into()
}
/// Create a font selection button with a visual indicator for the selected font.
fn font_button<'a>(txt: &'a str, selected: bool, font_name: &'a str) -> Element<'a, Message> {
    let padding_right = if selected { 10.0 } else { 15.0 };
    Button::new(container(text(txt))
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .padding([0.0, padding_right]))
        .style(if selected { style::top_but_style() } else { style::flat_but_style() })
        .padding(1.0)
        .on_press(Message::FontSelected(font_name.to_string())).into()
}
fn top_button(txt: &str, size: f32, msg: Message) -> Element<'_, Message> {
    Button::new(container(text(txt))
                .width(size)
                .align_x(alignment::Horizontal::Center))
        .style(style::top_but_style())
        .on_press(msg).into()
}
fn top_button_off(txt: &str, size: f32) -> Element<'_, Message> {
    Button::new(container(text(txt))
                .width(size)
                .align_x(alignment::Horizontal::Center))
        .style(style::top_but_style()).into()
}
fn top_icon(img: svg::Handle, msg: Message) -> Element<'static, Message> {
    Button::new(svg(img)
                .width(40.0))
        .style(style::top_but_style())
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

    fn display_row(&self, last_clicked: &LastClicked, font: Option<iced::Font>) -> (bool, Element<'_, Message>) {
        let mut row = Row::new();
        let idx = self.items_idx;
        if let Some(h) = &self.thumb_handle {
            let img = image(h.clone()).width(Length::Fixed(25.0));
            row = row.push(img);
        }
        //let shape = if self.unicode { text::Shaping::Advanced } else { text::Shaping::Basic };
        let shape = text::Shaping::Advanced;
        let mut txt = text(self.path.rsplitn(2,'/').next().unwrap()).width(Length::FillPortion(70)).shaping(shape);
        if let Some(f) = font { txt = txt.font(f); }
        row = row.push(container(txt).padding(Padding{ right: 0.0, left: 5.0, top: 0.0, bottom: 0.0 }));
        if !self.isdir() {
            let bytes = self.size as f64;
            let sz = if bytes > 1073741824.0 { format!("{:.2} GB", bytes/1073741824.0 ) }
            else if bytes > 1048576.0 { format!("{:.1} MB", bytes/1048576.0 ) }
            else if bytes > 1024.0 { format!("{:.0} KB", bytes/1024.0 ) }
            else { format!("{:.0} B", bytes) };
            let mut sz_txt = Text::new(sz);
            if let Some(f) = font { sz_txt = sz_txt.font(f); }
            row = row.push(container(sz_txt).padding(Padding{
                right: 15.0, left: 5.0, top: 0.0, bottom: 0.0
            }));
        }
        let systime = std::time::UNIX_EPOCH + Duration::from_secs(self.mtime.abs() as u64);
        let datetime: DateTime<Utc> = systime.into();
        let iso_8601_string = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
        let mut time_txt = Text::new(iso_8601_string);
        if let Some(f) = font { time_txt = time_txt.font(f); }
        row = row.push(container(time_txt).padding(Padding{
            right: 15.0, left: 0.0, top: 0.0, bottom: 0.0
        }));
        let clickable = match (self.isdir(), self.sel) {
            (true, true) => {
                let dr = iced_drop::droppable(row).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).height(ROW_HEIGHT).width(Length::Fill).style(style::selected_style()))
            },
            (true, false) => {
                let dr = iced_drop::droppable(row).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).height(ROW_HEIGHT).width(Length::Fill))
            },
            (false, true) => {
                mouse_area(container(row).height(ROW_HEIGHT).width(Length::Fill).style(style::selected_style()))
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

    fn display_thumb(&self, last_clicked: &LastClicked, thumbsize: f32, font: Option<iced::Font>) -> (bool, Element<'_, Message>) {
        const PAD: f32 = 2.0;
        let mut col = Column::new()
            .align_x(alignment::Horizontal::Center)
            .width(Length::Fixed(thumbsize-PAD*2.0));
        if let Some(handle) = &self.thumb_handle {
            if let image::Handle::Rgba{width,height,..} = handle {
                let (w,h) = (*width as f32, *height as f32);
                let scale = thumbsize as f32 / w.max(h);
                let w = w * scale;
                let h = h * scale;
                let im = image(handle.clone()).height(h).width(w);
                col = col.push(im);
            }
        }
        let shape = if self.unicode { text::Shaping::Advanced } else { text::Shaping::Basic };
        let mut txt = text(self.label.as_str()).size(13).shaping(shape);
        if let Some(f) = font { txt = txt.font(f); }
        col = col.push(txt);
        let idx = self.items_idx;
        let clickable = match (self.isdir(), self.sel) {
            (true, true) => {
                let dr = iced_drop::droppable(col).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).style(style::selected_style()).padding(PAD))
            },
            (true, false) => {
                let dr = iced_drop::droppable(col).on_drop(move |point,_| Message::DropBookmark(idx, point));
                mouse_area(container(dr).padding(PAD))
            },
            (false, true) => {
                mouse_area(container(col).style(style::selected_style()).padding(PAD))
            },
            (false, false) => {
                mouse_area(container(col).padding(PAD))
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
                                Preview::Image(Handle::from_rgba(w, h, rgba.as_raw().clone()))
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

    fn placeholder(ii: usize, di: usize) -> FItem {
        let mut ret = FItem(Box::new(Default::default()));
        ret.items_idx = ii;
        ret.display_idx = di;
        ret
    }

    fn new(pth: PathBuf, nav_id: u8) -> FItem {
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
        let msecs = mtime.duration_since(std::time::UNIX_EPOCH).map(|t|t.as_secs() as i64).unwrap_or_else(|e|{
            let diff = e.duration();
            let mut secs = diff.as_secs();
            if diff.subsec_nanos() > 0 {
                secs += 1;
            }
            -(secs as i64)
        });
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
            mtime: msecs,
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
            let mut file = File::open(cache_path).await.ok()?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).await.unwrap_or(0);
            let img = load_from_memory(buffer.as_ref()).ok()?;
            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
            return Some(Handle::from_rgba(w, h, rgba.as_raw().clone()))
        }
        if (imgtype == ImgType::Pdf && !icons.cando_pdf) || (imgtype == ImgType::Epub && !icons.cando_epub) {
            return Some(icons.doc.clone());
        }
        if fdir == Path::new(&icons.thumb_dir) {
            let mut buffer = Vec::new();
            let mut file = File::open(self.path.as_str()).await.ok()?;
            file.read_to_end(&mut buffer).await.unwrap_or(0);
            let img = load_from_memory(buffer.as_ref()).ok()?;
            let thumb = img.thumbnail(thumbsize, thumbsize);
            let (w,h,rgba) = (thumb.width(), thumb.height(), thumb.into_rgba8());
            Some(Handle::from_rgba(w, h, rgba.as_raw().clone()))
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
                            let img = load_from_memory(buffer.as_ref()).ok()?;
                            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
                            Some(Handle::from_rgba(w, h, rgba.as_raw().clone()))
                        },
                        Err(_) => None,
                    }
                },
                Err(_) => None,
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
                            let img = load_from_memory(buffer.as_ref()).ok()?;
                            let (w,h,rgba) = (img.width(), img.height(), img.into_rgba8());
                            Some(Handle::from_rgba(w, h, rgba.as_raw().clone()))
                        },
                        Err(_) => None,
                    }
                },
                Err(_) => None,
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
                                let mut pixmap = tiny_skia::PixmapMut::from_bytes(&mut pixels, w, h)?;
                                resvg::render(&tree, transforem, &mut pixmap);
                                let encoder = webp::Encoder::from_rgba(pixels.as_ref(), w, h);
                                let wp = encoder.encode_simple(false, 50.0).ok()?;
                                std::fs::write(cache_path, &*wp).ok()?;
                                Some(Handle::from_rgba(w, h, pixels))
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
                                let wp = encoder.encode_simple(false, 50.0).ok()?;
                                std::fs::write(cache_path, &*wp).ok()?;
                                Some(Handle::from_rgba(w, h, rgba.as_raw().clone()))
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

                    // Derive MIME type from extension via mime_guess.
                    let mime_type = mime_guess::from_ext(ext).first_or_octet_stream().to_string();
                    let needs_thumb = theme::mime_needs_thumbnail(&mime_type);
                    let is_audio = theme::mime_is_audio(&mime_type);

                    // 1. Try themed icon lookup for non-thumbnail types.
                    if !needs_thumb && ext != "pdf" && ext != "epub" {
                        if let Some(handle) = icons.lookup_themed_icon(&mime_type, ext, thumbsize) {
                            self.thumb_handle = Some(handle);
                            self.ftype = FType::File;
                            chan.send(self).unwrap();
                            return;
                        }
                    }

                    // 2. Thumbnail-generating image types
                    if matches!(ext, "svg" | "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "gif" | "webp") {
                        self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Norm, thumbsize, icons.clone()).await;
                        if let Some(_) = self.thumb_handle {
                            if ext == "svg" { self.svg = true; }
                            if ext == "gif" { self.gif = true; }
                            self.ftype = FType::Image;
                        } else {
                            self.thumb_handle = Some(icons.error.clone());
                            self.ftype = FType::File;
                        }
                    // 3. Video types that use video-rs for frame extraction
                    } else if matches!(ext, "webm" | "mkv" | "mp4" | "m4b" | "av1" | "avi" | "avif" | "flv" | "wmv" | "m4v" | "mpeg" | "mov") {
                        self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Vid, thumbsize, icons.clone()).await;
                        if let Some(_) = self.thumb_handle {
                            self.vid = true;
                            self.ftype = FType::Image;
                        } else {
                            self.thumb_handle = Some(icons.error.clone());
                            self.ftype = FType::File;
                        }
                    // 4. jxl and ico — image thumbnails
                    } else if matches!(ext, "jxl" | "ico") {
                        self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Norm, thumbsize, icons.clone()).await;
                        if let Some(_) = self.thumb_handle {
                            self.ftype = FType::Image;
                        } else {
                            self.thumb_handle = Some(icons.error.clone());
                            self.ftype = FType::File;
                        }
                    // 5. PDF — themed icon or thumbnail fallback
                    } else if ext == "pdf" {
                        self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Pdf, thumbsize, icons.clone()).await
                            .or(Some(icons.pdf_icon.clone()));
                        self.ftype = if icons.cando_pdf { FType::PdfEpub } else { FType::File };
                    // 6. EPUB — themed icon or thumbnail fallback
                    } else if ext == "epub" {
                        self.thumb_handle = self.prepare_cached_thumbnail(self.path.as_str(), ImgType::Epub, thumbsize, icons.clone()).await
                            .or(Some(icons.epub_icon.clone()));
                        self.ftype = if icons.cando_epub { FType::PdfEpub } else { FType::File };
                    // 7. Audio files (MIME-based or extension bucket)
                    } else if is_audio || matches!(theme::bucket_for_ext(ext), Some(theme::GenericBucket::Audio)) {
                        self.thumb_handle = Some(icons.audio.clone());
                        self.ftype = FType::File;
                    // 8. Generic document bucket
                    } else if matches!(theme::bucket_for_ext(ext), Some(theme::GenericBucket::Document)) {
                        self.thumb_handle = Some(icons.doc.clone());
                        self.ftype = FType::File;
                    // 9. Catch-all: pick icon by MIME category
                    } else if mime_type.starts_with("application/") && (
                        mime_type.contains("zip") || mime_type.contains("tar")
                        || mime_type.contains("gzip") || mime_type.contains("compress")
                        || mime_type.contains("7z") || mime_type.contains("rar")
                        || mime_type.contains("bzip") || mime_type.contains("xz")
                    ) {
                        self.thumb_handle = Some(icons.archive.clone());
                        self.ftype = FType::File;
                    } else if mime_type.starts_with("text/") && (
                        mime_type.contains("script") || mime_type.contains("x-")
                        || mime_type == "text/plain"
                        || mime_type.contains("csv")
                        || mime_type.contains("yaml")
                        || mime_type.contains("toml")
                        || mime_type.contains("json")
                        || mime_type.contains("xml")
                        || mime_type.contains("html")
                        || mime_type.contains("javascript")
                        || mime_type.contains("css")
                        || mime_type.contains("markdown")
                    ) {
                        self.thumb_handle = Some(icons.code.clone());
                        self.ftype = FType::File;
                    } else if mime_type.starts_with("video/") {
                        self.thumb_handle = Some(icons.video.clone());
                        self.ftype = FType::File;
                    } else if mime_type.starts_with("image/") {
                        self.thumb_handle = Some(icons.image.clone());
                        self.ftype = FType::File;
                    } else if mime_type.starts_with("audio/") {
                        self.thumb_handle = Some(icons.audio.clone());
                        self.ftype = FType::File;
                    } else if mime_type.starts_with("application/") {
                        // Generic application/octet-stream or other app types
                        // — use the generic document icon as a sensible default
                        self.thumb_handle = Some(icons.doc.clone());
                        self.ftype = FType::File;
                    } else {
                        self.thumb_handle = Some(icons.unknown.clone());
                        self.ftype = FType::File;
                    }
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

/// Try to find a working terminal emulator from a list of known terminals.
/// Returns None if no terminal is found.
fn find_terminal() -> Option<String> {
    let terminals = [
        "kitty",
        "alacritty",
        "foot",
        "wezterm",
        "st",
        "konsole",
        "terminator",
        "tilix",
        "xterm",
        "gnome-terminal",
        "xfce4-terminal",
        "urxvt",
    ];
    for term in &terminals {
        if std::process::Command::new("which").arg(term).output().map_or(false, |o| o.status.success()) {
            return Some(term.to_string());
        }
    }
    None
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

    fn run_command(self: &mut Self, icmd: usize) -> Task<Message> {
        let cmd = &self.conf.cmds[icmd];
        if cmd.builtin && cmd.label == "Terminal" {
            let cwd = self.dirs[0].clone();
            let cwd_path = PathBuf::from(&cwd);
            if self.conf.terminal.is_empty() {
                // Auto-detect terminal
                match find_terminal() {
                    Some(terminal_cmd) => {
                        tokio::task::spawn_blocking(move || {
                            let parts: Vec<&str> = terminal_cmd.split_whitespace().collect();
                            if parts.is_empty() { return; }
                            let mut cmd = OsCmd::new(parts[0]);
                            if parts.len() > 1 {
                                cmd.args(&parts[1..]);
                            }
                            cmd.arg("-e").arg("bash");
                            match cmd.current_dir(&cwd_path).spawn() {
                                Ok(_) => {},
                                Err(e) => eprintln!("Error opening terminal: {}", e),
                            }
                        });
                    },
                    None => {
                        self.modal = FModal::Error("No terminal emulator found. Set 'Terminal' in config.".into());
                    },
                }
            } else {
                // Use user-configured terminal command (run through bash)
                let filecmd = self.conf.terminal.clone();
                tokio::task::spawn_blocking(move || {
                    match OsCmd::new("bash").arg("-c").arg(filecmd).current_dir(&cwd_path).output() {
                        Ok(output) if !output.status.success() => eprintln!("{}{}",
                                    unsafe{std::str::from_utf8_unchecked(&output.stdout)},
                                    unsafe{std::str::from_utf8_unchecked(&output.stderr)}),
                        Ok(_) => {},
                        Err(e) => eprintln!("Error running command: {}", e)
                    };
                });
            }
            return Task::none();
        }
        if cmd.builtin && cmd.label == "Paste" {
            if self.dirs.len() != 1 {
                self.modal = FModal::Error("Cannot paste when multiple directories are open".into());
                return Task::none();
            }
            self.clipboard_paths.iter_mut().for_each(|path| {
                tokio::spawn(paste(mem::take(path), self.dirs[0].clone(), self.clipboard_cut));
            });
            self.clipboard_paths.clear();
            return Task::none();
        }
        let selected: Vec<&FItem> = self.items.iter().filter(|item| item.sel).collect();
        if !cmd.builtin && self.conf.command_confirmation && !selected.is_empty() && self.pending_cmd.is_none() {
            self.pending_cmd = Some(icmd);
            self.modal = FModal::CommandConfirm(cmd.label.clone());
            return Task::none();
        }
        if cmd.builtin && cmd.label == "Delete" && self.conf.delete_confirmation.need_confirm(false) {
            self.pending_delete_paths = selected.iter().map(|item| item.path.clone()).collect();
            self.modal = FModal::DeleteConfirm(self.pending_delete_paths.clone());
            return Task::none();
        }
        // Handle Rename before the loop so we can return a focus command
        if cmd.builtin && cmd.label == "Rename" {
            if let Some(item) = selected.first() {
                if self.modal == FModal::None {
                    self.new_path.basename = item.path.rsplitn(2, '/').next().unwrap().to_string();
                    self.new_path.full_path = item.path.clone();
                    self.modal = FModal::Rename(item.path.clone());
                    return iced::widget::operation::focus(self.rename_id.clone());
                } else {
                    self.modal = FModal::Error("Select only one file to rename".into());
                    return Task::none();
                }
            }
        }
        selected.into_iter().for_each(|item| {
            if cmd.builtin {
                match cmd.label.as_str() {
                    "Delete" => if let Err(e) = OsCmd::new("rm").arg("-rf").arg(&item.path).output() {
                        eprintln!("Error deleting {}: {}", item.path, e);
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
                        Ok(output) if !output.status.success() => eprintln!("{}{}",
                                                unsafe{std::str::from_utf8_unchecked(&output.stdout)},
                                                unsafe{std::str::from_utf8_unchecked(&output.stderr)}),
                        Ok(_) => {},
                        Err(e) => eprintln!("Error running command: {}", e)
                    };
                });
            }
        });
        Task::none()
    }

    fn keep_in_view(self: &mut Self, w: Rectangle, v: Rectangle) -> Task<Message> {
        let wbot = w.y + w.height;
        let abspos = if w.y < v.y {
            w.y
        } else if wbot > v.y + v.height {
           wbot - v.height
        } else { -1.0 };
        if abspos >= 0.0 {
            let offset = scrollable::AbsoluteOffset{x:0.0, y:abspos - self.content_y};
            self.update_scroll(offset.y);
            return iced::widget::operation::scroll_to(self.scroll_id.clone(), offset);
        }
        Task::none()
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
                        if self.conf.show_hidden || !ret.last().unwrap().hidden {
                            displayed.push(ret.len()-1);
                        }
                    });
                },
                Err(e) => eprintln!("Error reading dir {}: {}", dir, e),
            }
        }
        self.searchbar.clear();
        if let Some(ref ino) = self.ino_updater {
            let _ = ino.send(Inochan::NewDirs(inodirs));
        }
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
        if self.conf.keep_open {
            return;
        }
        self.conf.update(false);
        process::exit(0);
    }

    /// Build the settings menu items.
    fn build_settings_menu(&self) -> Vec<Item<'static, Message, iced::Theme, iced::Renderer>> {
        use iced_aw::menu::Item;
        vec![
            Item::new(menu_button(if self.conf.icon_view { "List View" } else { "Icon View" }, Message::ChangeView)),
            Item::new(menu_button("Sort A-Z", Message::Sort(1))),
            Item::new(menu_button("Sort Z-A", Message::Sort(2))),
            Item::new(menu_button("Sort Newest first", Message::Sort(3))),
            Item::new(menu_button("Sort Oldest first", Message::Sort(4))),
            Item::new(menu_button_checkbox("Show Hidden", self.conf.show_hidden, Message::ShowHidden(!self.conf.show_hidden))),
            Item::new(menu_button_checkbox("Recursive Search", self.recursive_search, Message::SetRecursive(!self.recursive_search))),
            Item::new(Element::<Message, iced::Theme, iced::Renderer>::from(text(format!("Thumbnail size:{}", self.conf.thumb_size)))),
            Item::new(Element::<Message, iced::Theme, iced::Renderer>::from(slider(50.0..=500.0, self.conf.thumb_size, Message::Thumbsize))),
            Item::new(menu_button("Themes & Fonts", Message::ToggleThemePane)),
        ]
    }

    /// Build the theme and font selection pane.
    fn build_theme_pane(&self) -> Element<'_, Message> {
        let font = self.font;
        let mut col = Column::new().padding(10.0).spacing(10.0);

        // Close button at top
        col = col.push(Button::new(Text::new("Close"))
            .on_press(Message::ToggleThemePane)
            .style(style::red_close_style())
            .width(Length::Fill)
            .padding(2.0));

        // Icon themes section
        let mut theme_col = Column::new().spacing(5.0);
        let mut theme_title = Text::new("Icon Themes").size(16);
        if let Some(f) = font { theme_title = theme_title.font(f); }
        theme_col = theme_col.push(container(theme_title)
            .padding(Padding { left: 5.0, ..Padding::ZERO }));
        theme_col = theme_col.push(rule::horizontal(2.0));
        let is_none = self.conf.icon_theme.as_deref() == Some("None") || self.conf.icon_theme.is_none();
        theme_col = theme_col.push(theme_button(is_none, "None"));
        let is_default = self.conf.icon_theme.as_deref() == Some("System default");
        theme_col = theme_col.push(theme_button(is_default, "System default"));
        
        if self.discovering_themes_and_fonts {
            let mut loading = Text::new("Loading...").size(13);
            if let Some(f) = font { loading = loading.font(f); }
            theme_col = theme_col.push(container(loading)
                .padding(Padding { left: 5.0, ..Padding::ZERO }));
        } else if let Some(themes) = &self.icon_themes {
            for theme_name in themes {
                let is_selected = self.conf.icon_theme.as_deref() == Some(theme_name.as_str());
                theme_col = theme_col.push(theme_button(is_selected, theme_name));
            }
        }
        col = col.push(theme_col);

        // Fonts section
        let mut font_col = Column::new().spacing(5.0);
        let mut font_title = Text::new("Fonts").size(16);
        if let Some(f) = font { font_title = font_title.font(f); }
        font_col = font_col.push(container(font_title)
            .padding(Padding { left: 5.0, ..Padding::ZERO }));
        font_col = font_col.push(rule::horizontal(2.0));
        
        let font_is_default = self.conf.font_name.is_none();
        font_col = font_col.push(font_button("System default", font_is_default, "System default"));
        
        if self.discovering_themes_and_fonts {
            let mut loading = Text::new("Loading...").size(13);
            if let Some(f) = font { loading = loading.font(f); }
            font_col = font_col.push(container(loading)
                .padding(Padding { left: 5.0, ..Padding::ZERO }));
        } else if let Some(fonts) = &self.font_names {
            for (name, _) in fonts {
                let is_selected = self.conf.font_name.as_deref() == Some(name.as_str());
                font_col = font_col.push(font_button(name, is_selected, name));
            }
        }
        col = col.push(font_col);

        // Close button at bottom
        col = col.push(Button::new(Text::new("Close"))
            .on_press(Message::ToggleThemePane)
            .style(style::red_close_style())
            .width(Length::Fill)
            .padding(2.0));

        Scrollable::new(col).into()
    }

    /// Build the preview pane showing thumbnails of selected files (list mode only).
    fn build_preview_pane(&self) -> Element<'_, Message> {
        let font = self.font;
        let mut col = Column::new().padding(10.0).spacing(10.0);
        let selected: Vec<_> = self.items.iter().filter(|item| item.sel).collect();
        if selected.is_empty() {
            let mut hint = Text::new("No files selected").size(13);
            if let Some(f) = font { hint = hint.font(f); }
            col = col.push(container(hint).padding(5.0));
        } else {
            for item in selected {
                let mut item_col = Column::new().spacing(2.0);
                // Thumbnail (full width of the pane)
                if let Some(h) = &item.thumb_handle {
                    item_col = item_col.push(
                        image(h.clone()).width(Length::Fill)
                    );
                }
                // Filename (owned String to avoid lifetime issues)
                let filename: String = item.path.rsplitn(2, '/').next().unwrap().to_string();
                let mut name_txt = Text::new(filename).size(12);
                if let Some(f) = font { name_txt = name_txt.font(f); }
                item_col = item_col.push(container(name_txt)
                    .padding(Padding { left: 5.0, ..Padding::ZERO }));
                col = col.push(item_col);
            }
        }
        Scrollable::new(col).into()
    }
}

async fn paste(path: String, dest: String, cut: bool) {
    let _ = tokio::process::Command::new(if cut { "mv" } else { "cp" })
        .arg(path).arg(dest).output().await;
}

impl Icons {
    fn new(thumbsize: f32, icon_theme: Option<String>) -> Icons {
        let home = std::env::var("HOME").unwrap();
        let tpath = Path::new(&home).join(".cache").join("pikeru").join("thumbnails");
        let cando_pdf = std::process::Command::new("which").arg("pdftoppm").output().map_or(false, |output| output.status.success());
        let cando_epub = std::process::Command::new("which").arg("epub-thumbnailer").output().map_or(false, |output| output.status.success());
        let theme_name = icon_theme.as_deref();

        // Try system icon themes first; fall back to bundled assets.
        let folder = Self::load_system_icon(
            theme::get_folder_icon_path(theme_name),
            include_bytes!("../assets/folder7.svg"),
            thumbsize,
        );
        let unknown = Self::load_system_icon(
            theme::get_unknown_icon_path(theme_name),
            include_bytes!("../assets/file6.svg"),
            thumbsize,
        );
        let doc = Self::load_system_icon(
            theme::get_document_icon_path(theme_name),
            include_bytes!("../assets/document.svg"),
            thumbsize,
        );
        let error = Self::load_system_icon(
            theme::get_error_icon_path(theme_name),
            include_bytes!("../assets/error.svg"),
            thumbsize,
        );
        let audio = Self::load_system_icon(
            theme::get_audio_icon_path(theme_name),
            include_bytes!("../assets/music4.svg"),
            thumbsize,
        );

        // Per-MIME-type icons — these are the new expanded file type support.
        // Each one tries themed icon first, then bundled fallback.
        let archive = Self::load_system_icon(
            theme::get_icon_path("package-x-generic", theme_name)
                .or_else(|| theme::get_icon_path("application-x-compressed", theme_name)),
            include_bytes!("../assets/archive.svg"),
            thumbsize,
        );
        let code = Self::load_system_icon(
            theme::get_icon_path("text-x-script", theme_name)
                .or_else(|| theme::get_icon_path("text-x-generic", theme_name)),
            include_bytes!("../assets/code-svgrepo-com.svg"),
            thumbsize,
        );
        let pdf_icon = Self::load_system_icon(
            theme::get_icon_path("application-pdf", theme_name)
                .or_else(|| theme::get_icon_path("x-office-document", theme_name)),
            include_bytes!("../assets/document.svg"),
            thumbsize,
        );
        let epub_icon = Self::load_system_icon(
            theme::get_icon_path("application-epub+zip", theme_name)
                .or_else(|| theme::get_icon_path("x-office-document", theme_name)),
            include_bytes!("../assets/document.svg"),
            thumbsize,
        );
        let video = Self::load_system_icon(
            theme::get_icon_path("video-x-generic", theme_name)
                .or_else(|| theme::get_icon_path("x-content-video", theme_name)),
            include_bytes!("../assets/video-file.svg"),
            thumbsize,
        );
        let image = Self::load_system_icon(
            theme::get_icon_path("image-x-generic", theme_name)
                .or_else(|| theme::get_icon_path("x-office-document", theme_name)),
            include_bytes!("../assets/image-file.svg"),
            thumbsize,
        );

        Icons {
            folder,
            unknown,
            doc,
            error,
            audio,
            archive,
            code,
            pdf_icon,
            epub_icon,
            video,
            image,
            thumb_dir: tpath.to_string_lossy().to_string(),
            settings: svg::Handle::from_memory(include_bytes!("../assets/settings2.svg")),
            updir: svg::Handle::from_memory(include_bytes!("../assets/up2.svg")),
            newdir: svg::Handle::from_memory(include_bytes!("../assets/newdir2.svg")),
            cmds: svg::Handle::from_memory(include_bytes!("../assets/cmd2.svg")),
            goto: svg::Handle::from_memory(include_bytes!("../assets/goto2.svg")),
            cando_pdf,
            cando_epub,
            theme_name: icon_theme,
        }
    }

    /// Load an icon from the system icon theme, falling back to a bundled asset.
    ///
    /// If the system icon is SVG, it is prerendered to pixels.
    /// If it is PNG (or XPM), it is decoded with the `image` crate.
    /// If no system icon is found, the bundled fallback bytes are prerendered as SVG.
    fn load_system_icon(
        system_icon: Option<(PathBuf, linicon::IconType)>,
        fallback_bytes: &[u8],
        thumbsize: f32,
    ) -> Handle {
        match system_icon {
            Some((path, icon_type)) => {
                match &icon_type {
                    linicon::IconType::SVG => {
                        let bytes = match std::fs::read(&path) {
                            Ok(b) => b,
                            Err(_) => return Self::prerender_svg(fallback_bytes, thumbsize),
                        };
                        Self::prerender_svg(&bytes, thumbsize)
                    }
                    linicon::IconType::PNG | linicon::IconType::XMP => {
                        // Decode the raster image and convert to a Handle.
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                match img::load_from_memory(&bytes) {
                                    Ok(img) => {
                                        let rgba = img.into_rgba8();
                                        let (w, h) = (rgba.width(), rgba.height());
                                        Handle::from_rgba(w, h, rgba.into_raw())
                                    }
                                    Err(_) => Self::prerender_svg(fallback_bytes, thumbsize)
                                }
                            }
                            Err(_) => Self::prerender_svg(fallback_bytes, thumbsize)
                        }
                    }
                }
            }
            None => Self::prerender_svg(fallback_bytes, thumbsize)
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
        Handle::from_rgba(w, h, pixels)
    }

    /// Look up the best icon for a given MIME type and extension from the
    /// selected theme. Returns `Some(handle)` if found, `None` to fall back
    /// to generic buckets.
    fn lookup_themed_icon(&self, mime: &str, ext: &str, thumbsize: u32) -> Option<Handle> {
        let theme_name = self.theme_name.as_deref();
        // Try MIME-based candidates (each with its own fallback chain)
        if let Some(candidates) = theme::mime_icon_candidates(mime) {
            for icon_name in &candidates {
                if let Some((path, icon_type)) = theme::get_icon_path(icon_name, theme_name) {
                    return Some(Self::load_system_icon(Some((path, icon_type)), include_bytes!("../assets/file6.svg"), thumbsize as f32));
                }
            }
        }

        // Secondary: extension-specific lookup
        let ext_icons: &[&str] = match ext {
            // Code — try language-specific first, fall back to generic script
            "rs" => &["text-x-rustsrc", "text-rust", "text-x-script"],
            "go" => &["text-x-go", "text-x-script"],
            "rb" => &["text-x-ruby", "text-x-script"],
            "java" => &["text-x-java", "text-x-script"],
            "php" => &["text-x-php", "text-x-script"],
            "c" | "h" | "cpp" | "hpp" | "cc" | "hh" | "cxx" | "hxx" => {
                &["text-x-c", "text-x-c++", "text-x-h"]
            }
            "cs" => &["text-x-csharp", "text-x-script"],
            "swift" => &["text-x-swift", "text-x-script"],
            "kt" | "kts" => &["text-x-kotlin", "text-x-script"],
            "lua" => &["text-x-lua", "text-x-script"],
            "zig" => &["text-x-zig", "text-x-script"],
            "nim" => &["text-x-nim", "text-x-script"],
            // Config / data formats
            "json" => &["application-json", "text-x-script"],
            "toml" => &["application-toml", "text-toml", "application-json"],
            "yaml" | "yml" => &["text-yaml", "text-x-script"],
            "ini" | "conf" | "cfg" => &["application-x-desktop", "x-office-document"],
            "csv" => &["text-csv", "application-vnd.oasis.opendocument.spreadsheet", "x-office-spreadsheet"],
            // Marked-up text
            "md" | "mkd" | "markdown" => &["text-x-markdown", "text-x-generic"],
            // Archives — try specific first, generic last
            "gz" => &["application-gzip", "package-x-generic"],
            "bz2" => &["application-x-bzip2", "package-x-generic"],
            "xz" | "lzma" => &["application/x-xz", "package-x-generic"],
            "tgz" | "tar.gz" => &["application-x-tar", "package-x-generic"],
            "7z" => &["application-x-7zip", "package-x-generic"],
            "rar" => &["application-x-rar", "package-x-generic"],
            // Video (non-thumbnailed)
            "3gp" => &["video-3gpp", "video-x-generic"],
            // Audio (non-thumbnailed)
            "mid" | "midi" => &["audio-midi", "audio-x-generic"],
            // Image (non-thumbnailed)
            "exr" => &["image-exr", "image-x-generic"],
            "psd" => &["image-psd", "image-x-generic"],
            "ai" => &["application-postscript", "image-x-generic"],
            // Misc
            "torrent" => &["application-x-bittorrent", "package-x-generic"],
            "vmdk" | "vdi" | "qcow2" => &["drive-harddisk", "package-x-generic"],
            "desktop" => &["application-x-desktop", "x-office-document"],
            "iso" => &["x-iso9660-image", "drive-harddisk", "media-optical"],
            "py" => &["text-x-python", "text-x-script"],
            // Fallback for unknown extensions — try a generic script icon
            _ => &[],
        };

        // Tertiary: extension-specific candidates
        for icon_name in ext_icons {
            if let Some((path, icon_type)) = theme::get_icon_path(icon_name, theme_name) {
                return Some(Self::load_system_icon(Some((path, icon_type)), include_bytes!("../assets/file6.svg"), thumbsize as f32));
            }
        }

        None
    }
}

impl Bookmark {
    fn new(label: &str, path: &str) -> Bookmark {
        Bookmark {
            label: label.into(),
            path: path.into(),
            id: WidgetId::unique(),
        }
    }
}

impl Cmd {
    fn new(label: &str, cmd: &str) -> Cmd {
        Cmd {
            label: label.into(),
            cmd: cmd.into(),
            builtin: false,
        }
    }

    fn builtin(label: &str) -> Cmd {
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


fn vid_frame(src: &str, thumbnail: Option<u32>, savepath: Option<&PathBuf>) -> Option<Handle> {
    let mut decoder = if let Some(thumbsize) = thumbnail {
        DecoderBuilder::new(Location::File(src.into()))
            .with_resize(Resize::Fit(thumbsize, thumbsize)).build().ok()?
    } else {
        Decoder::new(Location::File(src.into())).ok()?
    };
    let (w, h) = decoder.size_out();
    // Scan up to 30 frames for a non-black one
    let mut chosen_frame = None;
    let mut brightest = -1.0;
    for decoded in decoder.decode_iter().take(30) {
        if let Ok(frame) = decoded {
            let rgb = frame.1.slice(ndarray::s![.., .., ..]).to_slice()?;
            // Calculate average brightness
            let avg_brightness_x_3 = rgb.chunks_exact(3)
                .map(|pix| unsafe{*pix.get_unchecked(0)} as u32 +
                        unsafe{*pix.get_unchecked(1)} as u32 +
                        unsafe{*pix.get_unchecked(2)} as u32)
                .sum::<u32>() as f32 / (rgb.len() as f32 / 3.0);
            if avg_brightness_x_3 > 60.0 {
                chosen_frame = Some(frame);
                break;
            } else if avg_brightness_x_3 > brightest {
                brightest = avg_brightness_x_3;
                chosen_frame = Some(frame);
            }
        }
    }
    let frame = chosen_frame?;
    let rgb = frame.1.slice(ndarray::s![.., .., ..]).to_slice()?;
    let mut rgba = vec![255; rgb.len() * 4 / 3];
    for i in 0..rgb.len() / 3 {
        unsafe {
            let i3 = i * 3;
            let i4 = i * 4;
            *rgba.get_unchecked_mut(i4) = *rgb.get_unchecked(i3);
            *rgba.get_unchecked_mut(i4 + 1) = *rgb.get_unchecked(i3 + 1);
            *rgba.get_unchecked_mut(i4 + 2) = *rgb.get_unchecked(i3 + 2);
        }
    }
    if let Some(out) = savepath {
        let encoder = webp::Encoder::from_rgba(rgba.as_ref(), w, h);
        let wp = encoder.encode_simple(false, 50.0).unwrap();
        std::fs::write(out, &*wp).unwrap();
    }
    Some(Handle::from_rgba(w, h, rgba))
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
    Done(u8),
}

async fn recursive_add(mut updates: UReceiver<RecMsg>,
                       results: USender<RecMsg>,
                       selfy: USender<RecMsg>,
                       semchan: USender<SearchEvent>,
                       gitignore_txt: String,
                       respect_gitignore: bool,
                       do_index: bool) {
    let mut nav_id = 0;
    let mut recursive = true;
    let mut dirs = vec![];
    let mut ignores: Vec<Vec<Arc<gitignore::Gitignore>>> = vec![];
    let mut indexer = IndexProxy::new(do_index).await;
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
                    results.send(RecMsg::Done(nid)).unwrap();
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
                if dirs.is_empty() {
                    results.send(RecMsg::Done(nid)).unwrap();
                }
            },
            _ => {},
        };
    }
}

/// Event filter for iced::event::listen_with.
/// In iced 0.14, listen_with requires a fn pointer, not a closure.
fn pikeru_event_filter(evt: iced::Event, stat: Status, _window: iced::window::Id) -> Option<Message> {
    use iced::keyboard::key::Named;
    // Handle Escape to close modals regardless of widget focus
    if let iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Escape), .. }) = evt {
        return Some(Message::CloseModal);
    }
    if stat == Status::Ignored {
        match evt {
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(MouseButton::Back)) => Some(Message::UpDir),
            iced::Event::Mouse(iced::mouse::Event::ButtonPressed(MouseButton::Forward)) => Some(Message::DownDir),
            iced::Event::Mouse(iced::mouse::Event::WheelScrolled{ delta: ScrollDelta::Lines{ y, ..}}) => Some(Message::NextImage(if y<0.0 {1} else {-1})),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Enter), .. }) => Some(Message::Select(SelType::Click)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Shift), .. }) => Some(Message::Shift(true)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyReleased{ key: iced::keyboard::Key::Named(Named::Shift), .. }) => Some(Message::Shift(false)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Control), .. }) => Some(Message::Ctrl(true)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyReleased{ key: iced::keyboard::Key::Named(Named::Control), .. }) => Some(Message::Ctrl(false)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::ArrowUp), .. }) => Some(Message::ArrowKey(Named::ArrowUp)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::ArrowDown), .. }) => Some(Message::ArrowKey(Named::ArrowDown)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::ArrowLeft), .. }) => Some(Message::ArrowKey(Named::ArrowLeft)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::ArrowRight), .. }) => Some(Message::ArrowKey(Named::ArrowRight)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Backspace), .. }) => Some(Message::UpDir),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Delete), .. }) => Some(Message::Delete),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::PageUp), .. }) => Some(Message::PageUp),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::PageDown), .. }) => Some(Message::PageDown),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Tab), modifiers, .. }) if modifiers.contains(Modifiers::SHIFT) => Some(Message::CycleBookmarkBack),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Tab), .. }) => Some(Message::CycleBookmark),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Named(Named::Space), .. }) => Some(Message::Spacebar),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "h" => Some(Message::ArrowKey(Named::ArrowLeft)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "j" => Some(Message::ArrowKey(Named::ArrowDown)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "k" => Some(Message::ArrowKey(Named::ArrowUp)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "l" => Some(Message::ArrowKey(Named::ArrowRight)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "v" => Some(Message::ChangeView),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "i" => Some(Message::FocusFilepath),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "n" => Some(Message::NewDir(false)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "t" => Some(Message::RunCmd(5)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "y" => Some(Message::RunCmd(3)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "p" => Some(Message::RunCmd(4)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "s" || c.as_ref() == "/" => Some(Message::FocusSearch),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "1" => Some(Message::Sort(1)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "2" => Some(Message::Sort(2)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "3" => Some(Message::Sort(3)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "4" => Some(Message::Sort(4)),
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed{ key: iced::keyboard::Key::Character(ref c), .. }) if c.as_ref() == "q" => Some(Message::Cancel),
            _ => None,
        }
    } else { None }
}
