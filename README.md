# KoDrop

A small macOS app that sends books to a Kindle running KOReader over Wi-Fi. Drag a file onto the window or click to pick one. It also lists the books on your Kindle with your reading progress.

Books go to `/mnt/us/books`, which KoDrop creates if needed.

## Setup

1. In KOReader, turn on Wi-Fi and the SSH server, using the default port 2222.
2. Add your public SSH key to `koreader/settings/SSH/authorized_keys` on the Kindle. If the key has a passphrase, add it to your macOS keychain with `ssh-add --apple-use-keychain`.
3. Build the app, which requires Rust:

   ```sh
   ./bundle.sh
   open target/release/KoDrop.app
   ```

4. Enter your Kindle's IP address, shown in KOReader under Network info. Allow local network access if macOS asks.

Keep the Kindle awake with KOReader open while you send books.
