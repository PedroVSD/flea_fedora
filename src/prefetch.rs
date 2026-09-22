// The page cache prefetch behind the first window, see AGENTS.md "The first window": a cold launch is disk
// before it is anything else, so the launcher queues the reads the shell is about to make and gets on.
use std::fs;
use std::io::Read;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{mpsc, OnceLock};
use std::time::Duration;

// The launcher hands the shell the list's path in this variable, and the backend it spawns records into it.
pub const LIST_ENV: &str = "FLEA_PREFETCH";
const HEADER: &str = "flea-prefetch 2";
// A launch that never lists keeps the last list rather than recording whatever it did instead.
const RECORD_TIMEOUT_MS: u64 = 10_000;
// Bounds on a list this process did not write itself: ranges, one range's length, and the list's own size.
const MAX_RANGES: usize = 4096;
const MAX_RANGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LIST_BYTES: u64 = 1024 * 1024;
// posix_fadvise(2) POSIX_FADV_WILLNEED: queue the reads and return.
const POSIX_FADV_WILLNEED: i32 = 3;
// open(2) flags: a symlink at the last step is refused, and a FIFO swapped in after the check does not block.
const O_NONBLOCK: i32 = 0o4000;
const O_NOFOLLOW: i32 = 0o400000;
// sysconf(3) _SC_PAGESIZE, the same number on every Linux target the PKGBUILD names.
const SC_PAGESIZE: i32 = 30;
// pagemap(5): one little-endian u64 per page, bit 63 set when the page is present.
const PAGEMAP_ENTRY_BYTES: u64 = 8;
const PAGEMAP_PRESENT: u64 = 1 << 63;

// std already links the system libc, so the three symbols are declared here rather than taking a crate.
extern "C" {
    fn posix_fadvise(fd: i32, offset: i64, len: i64, advice: i32) -> i32;
    fn fork() -> i32;
    fn sysconf(name: i32) -> i64;
}

// One stretch of one file the shell had in memory: the unit a list is made of.
#[derive(Debug, PartialEq)]
struct Range {
    path: String,
    offset: u64,
    length: u64,
}

// $XDG_CACHE_HOME/flea/prefetch, or None when there is no home to put it in.
pub fn list_path() -> Option<PathBuf> {
    let cache = match crate::userfile::env_dir("XDG_CACHE_HOME") {
        Some(dir) => dir,
        None => crate::userfile::env_dir("HOME")?.join(".cache"),
    };
    Some(cache.join("flea/prefetch"))
}

// The launcher's side: a helper that queues the reads, waited for only as long as its fork takes.
pub fn warm(list: &Path) {
    if !list.is_file() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let spawned = Command::new(exe)
        .arg("--prefetch")
        .arg(list)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut child) = spawned {
        let _ = child.wait();
    }
}

// flea --prefetch <list>: forks so the launcher's wait returns at once and the shell it becomes never
// holds this as an unreaped child, then queues every read.
pub fn helper(list: &Path) -> i32 {
    // corner: a fork that fails keeps the work here, and the launcher waits it out, which is still a launch.
    if unsafe { fork() } > 0 {
        return 0;
    }
    let Some(text) = read_bounded(list) else { return 0 };
    let mut open: Option<(&str, Option<fs::File>)> = None;
    for range in parse_list(&text) {
        if open.as_ref().map(|(path, _)| *path) != Some(range.path) {
            open = Some((range.path, open_regular(range.path)));
        }
        if let Some((_, Some(file))) = &open {
            // The advice queues and returns; a refusal costs nothing but the read it would have saved.
            unsafe {
                posix_fadvise(file.as_raw_fd(), range.offset as i64, range.length as i64, POSIX_FADV_WILLNEED);
            }
        }
    }
    0
}

fn read_bounded(list: &Path) -> Option<String> {
    let file = fs::File::open(list).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_LIST_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_LIST_BYTES).read_to_string(&mut text).ok()?;
    Some(text)
}

// Checked before it is opened, because opening a device node can act on the device.
fn open_regular(path: &str) -> Option<fs::File> {
    if !fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let file = fs::OpenOptions::new().read(true).custom_flags(O_NOFOLLOW | O_NONBLOCK).open(path).ok()?;
    // Again on the open file, which is the one the advice goes to.
    file.metadata().ok()?.is_file().then_some(file)
}

struct ListRange<'a> {
    path: &'a str,
    offset: u64,
    length: u64,
}

// Sample input: "flea-prefetch 2\n0 32768 /usr/lib/libQt6Qml.so.6.11.2\n1048576 4096 /usr/share/fonts/a b.ttf\n"
fn parse_list(text: &str) -> Vec<ListRange<'_>> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    for line in lines {
        let mut fields = line.splitn(3, ' ');
        let (Some(offset), Some(length), Some(path)) = (fields.next(), fields.next(), fields.next()) else { continue };
        let (Ok(offset), Ok(length)) = (offset.parse::<u64>(), length.parse::<u64>()) else { continue };
        if !path.starts_with('/') || length == 0 || length > MAX_RANGE_BYTES {
            continue;
        }
        ranges.push(ListRange { path, offset, length });
        if ranges.len() == MAX_RANGES {
            break;
        }
    }
    ranges
}

