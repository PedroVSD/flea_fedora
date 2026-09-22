// Names a paste or a drop would land on: the question asked first, and the one choice the transfer carries.
use crate::backend::icons::Names;
use crate::backend::menu_actions::Selected;
use crate::backend::mime::Db;
use crate::backend::ops::free_copy_path;
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::{usable_dest, ALREADY_THERE};
use crate::backend::trash;
use crate::backend::undo::{ItemIdentity, Step};
use crate::error::FleaError;
use crate::json::{escape, field_str, field_usize};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

// The card lists this many names and says how many more there are, so the answer never carries more.
pub const SHOWN: usize = 3;
// The word Duplicate already uses, so a kept copy and a duplicate are named by one rule.
const KEEP_WORD: &str = "copy";
pub const TRASH_REFUSED: &str = "the item already there could not be moved to Trash, so nothing was replaced";
pub const HOLDS_SOURCE: &str = "the item already there holds the one being moved in, so it was not replaced";
pub const NO_FREE_NAME: &str = "every copy name for this item is already taken";
// copyfile's own word for a cancelled item, which is what the transfer counts a cancel by.
const CANCELLED: &str = "cancelled";

// The operator's one answer, for every name the question listed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Collide {
    Refuse,
    Keep,
    Replace,
    Skip,
}

impl Collide {
    // Anything that is not one of the three choices refuses, so a malformed word can never replace.
    fn from_word(word: &str) -> Collide {
        match word {
            "keep" => Collide::Keep,
            "replace" => Collide::Replace,
            "skip" => Collide::Skip,
            _ => Collide::Refuse,
        }
    }
}

// What the question saw: each colliding source, and the item that held its name in dest at that moment.
pub struct Question {
    id: usize,
    dest: PathBuf,
    seen: HashMap<PathBuf, ItemIdentity>,
    menu: Option<MenuCapture>,
}

// A menu's selection as its question saw it: Copy to closes its dialog, which expires the live one, before the answer lands.
struct MenuCapture {
    id: usize,
    items: Vec<Selected>,
    destination: Option<Selected>,
}

// The transfer request's two fields, before the question they name has been looked up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ask {
    word: Option<String>,
    id: usize,
}

impl Ask {
    // Sample input: {"c":"transfer","op":"copy","paths":["/home/gm/a.png"],"dest":"/home/gm/Pictures","collide":"replace","collideId":7}
    pub fn parse(line: &str) -> Ask {
        Ask { word: field_str(line, "collide"), id: field_usize(line, "collideId").unwrap_or(0) }
    }

    // The choice covers only the question this request names, asked about this same destination.
    pub fn policy(self, question: Option<Question>, dest: &Path) -> Policy {
        let seen = match question {
            Some(q) if self.answers(&q, dest) => q.seen,
            _ => HashMap::new(),
        };
        Policy { choice: self.word.as_deref().map(Collide::from_word), seen }
    }

    fn answers(&self, question: &Question, dest: &Path) -> bool {
        self.id != 0 && question.id == self.id && question.dest == dest
    }
}

// The selection a menu transfer runs on: the one its question captured, else the menu's live one.
pub(crate) fn menu_sources(ops: &Ops, id: usize, dest: &str, collide: &Ask) -> Result<(Vec<Selected>, Option<Selected>), String> {
    let captured = ops.question.as_ref().filter(|q| collide.answers(q, Path::new(dest))).and_then(|q| q.menu.as_ref());
    if let Some(menu) = captured.filter(|menu| menu.id == id) {
        return Ok((menu.items.clone(), menu.destination.clone()));
    }
    ops.menuactions.as_ref().ok_or_else(|| "Menu selection expired; reopen the menu.".to_string())
        .and_then(|menu| Ok((menu.selection(id)?, menu.provider_destination(id, Path::new(dest))?)))
}

// An expired selection captures nothing, so the question answers no collision and the transfer its own error.
fn capture(ops: &Ops, id: usize, dest: &str) -> Option<MenuCapture> {
    let menu = ops.menuactions.as_ref()?;
    Some(MenuCapture { id, items: menu.selection(id).ok()?, destination: menu.provider_destination(id, Path::new(dest)).ok()? })
}

// No choice at all is today's transfer exactly; any choice also turns on the same-folder rule.
#[derive(Default)]
pub struct Policy {
    choice: Option<Collide>,
    seen: HashMap<PathBuf, ItemIdentity>,
}

// Where one item goes; replace says what holds that path now goes to Trash first.
#[derive(Debug, PartialEq)]
pub enum Place {
    Land { to: PathBuf, replace: bool },
    Skip,
    Refuse(String),
}

impl Policy {
    // here says the item already lives in dest; a name the question did not see is left to the exclusive create, which refuses it.
    pub fn place(&self, src: &Path, dst: PathBuf, here: bool, moving: bool) -> Place {
        let Some(choice) = self.choice else {
            return if here { Place::Refuse(ALREADY_THERE.to_string()) } else { Place::Land { to: dst, replace: false } };
        };
        if here {
            // A move onto itself changes nothing, and a copy into its own folder is a Duplicate.
            return if moving { Place::Skip } else { keep_both(dst) };
        }
        let Ok(current) = ItemIdentity::inspect(&dst) else { return Place::Land { to: dst, replace: false } };
        if !self.seen.get(src).is_some_and(|seen| seen.same_item(&current)) {
            return Place::Land { to: dst, replace: false };
        }
        match choice {
            Collide::Refuse => Place::Land { to: dst, replace: false },
            Collide::Keep => keep_both(dst),
            Collide::Skip => Place::Skip,
            Collide::Replace if holds(&dst, src) => Place::Refuse(HOLDS_SOURCE.to_string()),
            Collide::Replace => Place::Land { to: dst, replace: true },
        }
    }
}

