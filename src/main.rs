use eframe::egui;
use egui::epaint::TextShape;
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

const PORT: &str = "2222";
const DESTINATION: &str = "/mnt/us/books/";
// The document formats KOReader v2025.04 registers. Leaves out its image, code and generic archive types
const BOOK_EXTENSIONS: &[&str] = &[
    "azw", "cbt", "cbz", "chm", "djv", "djvu", "doc", "docm", "docx", "epub", "epub3", "fb2", "fb3",
    "htm", "html", "md", "mobi", "odt", "pdb", "pdf", "prc", "rtf", "tcr", "txt", "xhtml", "xps",
];
const SSH_OPTIONS: &[&str] = &[
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=5",
    "-o",
    "StrictHostKeyChecking=accept-new",
    "-o",
    "LogLevel=ERROR",
];
const REFRESH_EVERY: Duration = Duration::from_secs(30);
// Keeps a lifted cover and its shadow from being clipped by the scroll area
const SHELF_MARGIN: f32 = 12.0;

// Prints one tab-separated record per line, parsed by parse_device
const DEVICE_SCRIPT: &str = r#"
printf 'battery\t%s\n' "$(gasgauge-info -c 2>/dev/null | tr -d '%')"
df -k /mnt/us | tail -n 1 | awk '{printf "storage\t%s\t%s\n",$3,$4}'
printf 'koreader\t%s\n' "$(cat /mnt/us/koreader/git-rev 2>/dev/null)"
root=/mnt/us/books
find "$root" \( -name '*.sdr' -o -name '.*' \) -prune -o -print 2>/dev/null | while IFS= read -r f; do
  rel="${f#$root}"
  rel="${rel#/}"
  if [ -d "$f" ]; then
    [ -n "$rel" ] && printf 'folder\t%s\n' "$rel"
    continue
  fi
  [ -f "$f" ] || continue
  sdr="${f%.*}.sdr"
  p=none
  if [ -d "$sdr" ]; then
    p=unknown
    v=$(grep -h percent_finished "$sdr"/metadata.*.lua 2>/dev/null | head -n 1 | sed 's/.*= *//; s/,.*//')
    [ -n "$v" ] && p=$v
  fi
  printf 'book\t%s\t%s\t%s\t%s\n' "$rel" "$(stat -c %s "$f")" "$(stat -c %Y "$f")" "$p"
done
"#;

const GROUND: Color32 = Color32::from_rgb(0xEC, 0xED, 0xE8);
const PANEL: Color32 = Color32::from_rgb(0xDF, 0xE1, 0xDB);
const LINE: Color32 = Color32::from_rgb(0xCD, 0xD0, 0xC8);
const INK: Color32 = Color32::from_rgb(0x16, 0x18, 0x1A);
const MUTED: Color32 = Color32::from_rgb(0x4A, 0x50, 0x58);
const FIELD: Color32 = Color32::from_rgb(0xF4, 0xF5, 0xF1);
const FIELD_LINE: Color32 = Color32::from_rgb(0xB4, 0xB7, 0xAE);
const TRACK: Color32 = Color32::from_rgb(0xC3, 0xC6, 0xBD);
const DASH: Color32 = Color32::from_rgb(0xA4, 0xA7, 0x9E);
const ACCENT: Color32 = Color32::from_rgb(0x1F, 0x47, 0xC4);
const ACCENT_DARK: Color32 = Color32::from_rgb(0x16, 0x34, 0x8F);
const DROP_GROUND: Color32 = Color32::from_rgb(0xE3, 0xE7, 0xF3);
const DROP_PRESSED: Color32 = Color32::from_rgb(0xD3, 0xDA, 0xEE);
const GOOD: Color32 = Color32::from_rgb(0x16, 0x62, 0x3F);
const BAD: Color32 = Color32::from_rgb(0xA8, 0x2A, 0x1E);
const BEZEL: Color32 = Color32::from_rgb(0x1B, 0x1D, 0x1F);
const EINK: Color32 = Color32::from_rgb(0xD9, 0xDA, 0xD2);
const EINK_LINE: Color32 = Color32::from_rgb(0xA9, 0xAB, 0xA3);
const COVERS: &[(Color32, Color32)] = &[
    (Color32::from_rgb(0x7A, 0x2E, 0x22), Color32::WHITE),
    (Color32::from_rgb(0xE2, 0xC6, 0x44), INK),
    (Color32::from_rgb(0x1F, 0x3A, 0x5F), Color32::WHITE),
    (Color32::from_rgb(0x27, 0x59, 0x4C), Color32::WHITE),
    (Color32::from_rgb(0x5A, 0x3B, 0x73), Color32::WHITE),
    (Color32::from_rgb(0xB9, 0x55, 0x2B), Color32::WHITE),
];

#[derive(Debug, PartialEq)]
enum Progress {
    NotOpened,
    Unknown,
    Read(f32),
}

#[derive(Debug, PartialEq)]
struct Book {
    folder: String,
    file: String,
    title: String,
    author: String,
    size: u64,
    modified: u64,
    progress: Progress,
}

