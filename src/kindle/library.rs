use std::path::Path;

// The document formats KOReader v2025.04 registers. Leaves out its image, code and generic archive types
pub const BOOK_EXTENSIONS: &[&str] = &[
    "azw", "cbt", "cbz", "chm", "djv", "djvu", "doc", "docm", "docx", "epub", "epub3", "fb2", "fb3",
    "htm", "html", "md", "mobi", "odt", "pdb", "pdf", "prc", "rtf", "tcr", "txt", "xhtml", "xps",
];

// Prints one tab-separated record per line, parsed by parse_device
pub(super) const DEVICE_SCRIPT: &str = r#"
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

#[derive(Debug, PartialEq)]
pub enum Progress {
    NotOpened,
    Unknown,
    Read(f32),
}

#[derive(Debug, PartialEq)]
pub struct Book {
    pub folder: String,
    pub file: String,
    pub title: String,
    pub author: String,
    pub size: u64,
    pub modified: u64,
    pub progress: Progress,
}

#[derive(Debug, Default, PartialEq)]
pub struct Device {
    pub books: Vec<Book>,
    pub folders: Vec<String>,
    pub battery: Option<u8>,
    pub used_kb: u64,
    pub free_kb: u64,
    pub koreader: String,
}

// The Kindle's storage is FAT-style, so this also rejects the characters it can't store
pub fn valid_folder_name(name: &str) -> Result<(), &'static str> {
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

pub fn is_book(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| BOOK_EXTENSIONS.iter().any(|b| ext.eq_ignore_ascii_case(b)))
}

pub(super) fn parse_device(output: &str) -> Device {
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
pub fn title_and_author(file: &str) -> (String, String) {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let (head, author) = match stem.split_once(" -- ") {
        Some((head, rest)) => (head, rest.split(" -- ").next().unwrap_or(rest)),
        None => stem.split_once(" - ").unwrap_or((stem, "")),
    };
    let title = head.split(" - ").next().unwrap_or(head);
    let title = title.split("_ ").next().unwrap_or(title);
    (title.trim().to_string(), author.trim().to_string())
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
}