fn keep_both(dst: PathBuf) -> Place {
    match free_copy_path(&dst, KEEP_WORD) {
        Some(free) => Place::Land { to: free, replace: false },
        None => Place::Refuse(NO_FREE_NAME.to_string()),
    }
}

// Trashing a folder the source sits inside would take the source to Trash with it; a link at dst trashes only itself.
fn holds(dst: &Path, src: &Path) -> bool {
    match (resolved_parent(dst), resolved_parent(src)) {
        (Some(there), Some(here)) => here.starts_with(there),
        _ => false,
    }
}

// The parent resolved and the last component left as it is, so a symlink names itself and not its target.
fn resolved_parent(path: &Path) -> Option<PathBuf> {
    Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?))
}

// The item already lives in dest: its name there is the item itself, through a symlinked folder too.
pub fn already_there(src: &Path, name: &str, dst: &Path, dest_real: &Path) -> bool {
    let src_here = match src.parent() {
        Some(parent) => parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf()).join(name),
        None => src.to_path_buf(),
    };
    dst == src || dest_real.join(name) == src_here
}

// Replace is Trash and then the transfer, journaled as one entry: undo removes the new item, then restores the old.
pub fn replacing(dst: &Path, steps: &mut Vec<Step>, land: impl FnOnce(&mut Vec<Step>) -> Result<(), FleaError>) -> Result<(), FleaError> {
    let (mut entries, failed) = trash::trash(&[dst.to_path_buf()]);
    if failed != 0 || entries.len() != 1 {
        return Err(FleaError { where_: "transfer".into(), path: dst.to_string_lossy().into(), msg: TRASH_REFUSED.into() });
    }
    let entry = entries.remove(0);
    let at = steps.len();
    steps.push(Step::Trashed(entry.clone()));
    let mut outcome = land(steps);
    if let Err(error) = &mut outcome {
        put_back(&entry, dst, steps, at, error);
    }
    outcome
}

// Nothing took the name, a cancel included, so the old item goes straight back instead of waiting on an undo.
fn put_back(entry: &trash::Entry, dst: &Path, steps: &mut Vec<Step>, at: usize, error: &mut FleaError) {
    let vacant = matches!(dst.symlink_metadata(), Err(e) if e.kind() == std::io::ErrorKind::NotFound);
    if !vacant || steps.len() != at + 1 {
        return;
    }
    match trash::restore(entry) {
        Ok(()) => steps.truncate(at),
        // A cancel keeps its own word, which is what counts it; the step stays, so undo can still restore it.
        Err(_) if error.msg == CANCELLED => {}
        Err(restore) => error.msg.push_str(&format!("; the item it replaced is still in Trash ({})", restore.msg)),
    }
}

// One colliding source as the card draws it: its name, and the kind its mark is chosen from.
struct Shown {
    name: String,
    dir: bool,
    mode: u32,
}

// The question itself, read-only: which sources would land on a name dest already holds.
fn ask(id: usize, paths: &[String], dest: &str) -> (Question, Vec<Shown>) {
    let mut question = Question { id, dest: PathBuf::from(dest), seen: HashMap::new(), menu: None };
    let mut shown = Vec::new();
    // An unusable destination asks nothing: the transfer that follows answers its own error.
    let Ok(dest_path) = usable_dest(dest) else { return (question, shown) };
    let dest_real = dest_path.canonicalize().unwrap_or_else(|_| dest_path.clone());
    for raw in paths {
        let src = Path::new(raw);
        if !src.is_absolute() {
            continue;
        }
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(meta) = src.symlink_metadata() else { continue };
        let dst = dest_path.join(name);
        // Same-folder items never ask: a copy keeps both and a move stays put, see Policy::place.
        if already_there(src, name, &dst, &dest_real) {
            continue;
        }
        let Ok(there) = ItemIdentity::inspect(&dst) else { continue };
        if question.seen.insert(src.to_path_buf(), there).is_none() && shown.len() < SHOWN {
            shown.push(Shown { name: name.to_string(), dir: meta.is_dir(), mode: meta.mode() });
        }
    }
    (question, shown)
}

// Sample output: {"t":"collisions","id":7,"total":2,"names":[{"n":"a.png","d":false,"i":"image-x-generic"}]}
fn collisions_line(id: usize, total: usize, shown: &[Shown], mime: &Db, icons: &Names) -> String {
    let names: Vec<String> = shown.iter().map(|s| format!(r#"{{"n":"{}","d":{},"i":"{}"}}"#,
        escape(&s.name), s.dir, escape(icons.icon_for(mime.lookup(&s.name), s.dir, s.mode)))).collect();
    format!(r#"{{"t":"collisions","id":{},"total":{},"names":[{}]}}"#, id, total, names.join(","))
}

// The backend keeps the latest question only; the one transfer that names it spends it. A menu_id asks about that menu's selection.
pub(crate) fn answer(ops: &mut Ops, id: usize, menu_id: usize, named: Vec<String>, dest: &str, mime: &Db, icons: &Names) -> String {
    let menu = if menu_id == 0 { None } else { capture(ops, menu_id, dest) };
    let paths = match (&menu, menu_id) {
        (Some(menu), _) => menu.items.iter().map(|item| item.path.to_string_lossy().to_string()).collect(),
        (None, 0) => named,
        (None, _) => Vec::new(),
    };
    let (mut question, shown) = ask(id, &paths, dest);
    question.menu = menu;
    let line = collisions_line(id, question.seen.len(), &shown, mime, icons);
    ops.question = Some(question);
    line
}

#[cfg(test)]
#[path = "collide_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "collide_replace_tests.rs"]
mod replace_tests;