// Set once by record_after_first_rows and fired by the list handler, so the loop needs no new parameter.
static FIRST_ROWS: OnceLock<mpsc::Sender<()>> = OnceLock::new();

// The backend's side: when it sends its first rows, the pages its parent shell has in memory become the
// next launch's list. Then and not later: by a second in, a media folder has decoded thumbnails and loaded
// every image plugin, and that list measured 13 to 47 ms slower on the next cold launch. Only the pages:
// whole files read three times the bytes and measured 40 ms slower.
pub fn record_after_first_rows() {
    let Some(list) = crate::userfile::env_dir(LIST_ENV) else { return };
    let parent = std::os::unix::process::parent_id();
    let (fired, first_rows) = mpsc::channel();
    if FIRST_ROWS.set(fired).is_err() {
        return;
    }
    std::thread::spawn(move || {
        if first_rows.recv_timeout(Duration::from_millis(RECORD_TIMEOUT_MS)).is_err() {
            return;
        }
        // A shell that exited handed this process to a reaper, whose pages say nothing about the shell.
        if std::os::unix::process::parent_id() != parent {
            return;
        }
        let Ok(maps) = fs::read_to_string(format!("/proc/{parent}/maps")) else { return };
        let Ok(pagemap) = fs::File::open(format!("/proc/{parent}/pagemap")) else { return };
        let page = page_size();
        let ranges = ranges_from(&maps, page, |start, count| present_pages(&pagemap, start, count, page));
        if !ranges.is_empty() {
            write_list(&list, &ranges);
        }
    });
}

// After every listing's rows: only the first call is heard, and a backend with no list to record hears none.
pub fn first_rows_sent() {
    if let Some(fired) = FIRST_ROWS.get() {
        let _ = fired.send(());
    }
}

fn page_size() -> u64 {
    let size = unsafe { sysconf(SC_PAGESIZE) };
    // corner: sysconf cannot fail for this name on Linux, and 4 KiB is the smallest page any target has.
    if size > 0 { size as u64 } else { 4096 }
}

// One bit per page of the mapping that starts at `start`, read from the shell's pagemap.
fn present_pages(pagemap: &fs::File, start: u64, count: u64, page: u64) -> Vec<bool> {
    let mut bytes = vec![0u8; (count * PAGEMAP_ENTRY_BYTES) as usize];
    if pagemap.read_exact_at(&mut bytes, start / page * PAGEMAP_ENTRY_BYTES).is_err() {
        return Vec::new();
    }
    bytes
        .chunks_exact(PAGEMAP_ENTRY_BYTES as usize)
        .map(|entry| u64::from_le_bytes(entry.try_into().unwrap_or_default()) & PAGEMAP_PRESENT != 0)
        .collect()
}