#[derive(Debug, Default, PartialEq)]
struct Device {
    books: Vec<Book>,
    folders: Vec<String>,
    battery: Option<u8>,
    used_kb: u64,
    free_kb: u64,
    koreader: String,
}

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

struct KoDrop {
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

fn config_path(name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/kodrop").join(name))
}

fn load_setting(name: &str) -> String {
    config_path(name)
        .and_then(|path| fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

fn save_setting(name: &str, value: &str) {
    let Some(path) = config_path(name) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, value);
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn remote_dir(folder: &str) -> String {
    if folder.is_empty() {
        DESTINATION.to_string()
    } else {
        format!("{DESTINATION}{folder}/")
    }
}

fn folder_label(folder: &str) -> &str {
    if folder.is_empty() { "books" } else { folder }
}

// The Kindle's storage is FAT-style, so this also rejects the characters it can't store
fn valid_folder_name(name: &str) -> Result<(), &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a folder name");
    }
    if name.starts_with('.') {
        return Err("Folder names can't start with a dot");
    }
    if name.len() > 255 {
        return Err("That name is too long");
    }
    if name.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)) {
        return Err("Folder names can't contain / \\ : * ? \" < > |");
    }
    Ok(())
}

fn is_book(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| BOOK_EXTENSIONS.iter().any(|b| ext.eq_ignore_ascii_case(b)))
}

// Limits: the same filename overwrites and an interrupted copy can leave a partial file.
// Uploading to a temp name and renaming over ssh would fix both
fn send(path: &Path, host: &str, folder: &str) -> Result<(), String> {
    create_folder(host, folder)?;
    run(Command::new("/usr/bin/scp")
        .args(["-O", "-P", PORT])
        .args(SSH_OPTIONS)
        .arg(path)
        .arg(format!("root@{host}:{}", shell_quote(&remote_dir(folder)))))
}

fn create_folder(host: &str, folder: &str) -> Result<(), String> {
    run(Command::new("/usr/bin/ssh")
        .args(["-p", PORT])
        .args(SSH_OPTIONS)
        .arg(format!("root@{host}"))
        .arg(format!("mkdir -p {}", shell_quote(&remote_dir(folder)))))
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(error_message(&String::from_utf8_lossy(&output.stderr)))
}

fn fetch_device(host: &str) -> Result<Device, String> {
    let output = Command::new("/usr/bin/ssh")
        .args(["-p", PORT])
        .args(SSH_OPTIONS)
        .arg(format!("root@{host}"))
        .arg(DEVICE_SCRIPT)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(parse_device(&String::from_utf8_lossy(&output.stdout)));
    }
    Err(error_message(&String::from_utf8_lossy(&output.stderr)))
}

fn parse_device(output: &str) -> Device {
    let mut device = Device::default();
    for line in output.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["battery", value] => device.battery = value.trim().parse().ok(),
            ["storage", used, free] => {
                device.used_kb = used.parse().unwrap_or(0);
                device.free_kb = free.parse().unwrap_or(0);
            }
            ["koreader", version] => device.koreader = version.trim().to_string(),
            ["folder", path] => device.folders.push(path.to_string()),
            ["book", path, size, modified, progress] if is_book(Path::new(path)) => {
                let (folder, file) = path.rsplit_once('/').unwrap_or(("", path));
                let (title, author) = title_and_author(file);
                device.books.push(Book {
                    folder: folder.to_string(),
                    file: file.to_string(),
                    title,
                    author,
                    size: size.parse().unwrap_or(0),
                    modified: modified.parse().unwrap_or(0),
                    progress: match *progress {
                        "none" => Progress::NotOpened,
                        value => value.parse().map_or(Progress::Unknown, Progress::Read),
                    },
                });
            }
            _ => {}
        }
    }
    device.books.sort_by_key(|book| std::cmp::Reverse(book.modified));
    device.folders.sort_by_key(|folder| folder.to_lowercase());
    device
}

// Reads title and author from the filename. The EPUB's own metadata would be more accurate
fn title_and_author(file: &str) -> (String, String) {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let (head, author) = match stem.split_once(" -- ") {
        Some((head, rest)) => (head, rest.split(" -- ").next().unwrap_or(rest)),
        None => stem.split_once(" - ").unwrap_or((stem, "")),
    };
    let title = head.split(" - ").next().unwrap_or(head);
    let title = title.split("_ ").next().unwrap_or(title);
    (title.trim().to_string(), author.trim().to_string())
}

fn format_size(bytes: u64) -> String {
    match bytes {
        0..=999_999 => format!("{} KB", bytes / 1000),
        1_000_000..=999_999_999 => format!("{:.1} MB", bytes as f64 / 1e6),
        _ => format!("{:.1} GB", bytes as f64 / 1e9),
    }
}

fn format_free(kb: u64) -> String {
    let bytes = kb * 1024;
    if bytes < 1_000_000_000 {
        format!("{} MB free", bytes / 1_000_000)
    } else {
        format!("{:.1} GB free", bytes as f64 / 1e9)
    }
}

