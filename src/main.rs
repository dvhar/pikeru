use img::load_from_memory;
use num_cpus;
use iced::{
    color,
    alignment,
    executor,
    subscription,
    Application, Command, Length, Element,
    Background,
    theme::Container,
    widget::{
        container::{Appearance, StyleSheet},
        mouse_area, container,
        image, image::Handle, Column, Row, text, responsive,
        Scrollable, scrollable::{Direction,Properties},
        Button, TextInput,
        column, row,
    },
    futures::{
        channel::mpsc,
        sink::SinkExt,
        StreamExt,
    }
};
use tokio::{fs::File, io::AsyncReadExt};
use std::{
    path::PathBuf,
    mem,
    process,
};


fn main() -> iced::Result {
    FilePicker::run(iced::Settings::default())
}

#[derive(Debug, Clone)]
enum Message {
    LoadDir,
    Open,
    Cancel,
    Init(mpsc::Sender<Item>),
    NextItem(Item),
    ItemClick(usize),
    TxtInput(String),
}

struct FilePicker {
    items: Vec<Item>,
    dirs: Vec<String>,
    inputbar: String,
    thumb_sender: Option<mpsc::Sender<Item>>,
    nproc: usize,
    lastidx: usize,
}

enum SubState {
    Starting,
    Ready(mpsc::Receiver<Item>),
}

#[derive(Debug, Clone, Default)]
enum FType {
    File,
    Image,
    Dir,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Default)]
struct Item {
    path: String,
    ftype: FType,
    handle: Option<Handle>,
    idx: usize,
    sel: bool,
}


impl Application for FilePicker {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = iced::Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        (
            Self {
                items: Default::default(),
                thumb_sender: None,
                nproc: num_cpus::get(),
                dirs: vec![
                    "/home/d/sync/docs/pics".into(),
                ],
                lastidx: 0,
                inputbar: Default::default(),
            },
            Command::none(),
        )
    }

    fn theme(&self) -> iced::Theme {
        iced::Theme::Dark
    }

    fn title(&self) -> String {
        String::from("File Picker")
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::Init(chan) => {
                self.thumb_sender = Some(chan);
                return self.update(Message::LoadDir);
            },
            Message::NextItem(doneitem) => {
                self.lastidx += 1;
                if self.lastidx < self.items.len() {
                    let nextitem = mem::take(&mut self.items[self.lastidx]);
                    tokio::task::spawn(nextitem.load(self.thumb_sender.as_ref().unwrap().clone()));
                }
                let i = doneitem.idx;
                self.items[i] = doneitem;
            },
            Message::LoadDir => {
                self.inputbar = self.dirs[0].clone();
                self.items = ls(&self.dirs);
                self.lastidx = self.nproc.min(self.items.len());
                for i in 0..self.lastidx {
                    let item = mem::take(&mut self.items[i]);
                    tokio::task::spawn(item.load(self.thumb_sender.as_ref().unwrap().clone()));
                }
            },
            Message::ItemClick(idx) => {
                self.items[idx].sel = true;
            },
            Message::TxtInput(txt) => self.inputbar = txt,
            Message::Open => {
                self.items.iter().filter(|item| item.sel ).for_each(|item| println!("{}", item.path));
                process::exit(0);
            },
            Message::Cancel => process::exit(0),
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        let mut state = SubState::Starting;
        subscription::channel("", 100, |mut messager| async move {
            loop {
                match &mut state {
                    SubState::Starting => {
                        let (sender, receiver) = mpsc::channel(100);
                        messager.send(Message::Init(sender)).await.unwrap();
                        state = SubState::Ready(receiver);
                    }
                    SubState::Ready(thumb_receiver) => {
                        let item = thumb_receiver.select_next_some().await;
                        messager.send(Message::NextItem(item)).await.unwrap();
                    },
                }
            }
        })
    }

    fn view(&self) -> iced::Element<'_, Self::Message> {
        responsive(|size| {
            let maxcols = (size.width / 160.0).max(1.0) as usize;
            let num_rows = self.items.len() / maxcols + if self.items.len() % maxcols != 0 { 1 } else { 0 };
            let mut rows = Column::new();
            for i in 0..num_rows {
                let start = i * maxcols;
                let mut row = Row::new();
                for j in 0..maxcols {
                    let idx = start + j;
                    if idx < self.items.len() {
                        row = row.push(unsafe{self.items.get_unchecked(idx)}.display());
                    }
                }
                rows = rows.push(row);
            }
            let ctrlbar = column![
                row![
                    top_button("Cmd", 80.0, Message::Cancel),
                    top_button("View", 80.0, Message::Cancel),
                    top_button("New Dir", 80.0, Message::Cancel),
                    top_button("Up Dir", 80.0, Message::Cancel),
                    top_button("Cancel", 100.0, Message::Cancel),
                    top_button("Open", 100.0, Message::Open)
                ].spacing(2),
                TextInput::new("directory or file path", self.inputbar.as_str())
                    .on_input(Message::TxtInput)
                    .on_paste(Message::TxtInput),
            ].align_items(iced::Alignment::End).width(Length::Fill);
            let content = Scrollable::new(rows)
                .width(Length::Fill)
                .height(Length::Fill)
                .direction(Direction::Vertical(Properties::new()));
            column![
                ctrlbar,
                content
            ].into()
        }).into()
    }
}

