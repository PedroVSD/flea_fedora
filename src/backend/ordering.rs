use super::dirsize::{walk_all, walked_bytes, DirSize, SORT_BUDGET_MS};
use super::listing::Listing;
use super::meta::stat_all;
use super::mime::Db;
use super::sort::{name_order, parse_sort_by, sort_listing};
use std::cmp::Ordering;
use std::path::Path;
use std::time::{Duration, Instant};

// Sample input: {"c":"list","by":"name","desc":false,"foldersFirst":true,"groupByKind":false}
pub fn request(
    l: &mut Listing,
    base: &Path,
    mime: &Db,
    line: &str,
) -> Result<(f64, f64, Vec<Option<DirSize>>), &'static str> {
    let value = crate::jsondoc::parse(line).map_err(|_| "invalid ordering request")?;
    let default_by = if value.get("c").and_then(|v| v.as_str()) == Some("sort") { "" } else { "name" };
    let by = value.get("by").and_then(|v| v.as_str()).unwrap_or(default_by);
    let desc = value.get("desc").and_then(|v| v.as_bool()).unwrap_or(false);
    let folders = value
        .get("foldersFirst")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let groups = value
        .get("groupByKind")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    ordered(l, base, mime, by, desc, folders, groups)
}

// The default retains the shipped fast path; explicit grouping needs only filename MIME lookup.
pub fn ordered(
    l: &mut Listing,
    base: &Path,
    mime: &Db,
    by: &str,
    desc: bool,
    folders: bool,
    groups: bool,
) -> Result<(f64, f64, Vec<Option<DirSize>>), &'static str> {
    let by = if by == "date" { "mtime" } else { by };
    if !["name", "size", "mtime", "kind"].contains(&by) {
        return Err("no such sort key; send name, size, mtime or kind");
    }
    if by != "kind" && folders && !groups {
        return Ok(sort_listing(l, base, parse_sort_by(by)?, desc));
    }
    let (stats, mut pass_ms) = if by == "size" || by == "mtime" {
        let (stats, ms) = stat_all(base, l);
        (Some(stats), ms)
    } else {
        (None, 0.0)
    };
    // One shared deadline bounds the whole folder pass, so a size sort costs about one slow row.
    let walked: Vec<Option<DirSize>> = if by == "size" {
        let t = Instant::now();
        let walked = walk_all(base, l, t + Duration::from_millis(SORT_BUDGET_MS));
        pass_ms += t.elapsed().as_secs_f64() * 1000.0;
        walked
    } else {
        Vec::new()
    };
    let start = Instant::now();
    let kinds: Vec<&str> = if by == "kind" || groups {
        (0..l.len())
            .map(|i| {
                if l.is_dir(i) {
                    "inode/directory"
                } else {
                    mime.lookup(l.name(i)).unwrap_or("application/octet-stream")
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut indices: Vec<usize> = (0..l.len()).collect();
    indices.sort_by(|&a, &b| {
        let group = if groups {
            group_rank(l.is_dir(a), kinds[a]).cmp(&group_rank(l.is_dir(b), kinds[b]))
        } else if folders {
            l.is_dir(b).cmp(&l.is_dir(a))
        } else {
            Ordering::Equal
        };
        if group != Ordering::Equal {
            return group;
        }
        let key = match by {
            "kind" => kinds[a].cmp(kinds[b]),
            "size" => {
                let stats = stats.as_ref().unwrap();
                // A folder orders by its walked recursive size, the same number dirsized reports.
                let sa = if l.is_dir(a) { walked_bytes(&walked, a) } else { stats[a].size };
                let sb = if l.is_dir(b) { walked_bytes(&walked, b) } else { stats[b].size };
                sa.cmp(&sb)
            }
            "mtime" => {
                let stats = stats.as_ref().unwrap();
                stats[a].mtime.cmp(&stats[b].mtime)
            }
            _ => Ordering::Equal,
        };
        let order = key.then_with(|| name_order(l.name(a).as_bytes(), l.name(b).as_bytes()));
        if desc {
            order.reverse()
        } else {
            order
        }
    });
    l.spans = indices.iter().map(|&i| l.spans[i]).collect();
    // Final row order, so the caller seeds its answered-row cache instead of walking them again.
    let seed: Vec<Option<DirSize>> = if by == "size" {
        indices.iter().map(|&i| walked[i]).collect()
    } else {
        Vec::new()
    };
    Ok((pass_ms, start.elapsed().as_secs_f64() * 1000.0, seed))
}

fn group_rank(directory: bool, mime: &str) -> u8 {
    if directory {
        0
    } else if mime.starts_with("image/") {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn sort_requires_a_key_while_list_keeps_its_default() {
        let d = TestDir::new("sort-key");
        let mut listing = Listing::new();
        listing.push("z.txt", false);
        listing.push("a.txt", false);
        let db = Db::from_str("50:text/plain:*.txt\n");
        for line in [
            r#"{"c":"sort"}"#,
            r#"{"c":"sort","by":null}"#,
            r#"{"c":"sort","by":true}"#,
            r#"{"c":"sort","by":""}"#,
            r#"{"c":"sort","by":"mode"}"#,
            r#"{"c":"sort","by":"mode","foldersFirst":false}"#,
            r#"{"c":"sort","by":"mode","groupByKind":true}"#,
        ] {
            assert_eq!(request(&mut listing, d.path(), &db, line).unwrap_err(),
                "no such sort key; send name, size, mtime or kind");
            assert_eq!((listing.name(0), listing.name(1)), ("z.txt", "a.txt"),
                "a refused sort leaves the existing order intact: {}", line);
        }
        request(&mut listing, d.path(), &db, r#"{"c":"list"}"#).unwrap();
        assert_eq!((listing.name(0), listing.name(1)), ("a.txt", "z.txt"));
    }

    #[test]
    fn folder_toggle_and_grouping_change_real_order() {
        let d = TestDir::new("ordering");
        d.dir("z-folder");
        d.file("b.txt", "text");
        d.file("c.jpg", "photo");
        let mut l = Listing::new();
        l.push("z-folder", true);
        l.push("b.txt", false);
        l.push("c.jpg", false);
        let db = Db::from_str("50:image/jpeg:*.jpg\n50:text/plain:*.txt\n");
        ordered(&mut l, d.path(), &db, "name", false, false, false).unwrap();
        assert_eq!(l.name(0), "b.txt");
        ordered(&mut l, d.path(), &db, "name", false, true, true).unwrap();
        assert_eq!(l.name(0), "z-folder");
        assert_eq!(l.name(1), "c.jpg");
        ordered(&mut l, d.path(), &db, "kind", false, false, false).unwrap();
        assert_eq!(l.name(0), "c.jpg");
        assert!(ordered(&mut l, d.path(), &db, "bad", false, false, false).is_err());
    }

    #[test]
    fn an_anchor_answers_its_index_in_every_order() {
        let d = TestDir::new("ordering-anchor");
        d.file("b.txt", "xx");
        d.file("a.txt", "xxxx");
        d.file("c.jpg", "x");
        d.dir("z-folder");
        // Distinct mtimes with no clock dependence: touch -d is coreutils, always on a Linux box.
        for (name, date) in [("b.txt", "2020-01-01"), ("a.txt", "2020-01-02"), ("c.jpg", "2020-01-03"), ("z-folder", "2020-01-04")] {
            let status = std::process::Command::new("touch").args(["-d".to_string(), date.to_string(), d.join(name).to_string_lossy().into_owned()]).status().expect("touch sets the fixture mtime");
            assert!(status.success(), "touch -d {date} on {name}");
        }
        let db = Db::from_str("50:image/jpeg:*.jpg\n50:text/plain:*.txt\n");
        let pushed = || {
            let mut l = Listing::new();
            for (name, dir) in [("b.txt", false), ("a.txt", false), ("c.jpg", false), ("z-folder", true)] {
                l.push(name, dir);
            }
            l
        };
        // The sort arm's own composition: order first, then locate the anchor in the new order.
        let index_of = |l: &Listing, anchor: &str| l.index_of(d.path(), &d.path().join(anchor)).map(|index| index as isize).unwrap_or(-1);
        // (by, desc, folders, groups, anchor, want, and the order want is read off)
        for (by, desc, folders, groups, anchor, want, order) in [
            ("name", false, true, false, "a.txt", 1, "[z-folder, a.txt, b.txt, c.jpg]"),
            ("name", true, true, false, "a.txt", 3, "[z-folder, c.jpg, b.txt, a.txt]"),
            ("name", false, false, false, "a.txt", 0, "[a.txt, b.txt, c.jpg, z-folder]"),
            ("name", true, false, false, "z-folder", 0, "[z-folder, c.jpg, b.txt, a.txt]"),
            ("name", false, false, true, "a.txt", 2, "[z-folder, c.jpg, a.txt, b.txt]"),
            ("kind", false, true, false, "c.jpg", 1, "[z-folder, c.jpg, a.txt, b.txt]"),
            ("size", false, true, false, "b.txt", 2, "[z-folder, c.jpg, b.txt, a.txt]"),
            ("size", true, true, false, "b.txt", 2, "[z-folder, a.txt, b.txt, c.jpg]"),
            ("mtime", false, true, false, "a.txt", 2, "[z-folder, b.txt, a.txt, c.jpg]"),
            ("mtime", false, false, false, "a.txt", 1, "[b.txt, a.txt, c.jpg, z-folder]"),
        ] {
            let mut l = pushed();
            ordered(&mut l, d.path(), &db, by, desc, folders, groups).unwrap();
            assert_eq!(index_of(&l, anchor), want, "{by} desc={desc} folders={folders} groups={groups} should order {order}");
        }
        // Files alone by walked bytes, no folder to place: [c.jpg(1), b.txt(2), a.txt(4)].
        let mut l = Listing::new();
        for (name, dir) in [("b.txt", false), ("a.txt", false), ("c.jpg", false)] {
            l.push(name, dir);
        }
        ordered(&mut l, d.path(), &db, "size", false, false, false).unwrap();
        assert_eq!((l.name(0), l.name(1), l.name(2)), ("c.jpg", "b.txt", "a.txt"));
        assert_eq!(index_of(&l, "b.txt"), 1);
        // Gone or foreign anchors answer -1, never a row.
        let mut l = pushed();
        ordered(&mut l, d.path(), &db, "name", false, true, false).unwrap();
        assert_eq!(index_of(&l, "gone.txt"), -1);
        assert!(l.index_of(d.path(), std::path::Path::new("/elsewhere/a.txt")).is_none());
        assert!(l.index_of(d.path(), std::path::Path::new("relative/a.txt")).is_none());
        // A refused key stays refused with an anchor on it, and the order stands.
        let before = l.name(0).to_string();
        let line = format!(r#"{{"c":"sort","by":"mode","anchor":"{}"}}"#, d.join("a.txt").display());
        assert_eq!(request(&mut l, d.path(), &db, &line).unwrap_err(),
            "no such sort key; send name, size, mtime or kind");
        assert_eq!(l.name(0), before);
    }

    #[test]
    fn size_without_folders_first_interleaves_by_walked_size() {
        let d = TestDir::new("ordering-sizeflat");
        // The walk counts mid's own entry, one 4 KiB block on ext4 and a few bytes on btrfs or tmpfs, so big.bin outweighs either.
        d.file("big.bin", &"x".repeat(64 * 1024));
        d.file("small.bin", "12345");
        d.dir("mid");
        d.file("mid/inside", &"x".repeat(100));
        let mut l = Listing::new();
        l.push("big.bin", false);
        l.push("mid", true);
        l.push("small.bin", false);
        let db = Db::from_str("50:application/octet-stream:*.bin\n");
        let (_, _, seed) = ordered(&mut l, d.path(), &db, "size", false, false, false).unwrap();
        // A build still keying folders at 0 answers mid first; walked it sits between the files.
        assert_eq!((l.name(0), l.name(1), l.name(2)), ("small.bin", "mid", "big.bin"));
        assert_eq!(seed.len(), 3, "the seed arrives in final row order");
        assert!(seed[0].is_none() && seed[2].is_none(), "file rows seed nothing");
        let mid = seed[1].expect("the folder row carries its walked size");
        assert!(!mid.partial && mid.bytes > 100, "the seed is the whole walk, not a floor");
    }
}