fn progress_label(progress: &Progress) -> String {
    match progress {
        Progress::NotOpened => "Not opened".to_string(),
        Progress::Unknown => "Progress unknown".to_string(),
        Progress::Read(p) if *p >= 1.0 => "Finished".to_string(),
        Progress::Read(p) => format!("{:.0}% read", p * 100.0),
    }
}

// scp ends with a generic "lost connection", the real cause is on the lines before it
fn error_message(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        "Connection failed".to_string()
    } else {
        lines.join(" - ")
    }
}

fn sans(size: f32) -> FontId {
    FontId::proportional(size)
}

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}

fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
    };
    add("plex-sans", include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf"));
    add("plex-bold", include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf"));
    add("plex-mono", include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"));
    add("plex-serif", include_bytes!("../assets/fonts/IBMPlexSerif-SemiBold.ttf"));

    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let with_fallback = |name: &str| {
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        list
    };
    let families = [
        (FontFamily::Proportional, with_fallback("plex-sans")),
        (FontFamily::Monospace, with_fallback("plex-mono")),
        (FontFamily::Name("bold".into()), with_fallback("plex-bold")),
        (FontFamily::Name("serif".into()), with_fallback("plex-serif")),
    ];
    fonts.families.extend(families);
    ctx.set_fonts(fonts);
}

// Draws text from its top-left corner, wrapped to width and cut off after max_rows
fn draw_text(
    painter: &egui::Painter,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    max_rows: usize,
) -> f32 {
    let galley = painter.layout_job(text_job(text, font, color, width, max_rows));
    let height = galley.size().y;
    painter.galley(pos, galley, color);
    height
}

fn text_job(text: &str, font: FontId, color: Color32, width: f32, max_rows: usize) -> LayoutJob {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, color));
    job.wrap = TextWrapping {
        max_width: width,
        max_rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job
}

fn draw_right_aligned(painter: &egui::Painter, right_top: Pos2, text: &str, font: FontId, color: Color32) {
    painter.text(right_top, egui::Align2::RIGHT_TOP, text, font, color);
}

fn draw_dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let points = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.add(Shape::dashed_line(&points, stroke, 8.0, 6.0));
}

fn draw_bar(painter: &egui::Painter, rect: Rect, fraction: f32, track: Color32, fill: Color32) {
    painter.rect_filled(rect, rect.height() / 2.0, track);
    if fraction > 0.0 {
        let mut filled = rect;
        filled.set_width((rect.width() * fraction.min(1.0)).max(rect.height()));
        painter.rect_filled(filled, rect.height() / 2.0, fill);
    }
}

fn draw_cover(painter: &egui::Painter, rect: Rect, book: &Book, index: usize) {
    let (title, author) = (&book.title, &book.author);
    let (background, foreground) = COVERS[index % COVERS.len()];
    painter.rect_filled(rect, 3.0, background);
    let inner = rect.shrink2(vec2(10.0, 14.0));
    draw_text(painter, inner.left_top(), title, serif(15.0), foreground, inner.width(), 5);
    let galley = painter.layout_job(text_job(author, serif(12.0), foreground, inner.width(), 2));
    let pos = pos2(inner.left(), inner.bottom() - galley.size().y);
    painter.galley(pos, galley, foreground);
}

// macOS loads the picker on first use, which is most of the delay on the first click.
// Loading it at launch makes that click open it right away
fn warm_up_picker() {
    if let Some(main_thread) = objc2::MainThreadMarker::new() {
        let _ = objc2_app_kit::NSOpenPanel::openPanel(main_thread);
    }
}