// Sample input, one maps line: "7f3c1e200000-7f3c1e5b1000 r--p 00001000 00:1f 1234      /usr/lib/libQt6Qml.so.6.11.2"
fn ranges_from(maps: &str, page: u64, mut present: impl FnMut(u64, u64) -> Vec<bool>) -> Vec<Range> {
    let max_pages = MAX_RANGE_BYTES / page;
    // Files in the order the shell first mapped them, each with the file pages it had.
    let mut files: Vec<(String, Vec<u64>)> = Vec::new();
    for line in maps.lines() {
        let mut fields = line.splitn(6, ' ');
        let (Some(span), _, Some(offset), _, _, Some(path)) =
            (fields.next(), fields.next(), fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let path = path.trim_start();
        if !path.starts_with('/') || path.ends_with(" (deleted)") {
            continue;
        }
        let Some((start, end)) = span.split_once('-') else { continue };
        let (Ok(start), Ok(end), Ok(offset)) =
            (u64::from_str_radix(start, 16), u64::from_str_radix(end, 16), u64::from_str_radix(offset, 16))
        else {
            continue;
        };
        let count = (end.saturating_sub(start) / page).min(max_pages);
        let first_page = offset / page;
        let pages: Vec<u64> = present(start, count)
            .iter()
            .enumerate()
            .filter(|(_, here)| **here)
            .map(|(i, _)| first_page + i as u64)
            .collect();
        match files.iter_mut().find(|(known, _)| known == path) {
            Some((_, known)) => known.extend(pages),
            None => files.push((path.to_string(), pages)),
        }
    }
    let mut ranges = Vec::new();
    for (path, mut pages) in files {
        pages.sort_unstable();
        pages.dedup();
        let mut run: Option<(u64, u64)> = None;
        for page_index in pages.into_iter().map(Some).chain(std::iter::once(None)) {
            match (run, page_index) {
                (Some((first, last)), Some(next)) if next == last + 1 => run = Some((first, next)),
                (Some((first, last)), next) => {
                    ranges.push(Range { path: path.clone(), offset: first * page, length: (last + 1 - first) * page });
                    run = next.map(|p| (p, p));
                }
                (None, next) => run = next.map(|p| (p, p)),
            }
        }
    }
    ranges.truncate(MAX_RANGES);
    ranges
}

// The write AGENTS.md "Predictable path writes" describes: our own temp file, created exclusively at 0600, then a rename.
fn write_list(list: &Path, ranges: &[Range]) {
    let Some(dir) = list.parent() else { return };
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    let mut text = String::from(HEADER);
    for range in ranges {
        text.push_str(&format!("\n{} {} {}", range.offset, range.length, range.path));
    }
    text.push('\n');
    let tmp = PathBuf::from(format!("{}.{}.tmp", list.display(), std::process::id()));
    let _ = fs::remove_file(&tmp);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))
        .and_then(|()| fs::rename(&tmp, list));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn only_present_file_pages_are_kept_in_first_mapped_order_and_merged() {
        let maps = "1000-4000 r--p 00000000 00:1f 1 /usr/lib/libQt6Qml.so.6\n\
                    4000-6000 r-xp 00003000 00:1f 1 /usr/lib/libQt6Qml.so.6\n\
                    6000-7000 rw-p 00000000 00:00 0 [heap]\n\
                    7000-8000 r--p 00000000 00:1f 2 /tmp/gone.so (deleted)\n\
                    8000-a000 r--p 00002000 00:1f 3                /usr/share/fonts/a b.ttf\n";
        let present = |start: u64, count: u64| -> Vec<bool> {
            match start {
                0x1000 => vec![true, true, false],
                0x4000 => vec![true, true],
                0x8000 => vec![false, true],
                _ => vec![true; count as usize],
            }
        };
        let ranges = ranges_from(maps, 0x1000, present);
        let want = vec![
            Range { path: "/usr/lib/libQt6Qml.so.6".into(), offset: 0, length: 0x2000 },
            Range { path: "/usr/lib/libQt6Qml.so.6".into(), offset: 0x3000, length: 0x2000 },
            Range { path: "/usr/share/fonts/a b.ttf".into(), offset: 0x3000, length: 0x1000 },
        ];
        assert_eq!(ranges, want);
    }

    #[test]
    fn a_list_needs_its_header_skips_bad_lines_and_is_capped() {
        assert!(parse_list("0 4096 /usr/lib/libc.so.6\n").is_empty());
        let text = format!("{HEADER}\n0 4096 /usr/lib/libc.so.6\nx 1 /a\n0 0 /b\n0 4096 relative\n8192 4096 /a b\n");
        let parsed: Vec<(&str, u64, u64)> = parse_list(&text).iter().map(|r| (r.path, r.offset, r.length)).collect();
        assert_eq!(parsed, vec![("/usr/lib/libc.so.6", 0, 4096), ("/a b", 8192, 4096)]);
        let mut long = String::from(HEADER);
        for i in 0..(MAX_RANGES + 10) {
            long.push_str(&format!("\n0 4096 /usr/lib/lib{i}.so"));
        }
        assert_eq!(parse_list(&long).len(), MAX_RANGES);
    }

    #[test]
    fn the_list_is_written_whole_private_and_read_back() {
        let dir = crate::backend::testdir::TestDir::new("prefetch-write");
        let list = dir.path().join("cache/flea/prefetch");
        write_list(&list, &[Range { path: "/usr/lib/libc.so.6".into(), offset: 0, length: 4096 }]);
        let text = read_bounded(&list).unwrap();
        let parsed: Vec<(&str, u64, u64)> = parse_list(&text).iter().map(|r| (r.path, r.offset, r.length)).collect();
        assert_eq!(parsed, vec![("/usr/lib/libc.so.6", 0, 4096)]);
        assert_eq!(fs::metadata(&list).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_dir(list.parent().unwrap()).unwrap().count(), 1, "no temp file is left behind");
    }

    #[test]
    fn only_a_regular_file_is_opened() {
        let dir = crate::backend::testdir::TestDir::new("prefetch-open");
        let file = dir.file("real.so", "x");
        let link = dir.join("link.so");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        let fifo = dir.join("fifo");
        assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        assert!(open_regular(file.to_str().unwrap()).is_some());
        assert!(open_regular(link.to_str().unwrap()).is_none(), "a symlink is refused");
        assert!(open_regular(fifo.to_str().unwrap()).is_none(), "a FIFO is refused without blocking");
        assert!(open_regular(dir.path().to_str().unwrap()).is_none(), "a directory is refused");
    }
}
