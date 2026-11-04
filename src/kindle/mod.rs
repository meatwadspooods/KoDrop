mod library;
mod remote;

pub use library::{BOOK_EXTENSIONS, Book, Device, Progress, is_book, title_and_author, valid_folder_name};
pub use remote::{create_folder, fetch_device, send};
