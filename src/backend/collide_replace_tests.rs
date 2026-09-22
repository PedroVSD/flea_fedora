// Replace, its undo and its redo, against a stand-in trash inside the sandbox.
use super::tests::{asked, chosen, refusing_trash, text, transfer, StandIn};
use super::*;
use crate::backend::testdir::TestDir;
use crate::backend::undo::Journal;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;

// Numbers the stand-in's entries, so two items trashed from one path never share a URI.
static NEXT_ENTRY: AtomicUsize = AtomicUsize::new(0);
const URI: &str = "trash:///";
const ORIGINAL: &str = ".original";

fn exited(code: i32, stdout: String) -> Output {
    Output { status: ExitStatus::from_raw(code << 8), stdout: stdout.into_bytes(), stderr: Vec::new() }
}

// gio's trash, played by a folder inside the sandbox: entry n sits at can/n with its original path beside it.
fn gio_trash(can: &Path, args: &[&str]) -> Output {
    match args {
        ["trash", "--list"] => {
            let mut listed = String::new();
            for item in std::fs::read_dir(can).unwrap().flatten() {
                let name = item.file_name().to_string_lossy().to_string();
                if let Some(n) = name.strip_suffix(ORIGINAL) {
                    listed.push_str(&format!("{}{}\t{}\n", URI, n, std::fs::read_to_string(item.path()).unwrap()));
                }
            }
            exited(0, listed)
        }
        ["trash", "--restore", uri] => {
            let n = uri.strip_prefix(URI).unwrap();
            let original = PathBuf::from(std::fs::read_to_string(can.join(format!("{}{}", n, ORIGINAL))).unwrap());
            if original.symlink_metadata().is_ok() {
                return exited(1, String::new());
            }
            std::fs::rename(can.join(n), &original).unwrap();
            std::fs::remove_file(can.join(format!("{}{}", n, ORIGINAL))).unwrap();
            exited(0, String::new())
        }
        ["trash", "--", paths @ ..] => {
            for path in paths {
                let n = NEXT_ENTRY.fetch_add(1, Ordering::Relaxed).to_string();
                std::fs::rename(path, can.join(&n)).unwrap();
                std::fs::write(can.join(format!("{}{}", n, ORIGINAL)), path).unwrap();
            }
            exited(0, String::new())
        }
        _ => exited(1, String::new()),
    }
}

fn working_trash(d: &TestDir) -> (StandIn, PathBuf) {
    let can = d.dir("can");
    let at = can.clone();
    trash::STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| Some(gio_trash(&at, args)))));
    (StandIn, can)
}

fn redo(journal: &mut Journal) -> Result<String, FleaError> {
    let (tx, _rx) = channel();
    journal.redo(1, &AtomicBool::new(false), &tx)
}

#[test]
fn replace_trashes_the_old_item_first_and_undo_and_redo_swap_them_back() {
    let d = TestDir::new("collide-replace");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let (_trash, can) = working_trash(&d);
    let (ok, failed, _, _, entry) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(text(&there), "yours");
    assert!(matches!(&entry.steps[..], [Step::Trashed(old), Step::Copied { .. }] if old.original == there), "{:?}", entry.steps);
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 2, "the old item and its original path are in the trash, not deleted");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    assert_eq!(journal.undo().unwrap(), "copy");
    assert_eq!(text(&there), "there", "one undo takes the new item away and restores the old one to its place");
    assert_eq!(text(&photo), "yours");
    assert_eq!(redo(&mut journal).unwrap(), "copy");
    assert_eq!(text(&there), "yours", "redo replaces again, through the trash again");
    journal.undo().unwrap();
    assert_eq!(text(&there), "there");
}