impl KoDrop {
    fn new(cc: &eframe::CreationContext) -> Self {
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

    fn draw_kindle(&self, painter: &egui::Painter, rect: Rect, incoming: Option<&str>) {
        painter.rect_filled(rect, 14.0, BEZEL);
        let screen = rect.shrink(12.0);
        painter.rect_filled(screen, 0.0, EINK);
        let inner = screen.shrink(10.0);

        let battery = self
            .device
            .as_ref()
            .and_then(|d| d.battery)
            .map_or(String::new(), |b| format!("{b}%"));
        let leaf = folder_label(&self.folder).rsplit('/').next().unwrap_or("books");
        draw_text(painter, inner.left_top(), leaf, mono(10.0), INK, inner.width() - 32.0, 1);
        draw_right_aligned(painter, inner.right_top(), &battery, mono(10.0), INK);
        let mut y = inner.top() + 17.0;
        painter.hline(inner.x_range(), y, Stroke::new(1.0, INK));
        y += 7.0;

        if let Some(title) = incoming {
            let height = 22.0;
            let row = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), height));
            painter.rect_filled(row, 0.0, ACCENT);
            let text_pos = pos2(row.left() + 6.0, y + 4.0);
            draw_text(painter, text_pos, title, serif(10.5), Color32::WHITE, row.width() - 12.0, 1);
            y += height + 7.0;
        }

        for book in self.visible_books() {
            if y + 15.0 > inner.bottom() {
                break;
            }
            draw_text(painter, pos2(inner.left(), y), &book.title, serif(10.5), INK, inner.width(), 1);
            y += 19.0;
            painter.hline(inner.x_range(), y, Stroke::new(1.0, EINK_LINE));
            y += 7.0;
        }
    }

    fn draw_device_panel(&mut self, ui: &mut egui::Ui, rect: Rect, incoming: Option<&str>) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, PANEL);
        painter.vline(rect.right(), rect.y_range(), Stroke::new(1.0, LINE));

        let inner = rect.shrink(24.0);
        let kindle = Rect::from_center_size(
            pos2(inner.center().x, inner.top() + 118.0),
            vec2(176.0, 236.0),
        );
        self.draw_kindle(&painter, kindle, incoming);

        let mut y = kindle.bottom() + 18.0;
        y += draw_text(&painter, pos2(inner.left(), y), "Kindle", bold(17.0), INK, inner.width(), 1);
        y += 6.0;
        let (label, color, reason) = match &self.connection {
            Connection::Connecting => ("Connecting…", MUTED, None),
            Connection::Connected => ("Connected", GOOD, None),
            Connection::Failed(reason) => ("Not connected", BAD, Some(reason.as_str())),
        };
        painter.circle_filled(pos2(inner.left() + 4.0, y + 9.0), 4.0, color);
        y += draw_text(&painter, pos2(inner.left() + 15.0, y), label, bold(13.0), color, inner.width(), 1);
        if let Some(reason) = reason {
            y += 4.0;
            y += draw_text(&painter, pos2(inner.left(), y), reason, sans(12.0), MUTED, inner.width(), 3);
        }
        y += 16.0;

        let device = self.device.as_ref();
        let free = device.map_or("Unknown".to_string(), |d| format_free(d.free_kb));
        let used = device.map_or(0.0, |d| {
            d.used_kb as f32 / (d.used_kb + d.free_kb).max(1) as f32
        });
        let row = |y: f32, label: &str, value: &str| {
            painter.text(pos2(inner.left(), y), egui::Align2::LEFT_TOP, label, sans(13.0), INK);
            draw_right_aligned(&painter, pos2(inner.right(), y + 1.0), value, mono(12.0), INK);
        };
        row(y, "Storage", &free);
        y += 25.0;
        let bar = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 6.0));
        draw_bar(&painter, bar, used, TRACK, INK);
        y += 22.0;
        let battery = device
            .and_then(|d| d.battery)
            .map_or("Unknown".to_string(), |b| format!("{b}%"));
        row(y, "Battery", &battery);
        y += 28.0;
        let koreader = device
            .map(|d| d.koreader.as_str())
            .filter(|v| !v.is_empty())
            .unwrap_or("Unknown");
        row(y, "KOReader", koreader);
        y += 28.0;

        painter.text(pos2(inner.left(), y + 5.0), egui::Align2::LEFT_TOP, "Address", sans(13.0), INK);
        let field_rect = Rect::from_min_size(pos2(inner.right() - 112.0, y), vec2(112.0, 28.0));
        let field = egui::TextEdit::singleline(&mut self.host)
            .font(mono(12.0))
            .horizontal_align(egui::Align::RIGHT)
            .vertical_align(egui::Align::Center)
            .hint_text("10.0.0.0");
        let response = ui
            .scope(|ui| {
                let visuals = ui.visuals_mut();
                visuals.extreme_bg_color = FIELD;
                visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
                visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
                ui.put(field_rect, field)
            })
            .inner
            .on_hover_text("Your Kindle's IP address");
        if response.changed() {
            save_setting("host", self.host.trim());
        }
        if response.lost_focus() {
            let ctx = ui.ctx().clone();
            self.refresh(&ctx);
        }
    }

    fn draw_activity(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        let mut y = rect.top();
        for item in self.recent_items() {
            let (dot, text) = match &item.status {
                Status::Sending => (ACCENT, format!("Sending {} to {}…", item.name, folder_label(&item.folder))),
                Status::Sent => (GOOD, format!("Sent {} to {}", item.name, folder_label(&item.folder))),
                Status::Failed(reason) => (BAD, format!("{}: {reason}", item.name)),
            };
            painter.circle_filled(pos2(rect.left() + 4.0, y + 9.0), 4.0, dot);
            y += draw_text(&painter, pos2(rect.left() + 15.0, y), &text, sans(13.0), INK, rect.width() - 15.0, 2);
            y += 6.0;
        }
    }

    fn recent_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().rev().take(3)
    }

    fn draw_shelf(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let inner = Rect::from_min_max(
            rect.min + vec2(28.0, 24.0),
            rect.max - vec2(28.0, 24.0),
        );
        let painter = ui.painter_at(rect);

        painter.text(inner.left_top(), egui::Align2::LEFT_TOP, "Books", bold(22.0), INK);
        let count = self.visible_books().len();
        let summary = match (&self.device, count) {
            (None, _) => "Not connected".to_string(),
            (_, 1) => "1 book".to_string(),
            _ => format!("{count} books"),
        };
        painter.text(pos2(inner.left() + 76.0, inner.top() + 9.0), egui::Align2::LEFT_TOP, summary, sans(13.0), MUTED);

        let button_rect = Rect::from_min_size(pos2(inner.right() - 78.0, inner.top()), vec2(78.0, 32.0));
        let label = if self.refreshing { "Refreshing" } else { "Refresh" };
        let hovered = ui.rect_contains_pointer(button_rect);
        let pressed = hovered && ui.input(|i| i.pointer.primary_down());
        let (fill, line, text) = match (hovered, pressed) {
            (true, true) => (DROP_GROUND, ACCENT_DARK, ACCENT_DARK),
            (true, false) => (Color32::WHITE, ACCENT, ACCENT_DARK),
            _ => (FIELD, FIELD_LINE, INK),
        };
        let button = egui::Button::new(egui::RichText::new(label).font(sans(13.0)).color(text))
            .fill(fill)
            .stroke(Stroke::new(1.0, line))
            .corner_radius(6.0);
        let response = ui.put(button_rect, button).on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            let ctx = ui.ctx().clone();
            self.refresh(&ctx);
        }

        let combo_rect = Rect::from_min_size(pos2(button_rect.left() - 152.0, inner.top()), vec2(140.0, 32.0));
        draw_right_aligned(&painter, pos2(combo_rect.left() - 8.0, inner.top() + 8.0), "Folder", sans(13.0), MUTED);
        self.draw_folder_menu(ui, combo_rect);

        let activity_height = self.recent_items().count() as f32 * 28.0;
        let shelf = Rect::from_min_max(
            pos2(inner.left() - SHELF_MARGIN, inner.top() + 50.0 - SHELF_MARGIN),
            pos2(inner.right() + SHELF_MARGIN, inner.bottom() - activity_height),
        );
        if activity_height > 0.0 {
            let activity = Rect::from_min_max(pos2(inner.left(), shelf.bottom() + 8.0), inner.right_bottom());
            self.draw_activity(ui, activity);
        }

        let choose = ui
            .scope_builder(egui::UiBuilder::new().max_rect(shelf), |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| self.draw_grid(ui, inner.width()))
                    .inner
            })
            .inner;
        if choose {
            let ctx = ui.ctx().clone();
            self.open_picker(&ctx);
        }
    }

    fn draw_folder_menu(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let mut folders = vec![String::new()];
        folders.extend(self.device.iter().flat_map(|d| d.folders.iter().cloned()));
        let current = self.folder.clone();
        let mut picked = None;
        let mut add_new = false;

        let text_color = if ui.rect_contains_pointer(rect) { ACCENT_DARK } else { INK };
        let selected = egui::RichText::new(folder_label(&current)).font(sans(13.0)).color(text_color);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.spacing_mut().interact_size.y = rect.height();
            let visuals = ui.visuals_mut();
            for (widget, fill, line) in [
                (&mut visuals.widgets.inactive, FIELD, FIELD_LINE),
                (&mut visuals.widgets.hovered, Color32::WHITE, ACCENT),
                (&mut visuals.widgets.active, DROP_GROUND, ACCENT_DARK),
                (&mut visuals.widgets.open, Color32::WHITE, ACCENT),
            ] {
                widget.weak_bg_fill = fill;
                widget.bg_stroke = Stroke::new(1.0, line);
                widget.corner_radius = 6.0.into();
                widget.expansion = 0.0;
            }

            let combo = egui::ComboBox::from_id_salt("folder")
                .selected_text(selected)
                .width(rect.width())
                .truncate()
                .show_ui(ui, |ui| {
                    let visuals = ui.visuals_mut();
                    visuals.widgets.hovered.weak_bg_fill = DROP_GROUND;
                    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                    visuals.selection.bg_fill = DROP_PRESSED;
                    visuals.selection.stroke = Stroke::new(1.0, ACCENT_DARK);
                    for folder in &folders {
                        let label = egui::RichText::new(folder_label(folder)).font(sans(13.0));
                        let item = ui
                            .selectable_label(*folder == current, label)
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if item.clicked() {
                            picked = Some(folder.clone());
                        }
                    }
                    ui.separator();
                    let label = egui::RichText::new("New folder…").font(sans(13.0)).color(ACCENT_DARK);
                    let item = ui
                        .selectable_label(false, label)
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    add_new = item.clicked();
                });
            combo.response.on_hover_cursor(egui::CursorIcon::PointingHand);
        });

        if let Some(folder) = picked {
            self.select_folder(folder);
        }
        if add_new {
            self.new_folder = Some(String::new());
            ui.ctx().request_repaint();
        }
    }

    fn draw_new_folder_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.new_folder.take() else { return };
        let mut create = false;
        let mut cancel = false;

        let dialog = egui::Modal::new(egui::Id::new("new-folder")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(egui::RichText::new("New folder").font(bold(17.0)).color(INK));
            ui.label(egui::RichText::new("It's created inside books on your Kindle").font(sans(13.0)).color(MUTED));

            let visuals = ui.visuals_mut();
            visuals.extreme_bg_color = FIELD;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
            let field = egui::TextEdit::singleline(&mut name)
                .font(sans(14.0))
                .desired_width(f32::INFINITY)
                .margin(vec2(8.0, 7.0))
                .hint_text("Folder name");
            ui.add(field).request_focus();

            let check = valid_folder_name(&name);
            let note = match check {
                Err(reason) if !name.trim().is_empty() => reason,
                _ => " ",
            };
            ui.label(egui::RichText::new(note).font(sans(12.0)).color(BAD));
            let valid = check.is_ok();
            create = valid && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().button_padding = vec2(14.0, 7.0);
                let visuals = ui.visuals_mut();
                for (widget, fill) in [
                    (&mut visuals.widgets.inactive, ACCENT),
                    (&mut visuals.widgets.hovered, ACCENT_DARK),
                    (&mut visuals.widgets.active, ACCENT_DARK),
                ] {
                    widget.weak_bg_fill = fill;
                    widget.bg_stroke = Stroke::NONE;
                    widget.expansion = 0.0;
                }
                let label = egui::RichText::new("Create").font(sans(13.0)).color(Color32::WHITE);
                let button = ui.add_enabled(valid, egui::Button::new(label));
                create |= button.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();

                let visuals = ui.visuals_mut();
                visuals.widgets.inactive.weak_bg_fill = FIELD;
                visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
                visuals.widgets.hovered.weak_bg_fill = Color32::WHITE;
                visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
                visuals.widgets.active.weak_bg_fill = DROP_GROUND;
                visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_DARK);
                let label = egui::RichText::new("Cancel").font(sans(13.0)).color(INK);
                let button = ui.add(egui::Button::new(label));
                cancel = button.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
            });
        });

        if create {
            self.start_create_folder(name.trim().to_string(), ctx);
        } else if !cancel && !dialog.should_close() {
            self.new_folder = Some(name);
        }
    }

    // Returns true when the drop tile is clicked
    fn draw_grid(&self, ui: &mut egui::Ui, width: f32) -> bool {
        const GAP: f32 = 20.0;
        const COVER_HEIGHT: f32 = 200.0;
        const ROW_HEIGHT: f32 = COVER_HEIGHT + 42.0;
        const MIN_CARD_WIDTH: f32 = 140.0;
        let columns = (((width + GAP) / (MIN_CARD_WIDTH + GAP)).floor() as usize).max(1);
        let card_width = (width - GAP * (columns - 1) as f32) / columns as f32;
        let books = self.visible_books();

        let leftover = columns - books.len() % columns;
        let tile_on_new_row = leftover == columns && !books.is_empty();
        let rows = books.len().div_ceil(columns) + usize::from(tile_on_new_row || books.is_empty());
        let height = rows as f32 * (ROW_HEIGHT + GAP);
        let size = vec2(width + SHELF_MARGIN, height + SHELF_MARGIN);
        let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();

        let cell = |index: usize| {
            pos2(
                area.left() + SHELF_MARGIN + (index % columns) as f32 * (card_width + GAP),
                area.top() + SHELF_MARGIN + (index / columns) as f32 * (ROW_HEIGHT + GAP),
            )
        };
        for (index, book) in books.iter().enumerate() {
            let top_left = cell(index);
            let card = Rect::from_min_size(top_left, vec2(card_width, ROW_HEIGHT));
            let response = ui
                .interact(card, ui.id().with(&book.file), egui::Sense::hover())
                .on_hover_text(&book.file);
            let lift = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);

            let resting = Rect::from_min_size(top_left, vec2(card_width, COVER_HEIGHT));
            let cover = resting.translate(vec2(0.0, -5.0 * lift));
            if lift > 0.0 {
                let shadow = egui::Shadow {
                    offset: [0, 8],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha((70.0 * lift) as u8),
                };
                painter.add(shadow.as_shape(cover, 3.0));
            }
            draw_cover(painter, cover, book, index);

            let fraction = match book.progress {
                Progress::Read(p) => p,
                _ => 0.0,
            };
            let bar = Rect::from_min_size(pos2(resting.left(), resting.bottom() + 8.0), vec2(card_width, 4.0));
            draw_bar(painter, bar, fraction, LINE, ACCENT);

            let y = bar.bottom() + 8.0;
            let size = format_size(book.size);
            let label_color = if response.hovered() { INK } else { MUTED };
            painter.text(pos2(resting.left(), y), egui::Align2::LEFT_TOP, progress_label(&book.progress), sans(12.0), label_color);
            draw_right_aligned(painter, pos2(resting.right(), y), &size, sans(12.0), label_color);
        }

        let span = if books.is_empty() || tile_on_new_row { columns } else { leftover };
        let tile = Rect::from_min_size(
            cell(books.len()),
            vec2(span as f32 * (card_width + GAP) - GAP, COVER_HEIGHT),
        );
        let response = ui
            .interact(tile, ui.id().with("choose-books"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Choose books to send")
        });
        let (hovered, pressed) = (response.hovered(), response.is_pointer_button_down_on());
        if pressed {
            painter.rect_filled(tile, 0.0, DROP_PRESSED);
        } else if hovered {
            painter.rect_filled(tile, 0.0, DROP_GROUND);
        }
        let (line, heading, detail) = if hovered {
            (ACCENT, ACCENT_DARK, ACCENT_DARK)
        } else {
            (DASH, INK, MUTED)
        };
        draw_dashed_rect(painter, tile, Stroke::new(2.0, line));
        let center = tile.center();
        painter.text(center - vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, "Drop books anywhere in this window", bold(15.0), heading);
        painter.text(center + vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, "or click to choose them from your Mac", sans(13.0), detail);
        response.clicked()
    }

    fn draw_drop_target(&self, ui: &egui::Ui, rect: Rect, hovered: &[PathBuf]) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, DROP_GROUND);
        draw_dashed_rect(&painter, rect.shrink(16.0), Stroke::new(3.0, ACCENT));

        let books: Vec<&PathBuf> = hovered.iter().filter(|p| is_book(p)).collect();
        let center = rect.center();
        // Finder only names dragged files on some drags, so an empty list still means "drop here"
        if !hovered.is_empty() && books.is_empty() {
            painter.text(center - vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, "That isn't a book file", bold(22.0), BAD);
            painter.text(center + vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, "KoDrop only sends files KOReader can open, like EPUB, PDF and MOBI", sans(14.0), MUTED);
            return;
        }

        let file_name = |path: &PathBuf| {
            path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
        };
        let (title, author) = books
            .first()
            .map_or((String::new(), String::new()), |path| title_and_author(&file_name(path)));
        if !title.is_empty() {
            draw_ghost_cover(&painter, center - vec2(0.0, 100.0), &title, &author);
        }

        let folder = folder_label(&self.folder);
        let detail = match books.as_slice() {
            [] => format!("It goes to the {folder} folder"),
            [only] => format!("{} goes to the {folder} folder", file_name(only)),
            many => format!("{} books go to the {folder} folder", many.len()),
        };
        painter.text(center + vec2(0.0, 40.0), egui::Align2::CENTER_CENTER, "Release to send to your Kindle", bold(22.0), ACCENT_DARK);
        draw_centered(&painter, center + vec2(0.0, 66.0), &detail, sans(14.0), MUTED, rect.width() - 80.0);
    }
}