fn top_button(txt: &str, size: f32, msg: Message) -> Element<'static, Message> {
    Button::new(text(txt)
                .width(size)
                .horizontal_alignment(alignment::Horizontal::Center))
        .on_press(msg).into()
}

impl Item {

    fn display(&self) -> Element<'static, Message> {
        let mut col = Column::new()
            .align_items(iced::Alignment::Center)
            .width(160);
        if let Some(h) = &self.handle {
            col = col.push(image(h.clone()));
        }
        let mut label = self.path.rsplitn(2,'/').next().unwrap();
        col = if self.path.len() > 16 {
            label = &label[(label.len().max(16)-16)..label.len()];
            let mut shortened = ['.' as u8; 19];
            shortened[3..3+label.len()].copy_from_slice(label.as_bytes());
            col.push(text(unsafe{std::str::from_utf8_unchecked(&shortened)})).into()
        } else {
            col.push(text(label)).into()
        };
        let clickable = if self.sel {
            mouse_area(container(col).style(get_sel_theme()))
        } else {
            mouse_area(col)
        };
        clickable.on_press(Message::ItemClick(self.idx))
            .on_right_press(Message::ItemClick(self.idx))
            .on_middle_press(Message::ItemClick(self.idx))
            .into()
    }

    fn new(pth: PathBuf, idx: usize) -> Self {
        let ftype = if pth.is_dir() {
            FType::Dir
        } else {
            FType::Unknown
        };
        Item {
            path: pth.to_string_lossy().to_string(),
            ftype,
            idx,
            handle: None,
            sel: false,
        }
    }

    async fn load(mut self, mut chan: mpsc::Sender<Item>) {
        match self.ftype {
            FType::Dir => {},
            _ => {
                let ext = match self.path.rsplitn(2,'.').next() {
                    Some(s) => s,
                    None => "",
                };
                self.ftype = match ext.to_lowercase().as_str() {
                    "png"|"jpg"|"jpeg"|"bmp"|"tiff"|"gif"|"webp" => {
                        let mut file = File::open(self.path.as_str()).await.unwrap();
                        let mut buffer = Vec::new();
                        file.read_to_end(&mut buffer).await.unwrap();
                        let img = load_from_memory(buffer.as_ref()).unwrap();
                        let thumb = img.thumbnail(160, 160);
                        let (w,h,rgba) = (thumb.width(), thumb.height(), thumb.into_rgba8());
                        let pixels = rgba.as_raw();
                        self.handle = Some(Handle::from_pixels(w, h, pixels.clone()));
                        FType::Image
                    },
                    _ => FType::File
                };
            }
        }
        chan.send(self).await.unwrap();
    }
}

fn ls(dirs: &Vec<String>) -> Vec<Item> {
    let mut ret = vec![];
    for dir in dirs {
        let list = std::fs::read_dir(dir.as_str()).unwrap();
        list.for_each(|f| {
            let path = f.unwrap().path();
            let idx = ret.len();
            ret.push(Item::new(path, idx));
        });
    }
    ret
}

pub struct SelectedTheme;
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
pub fn get_sel_theme() -> Container {
    Container::Custom(
        Box::new(SelectedTheme) as Box<dyn StyleSheet<Style = iced::Theme>>
    )
}