#[test]
fn a_moved_replace_undoes_to_both_original_places() {
    let d = TestDir::new("collide-replace-move");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let (_trash, _can) = working_trash(&d);
    let (ok, _, _, _, entry) = transfer(true, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!(ok, 1);
    assert!(!photo.exists());
    assert_eq!(text(&there), "yours");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    assert_eq!(journal.undo().unwrap(), "move");
    assert_eq!((text(&photo), text(&there)), ("yours".to_string(), "there".to_string()));
}

#[test]
fn a_colliding_folder_is_replaced_whole_and_never_merged() {
    let d = TestDir::new("collide-folder");
    let to = d.dir("to");
    let album = d.dir("from/album");
    d.file("from/album/new.txt", "new");
    d.dir("to/album");
    d.file("to/album/old.txt", "old");
    let (_trash, _can) = working_trash(&d);
    let (ok, _, _, _, entry) = transfer(false, &[&album], &to, chosen("replace", asked(1, &[&album], &to), &to));
    assert_eq!(ok, 1);
    assert!(to.join("album/new.txt").exists() && !to.join("album/old.txt").exists(), "the old folder went whole, nothing was merged");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&to.join("album"));
    journal.undo().unwrap();
    assert!(to.join("album/old.txt").exists() && !to.join("album/new.txt").exists());
}

#[test]
fn when_trash_refuses_nothing_is_replaced_and_nothing_is_journaled() {
    let d = TestDir::new("collide-refused");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let _trash = refusing_trash();
    let (ok, failed, _, errors, entry) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, failed), (0, 1));
    assert_eq!(errors, vec![TRASH_REFUSED.to_string()]);
    assert_eq!((text(&there), text(&photo)), ("there".to_string(), "yours".to_string()));
    assert!(entry.steps.is_empty());
}

#[test]
fn a_replace_whose_copy_fails_puts_the_old_item_straight_back() {
    let d = TestDir::new("collide-putback");
    let to = d.dir("to");
    d.dir("from");
    let shut = d.file("from/shut.txt", "unreadable");
    let there = d.file("to/shut.txt", "there");
    let (_trash, can) = working_trash(&d);
    let question = asked(1, &[&shut], &to);
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (ok, failed, _, errors, entry) = transfer(false, &[&shut], &to, chosen("replace", question, &to));
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!((ok, failed), (0, 1));
    assert_eq!(errors, vec!["permission denied".to_string()]);
    assert_eq!(text(&there), "there", "the copy never started, so the old item is back in its place");
    assert!(entry.steps.is_empty(), "and there is nothing left for undo to do");
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 0);
    // A cancel reaches the same path and keeps its own word, which is what the transfer counts it by.
    let cancelled = |_: &mut Vec<Step>| Err(FleaError { where_: "copy".into(), path: String::new(), msg: CANCELLED.into() });
    let mut steps = Vec::new();
    assert_eq!(replacing(&there, &mut steps, cancelled).unwrap_err().msg, CANCELLED);
    assert_eq!(text(&there), "there");
    assert!(steps.is_empty());
}

#[test]
fn replace_refuses_an_item_that_holds_the_one_being_moved_in() {
    let d = TestDir::new("collide-holds");
    let outer = d.dir("x");
    let inner = d.dir("x/x");
    let _trash = refusing_trash();
    let (ok, failed, _, errors, _) = transfer(true, &[&inner], d.path(), chosen("replace", asked(1, &[&inner], d.path()), d.path()));
    assert_eq!((ok, failed, errors), (0, 1, vec![HOLDS_SOURCE.to_string()]));
    assert!(outer.is_dir() && inner.is_dir());
}

#[test]
fn redo_of_a_replace_refuses_an_old_item_that_changed_since_the_undo() {
    let d = TestDir::new("collide-redo-changed");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let (_trash, can) = working_trash(&d);
    let (_, _, _, _, entry) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    journal.undo().unwrap();
    std::fs::rename(&there, d.join("elsewhere.png")).unwrap();
    d.file("to/photo.png", "someone else's since");
    assert!(redo(&mut journal).unwrap_err().msg.contains("changed"));
    assert_eq!(text(&there), "someone else's since", "redo trashes only the item the undo put back");
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 0);
}

#[test]
fn a_link_at_the_name_is_replaced_as_itself_and_its_target_is_never_touched() {
    let d = TestDir::new("collide-link");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let link = to.join("photo.png");
    std::os::unix::fs::symlink(&photo, &link).unwrap();
    let (_trash, _can) = working_trash(&d);
    let (ok, _, _, errors, _) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, errors), (1, Vec::<String>::new()), "a link to the source does not hold the source");
    assert!(!link.symlink_metadata().unwrap().file_type().is_symlink(), "the link went to Trash and a copy took its name");
    assert_eq!((text(&link), text(&photo)), ("yours".to_string(), "yours".to_string()));
}