fn draw_centered(painter: &egui::Painter, top_center: Pos2, text: &str, font: FontId, color: Color32, width: f32) {
    let galley = painter.layout_job(text_job(text, font, color, width, 2));
    let pos = top_center - vec2(galley.size().x / 2.0, 0.0);
    painter.galley(pos, galley, color);
}

fn draw_ghost_cover(painter: &egui::Painter, center: Pos2, title: &str, author: &str) {
    const ANGLE: f32 = -0.07;
    const SIZE: egui::Vec2 = egui::vec2(156.0, 200.0);
    let turn = egui::emath::Rot2::from_angle(ANGLE);
    let corner = |x: f32, y: f32| center + turn * vec2(x, y);
    let (hw, hh) = (SIZE.x / 2.0, SIZE.y / 2.0);
    let (background, foreground) = COVERS[COVERS.len() - 1];
    painter.add(Shape::convex_polygon(
        vec![corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)],
        background,
        Stroke::NONE,
    ));

    let width = SIZE.x - 20.0;
    let title_galley = painter.layout_job(text_job(title, serif(15.0), foreground, width, 5));
    painter.add(Shape::Text(
        TextShape::new(corner(10.0 - hw, 14.0 - hh), title_galley, foreground).with_angle(ANGLE),
    ));
    let author_galley = painter.layout_job(text_job(author, serif(12.0), foreground, width, 2));
    let author_top = hh - 14.0 - author_galley.size().y;
    painter.add(Shape::Text(
        TextShape::new(corner(10.0 - hw, author_top), author_galley, foreground).with_angle(ANGLE),
    ));
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

