mod device_panel;
mod folders;
mod paint;
mod shelf;
mod theme;

use crate::kindle::{BOOK_EXTENSIONS, Book, Device, create_folder, fetch_device, is_book, send, title_and_author};
use crate::settings::{load_setting, save_setting};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};
use theme::{GROUND, install_fonts};

const REFRESH_EVERY: Duration = Duration::from_secs(30);

enum Connection {
    Connecting,
    Connected,
    Failed(String),
}

enum Status {
    Sending,
    Sent,
    Failed(String),
}

struct Item {
    name: String,
    folder: String,
    status: Status,
}

enum Message {
    SendDone(usize, Status),
    Refreshed(Result<Device, String>),
    FolderCreated(String, Result<(), String>),
}

pub struct KoDrop {
    host: String,
    // Relative to DESTINATION, empty means the books folder itself
    folder: String,
    new_folder: Option<String>,
    items: Vec<Item>,
    device: Option<Device>,
    connection: Connection,
    refreshing: bool,
    refresh_again: bool,
    last_refresh: Instant,
    messages: Receiver<Message>,
    messages_tx: Sender<Message>,
}

// macOS loads the picker on first use, which is most of the delay on the first click.
// Loading it at launch makes that click open it right away
fn warm_up_picker() {
    if let Some(main_thread) = objc2::MainThreadMarker::new() {
        let _ = objc2_app_kit::NSOpenPanel::openPanel(main_thread);
    }
}

impl KoDrop {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        install_fonts(&cc.egui_ctx);
        warm_up_picker();
        cc.egui_ctx.set_theme(egui::Theme::Light);
        let (messages_tx, messages) = channel();
        let mut app = Self {
            host: load_setting("host"),
            folder: load_setting("folder"),
            new_folder: None,
            items: Vec::new(),
            device: None,
            connection: Connection::Connecting,
            refreshing: false,
            refresh_again: false,
            last_refresh: Instant::now(),
            messages,
            messages_tx,
        };
        app.refresh(&cc.egui_ctx);
        app
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.last_refresh = Instant::now();
        let host = self.host.trim().to_string();
        if host.is_empty() {
            self.connection = Connection::Failed("Enter your Kindle's IP address".to_string());
            return;
        }
        self.refreshing = true;
        let tx = self.messages_tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(Message::Refreshed(fetch_device(&host)));
            ctx.request_repaint();
        });
    }

    fn visible_books(&self) -> Vec<&Book> {
        self.device
            .iter()
            .flat_map(|d| d.books.iter())
            .filter(|book| book.folder == self.folder)
            .collect()
    }

    fn select_folder(&mut self, folder: String) {
        save_setting("folder", &folder);
        self.folder = folder;
    }

    fn start_create_folder(&mut self, name: String, ctx: &egui::Context) {
        let host = self.host.trim().to_string();
        let tx = self.messages_tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = create_folder(&host, &name);
            let _ = tx.send(Message::FolderCreated(name, result));
            ctx.request_repaint();
        });
    }

    // Blocks this thread while the picker is open, sends already running keep going on their own
    fn open_picker(&mut self, ctx: &egui::Context) {
        let picked = rfd::FileDialog::new()
            .set_title("Choose books to send to your Kindle")
            .add_filter("KOReader books", BOOK_EXTENSIONS)
            .pick_files();
        for path in picked.into_iter().flatten() {
            self.add(path, ctx);
        }
    }

    fn add(&mut self, path: PathBuf, ctx: &egui::Context) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let host = self.host.trim().to_string();
        let folder = self.folder.clone();

        let refused = if host.is_empty() {
            Some("Enter your Kindle's IP address first")
        } else if !is_book(&path) {
            Some("Not a book format")
        } else {
            None
        };
        if let Some(reason) = refused {
            self.items.push(Item {
                name,
                folder,
                status: Status::Failed(reason.to_string()),
            });
            return;
        }

        let index = self.items.len();
        self.items.push(Item {
            name,
            folder: folder.clone(),
            status: Status::Sending,
        });
        let tx = self.messages_tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let status = match send(&path, &host, &folder) {
                Ok(()) => Status::Sent,
                Err(e) => Status::Failed(e),
            };
            let _ = tx.send(Message::SendDone(index, status));
            ctx.request_repaint();
        });
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.messages.try_recv() {
            match message {
                Message::SendDone(index, status) => {
                    let sent = matches!(status, Status::Sent);
                    self.items[index].status = status;
                    if sent {
                        self.refresh(ctx);
                    }
                }
                Message::Refreshed(result) => {
                    self.refreshing = false;
                    match result {
                        Ok(device) => {
                            // A queued refresh means this list may predate a folder just created
                            let gone = !self.folder.is_empty() && !device.folders.contains(&self.folder);
                            if gone && !self.refresh_again {
                                self.select_folder(String::new());
                            }
                            self.device = Some(device);
                            self.connection = Connection::Connected;
                        }
                        Err(reason) => self.connection = Connection::Failed(reason),
                    }
                    if std::mem::take(&mut self.refresh_again) {
                        self.refresh(ctx);
                    }
                }
                Message::FolderCreated(name, result) => match result {
                    Ok(()) => {
                        if let Some(device) = &mut self.device
                            && !device.folders.contains(&name)
                        {
                            device.folders.push(name.clone());
                            device.folders.sort_by_key(|folder| folder.to_lowercase());
                        }
                        self.select_folder(name);
                        self.refresh(ctx);
                    }
                    Err(reason) => self.items.push(Item {
                        name,
                        folder: String::new(),
                        status: Status::Failed(reason),
                    }),
                },
            }
        }
    }

    fn recent_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().rev().take(3)
    }
}

impl eframe::App for KoDrop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_messages(&ctx);
        if self.last_refresh.elapsed() >= REFRESH_EVERY {
            self.refresh(&ctx);
        }
        ctx.request_repaint_after(Duration::from_secs(1));

        let (hovering, hovered, dropped) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw.hovered_files.iter().filter_map(|f| f.path.clone()).collect::<Vec<_>>(),
                i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect::<Vec<_>>(),
            )
        });
        for path in dropped {
            self.add(path, &ctx);
        }

        // Runs before the menu that opens it, so the click on "New folder…" is over by the
        // time the dialog first appears. Otherwise that click would count as clicking outside
        self.draw_new_folder_dialog(&ctx);

        let frame = egui::Frame::NONE.fill(GROUND);
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            let full = ui.max_rect();
            let (left, right) = full.split_left_right_at_x(full.left() + 300.0);
            let incoming = hovered
                .iter()
                .find(|p| is_book(p))
                .and_then(|p| p.file_name())
                .map(|n| title_and_author(&n.to_string_lossy()).0);
            self.draw_device_panel(ui, left, incoming.as_deref());
            if hovering {
                self.draw_drop_target(ui, right, &hovered);
            } else {
                self.draw_shelf(ui, right);
            }
        });
    }
}