// eframe replaces the bundle icon in the Dock with its own default unless it's given one.
// The icon is optional, a missing file gives empty data, which turns that default off
fn load_icon(path: &Path) -> egui::IconData {
    fs::read(path)
        .ok()
        .and_then(|png| eframe::icon_data::from_png_bytes(&png).ok())
        .unwrap_or_default()
}

fn app_icon() -> egui::IconData {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/KoDrop-icon.png")));
    bundled.map_or_else(egui::IconData::default, |path| load_icon(&path))
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([760.0, 720.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "KoDrop",
        options,
        Box::new(|cc| Ok(Box::new(KoDrop::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_books_by_extension() {
        assert!(is_book(Path::new("/tmp/Book.EPUB")));
        assert!(!is_book(Path::new("/tmp/movie.mp4")));
        assert!(!is_book(Path::new("/tmp/no_extension")));
        assert!(is_book(Path::new("/tmp/report.xps")));
        assert!(!is_book(Path::new("/tmp/kf8.azw3")));
        assert!(!is_book(Path::new("/tmp/comic.cbr")));
    }

    #[test]
    fn reports_failure_when_nothing_is_listening() {
        let error = send(Path::new("/etc/hosts"), "127.0.0.1", "").unwrap_err();
        assert!(error.contains("Connection refused"), "{error}");
    }

    #[test]
    fn quotes_remote_paths() {
        assert_eq!(shell_quote("/mnt/us/books/"), "'/mnt/us/books/'");
        assert_eq!(shell_quote("My Book's; rm -rf x"), "'My Book'\\''s; rm -rf x'");
        assert_eq!(remote_dir(""), "/mnt/us/books/");
        assert_eq!(remote_dir("Fiction/Classics"), "/mnt/us/books/Fiction/Classics/");
    }

    #[test]
    fn checks_folder_names() {
        for good in ["Fiction", "Sci-Fi & Fantasy", "Adam's Books", "  Trimmed  "] {
            assert!(valid_folder_name(good).is_ok(), "{good}");
        }
        for bad in ["", "   ", "../x", "a/b", ".hidden", "a:b", "a\\b", "a\nb"] {
            assert!(valid_folder_name(bad).is_err(), "{bad}");
        }
        assert!(valid_folder_name(&"x".repeat(256)).is_err());
    }

    #[test]
    fn sorts_books_into_folders() {
        let output = "folder\tzebra\nfolder\tFiction\nfolder\tFiction/Classics\n\
            book\tRoot - A.epub\t10\t1\tnone\n\
            book\tFiction/Middle - B.epub\t10\t2\tnone\n\
            book\tFiction/Classics/Deep - C.pdf\t10\t3\tnone\n\
            book\tFiction/notes.lua\t10\t4\tnone\n";
        let device = parse_device(output);
        assert_eq!(device.folders, ["Fiction", "Fiction/Classics", "zebra"]);
        let placed: Vec<(&str, &str)> = device
            .books
            .iter()
            .map(|b| (b.folder.as_str(), b.file.as_str()))
            .collect();
        assert_eq!(
            placed,
            [
                ("Fiction/Classics", "Deep - C.pdf"),
                ("Fiction", "Middle - B.epub"),
                ("", "Root - A.epub"),
            ]
        );
    }

    #[test]
    fn keeps_the_cause_ahead_of_lost_connection() {
        let stderr = "ssh: connect to host x port 2222: Connection refused\r\nlost connection\r\n";
        assert!(error_message(stderr).starts_with("ssh: connect to host"));
        assert_eq!(error_message(""), "Connection failed");
    }

    #[test]
    fn splits_real_filenames_into_title_and_author() {
        let cases = [
            ("Enshittification_ Why Everything Suddenly Got Worse and What -- Cory Doctorow.epub", "Enshittification", "Cory Doctorow"),
            ("Technofeudalism _ What Killed Capitalism -- Yanis Varoufakis.epub", "Technofeudalism", "Yanis Varoufakis"),
            ("The Wretched of The Earth - Frantz Fanon.epub", "The Wretched of The Earth", "Frantz Fanon"),
            ("What Is Marxism - Alan Woods & Rob Sewell.epub", "What Is Marxism", "Alan Woods & Rob Sewell"),
            ("Layla and Majnun - The Classic Love Story of Persian -- Nizami -- 2011 -- Anna.epub", "Layla and Majnun", "Nizami"),
            ("my_book.epub", "my_book", ""),
        ];
        for (file, title, author) in cases {
            assert_eq!(title_and_author(file), (title.to_string(), author.to_string()), "{file}");
        }
    }

    #[test]
    fn parses_device_output() {
        let output = "battery\t96\nstorage\t611544\t781088\nkoreader\tv2025.04\n\
            book\tOld - A.epub\t538870\t100\t0.25\n\
            book\tNew - B.epub\t2413228\t200\tnone\n\
            book\tDone - C.epub\t1000\t50\t1\n\
            book\tOdd - D.epub\t1000\t40\tunknown\n\
            book\tnotes.lua\t10\t300\tnone\n\
            junk line\n";
        let device = parse_device(output);
        assert_eq!(device.battery, Some(96));
        assert_eq!(device.used_kb, 611544);
        assert_eq!(device.free_kb, 781088);
        assert_eq!(device.koreader, "v2025.04");
        let titles: Vec<&str> = device.books.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["New", "Old", "Done", "Odd"]);
        assert_eq!(device.books[0].progress, Progress::NotOpened);
        assert_eq!(device.books[1].progress, Progress::Read(0.25));
        assert_eq!(device.books[2].progress, Progress::Read(1.0));
        assert_eq!(device.books[3].progress, Progress::Unknown);
    }

    #[test]
    fn missing_icon_is_empty() {
        let icon = load_icon(Path::new("/nonexistent/KoDrop-icon.png"));
        assert_eq!((icon.width, icon.height), (0, 0));
    }

    #[test]
    fn formats_labels() {
        assert_eq!(format_size(2413228), "2.4 MB");
        assert_eq!(format_size(538870), "538 KB");
        assert_eq!(format_free(781088), "799 MB free");
        assert_eq!(progress_label(&Progress::Read(0.23705)), "24% read");
        assert_eq!(progress_label(&Progress::Read(1.0)), "Finished");
        assert_eq!(progress_label(&Progress::NotOpened), "Not opened");
    }
}
