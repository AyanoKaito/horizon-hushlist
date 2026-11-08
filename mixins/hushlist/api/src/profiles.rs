//! Profiles: named sets of sound settings, one of them active, and the
//! profiles page of the screen. No game types, so `cargo test` covers it.
//!
//! Files under `config/hushlist/`: `profiles.txt` keeps the order, the names
//! and the active profile; `profiles/<id>.txt` keeps each profile's settings
//! in the settings file format.

use crate::ui::{Rect, contains};

pub const INDEX_FILE: &str = "profiles.txt";
pub const DIR: &str = "profiles";
/// Longest profile name, in characters.
pub const NAME_MAX: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// Names the profile's file; never reused.
    pub id: u32,
    /// Empty: the default name in the game's language.
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profiles {
    pub list: Vec<Profile>,
    pub active: u32,
}

impl Default for Profiles {
    fn default() -> Self {
        Profiles { list: vec![Profile { id: 1, name: String::new() }], active: 1 }
    }
}

/// A name as stored: one line, trimmed, at most [`NAME_MAX`] characters.
pub fn clean_name(name: &str) -> String {
    let one_line: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    one_line.trim().chars().take(NAME_MAX).collect::<String>().trim_end().to_owned()
}

impl Profiles {
    /// `active <id>` and `profile <id> <name>` lines; anything unreadable gives one default profile.
    pub fn parse(text: &str) -> Self {
        let mut list: Vec<Profile> = Vec::new();
        let mut active = None;
        for line in text.lines() {
            let line = line.trim_start();
            if let Some(rest) = line.strip_prefix("active ") {
                active = rest.trim().parse().ok();
            } else if let Some(rest) = line.strip_prefix("profile ") {
                let (id, name) = rest.split_once(' ').unwrap_or((rest, ""));
                if let Ok(id) = id.trim().parse::<u32>()
                    && id > 0
                    && !list.iter().any(|p| p.id == id)
                {
                    list.push(Profile { id, name: clean_name(name) });
                }
            }
        }
        if list.is_empty() {
            return Profiles::default();
        }
        let active = active.filter(|a| list.iter().any(|p| p.id == *a)).unwrap_or(list[0].id);
        Profiles { list, active }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# Hushlist profiles: the active one, then each profile's file number and name, in order.\n");
        out.push_str(&format!("active {}\n", self.active));
        for p in &self.list {
            out.push_str(&format!("profile {} {}\n", p.id, p.name));
        }
        out
    }

    /// The profile's settings file name, inside [`DIR`].
    pub fn file(id: u32) -> String {
        format!("{id}.txt")
    }

    pub fn active_index(&self) -> usize {
        self.list.iter().position(|p| p.id == self.active).unwrap_or(0)
    }

    fn next_id(&self) -> u32 {
        self.list.iter().map(|p| p.id).max().unwrap_or(0) + 1
    }

    /// A new profile at the end; returns its index.
    pub fn add(&mut self, name: &str) -> usize {
        let id = self.next_id();
        self.list.push(Profile { id, name: clean_name(name) });
        self.list.len() - 1
    }

    /// A copy of profile `i` right under it; returns the copy's index. The caller copies the file.
    pub fn duplicate(&mut self, i: usize, name: &str) -> Option<usize> {
        if i >= self.list.len() {
            return None;
        }
        let id = self.next_id();
        self.list.insert(i + 1, Profile { id, name: clean_name(name) });
        Some(i + 1)
    }

    /// False, and no change, for an empty name.
    pub fn rename(&mut self, i: usize, name: &str) -> bool {
        let name = clean_name(name);
        match self.list.get_mut(i) {
            Some(p) if !name.is_empty() => {
                p.name = name;
                true
            }
            _ => false,
        }
    }

    /// The active profile and the last one cannot be deleted.
    pub fn can_delete(&self, i: usize) -> bool {
        self.list.len() > 1 && self.list.get(i).is_some_and(|p| p.id != self.active)
    }

    /// Removes profile `i`; returns its id so the caller can delete its file.
    pub fn delete(&mut self, i: usize) -> Option<u32> {
        self.can_delete(i).then(|| self.list.remove(i).id)
    }

    pub fn set_active(&mut self, i: usize) -> bool {
        match self.list.get(i) {
            Some(p) => {
                self.active = p.id;
                true
            }
            None => false,
        }
    }
}

/// The profiles page while it is open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageState {
    pub open: bool,
    /// Selected row.
    pub sel: usize,
    /// The row whose name is being typed, and the text so far.
    pub edit: Option<usize>,
    pub name: String,
    /// The footer asks to confirm deleting the selected profile.
    pub delete: bool,
}

/// Measured text widths for the page's buttons.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PageMeasure {
    pub back: f32,
    pub new: f32,
    pub use_: f32,
    pub rename: f32,
    pub duplicate: f32,
    pub delete: f32,
    pub save: f32,
    pub cancel: f32,
    pub delete_yes: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageLayout {
    pub back: Rect,
    /// Where the page title starts; the hint follows it.
    pub title: [f32; 2],
    pub items: Vec<Rect>,
    /// Under the list: why the active profile cannot be deleted.
    pub note: Option<[f32; 2]>,
    pub new: Option<Rect>,
    pub use_: Option<Rect>,
    pub rename: Option<Rect>,
    pub duplicate: Option<Rect>,
    pub delete: Option<Rect>,
    pub save: Option<Rect>,
    pub cancel: Option<Rect>,
    pub delete_yes: Option<Rect>,
    /// Footer text start (the delete question) and the room it has.
    pub foot_text: [f32; 2],
    pub foot_room: f32,
    /// "Use" and "Delete..." do something for the selected row.
    pub can_use: bool,
    pub can_delete: bool,
}

pub const ITEM_H: f32 = 30.0;
pub const ITEM_GAP: f32 = 3.0;
const BUTTON_H: f32 = 20.0;
const BUTTON_PAD: f32 = 18.0;
const BUTTON_GAP: f32 = 5.0;

/// The page inside `panel` (the screen's panel, header excluded by the top offset).
pub fn page_layout(panel: Rect, m: &PageMeasure, profiles: &Profiles, st: &PageState) -> PageLayout {
    let [x, y, w, h] = panel;
    let pad = 8.0;
    let back = [x + pad, y + 28.0, m.back + BUTTON_PAD, 18.0];
    let title = [back[0] + back[2] + 8.0, back[1] + 5.0];
    let top = y + 54.0;
    let foot_y = y + h - pad - BUTTON_H;
    let mut items = Vec::new();
    for i in 0..profiles.list.len() {
        let iy = top + i as f32 * (ITEM_H + ITEM_GAP);
        if iy + ITEM_H > foot_y - 6.0 {
            break;
        }
        items.push([x + pad, iy, w - 2.0 * pad, ITEM_H]);
    }
    let active = profiles.active_index();
    let can_use = st.sel != active && st.sel < profiles.list.len();
    let can_delete = profiles.can_delete(st.sel);
    let note = (st.sel == active && profiles.list.len() > 1 && st.edit.is_none() && !st.delete)
        .then(|| [x + pad + 4.0, top + items.len() as f32 * (ITEM_H + ITEM_GAP) + 4.0])
        .filter(|n| n[1] + 9.0 < foot_y - 4.0);
    let mut out = PageLayout { back, title, items, note, can_use, can_delete, ..PageLayout::default() };
    let right = x + w - pad;
    // Right-aligned buttons, given right to left.
    let place = |widths: &[f32]| -> Vec<Rect> {
        let mut rx = right;
        let mut rects: Vec<Rect> = widths
            .iter()
            .map(|tw| {
                let bw = tw + BUTTON_PAD;
                rx -= bw;
                let r = [rx, foot_y, bw, BUTTON_H];
                rx -= BUTTON_GAP;
                r
            })
            .collect();
        rects.reverse();
        rects
    };
    out.foot_text = [x + pad + 2.0, foot_y + 6.0];
    if st.delete {
        let r = place(&[m.cancel, m.delete_yes]);
        out.delete_yes = Some(r[0]);
        out.cancel = Some(r[1]);
        out.foot_room = r[0][0] - 8.0 - out.foot_text[0];
    } else if st.edit.is_some() {
        let r = place(&[m.cancel, m.save]);
        out.save = Some(r[0]);
        out.cancel = Some(r[1]);
    } else {
        let r = place(&[m.delete, m.duplicate, m.rename, m.use_]);
        out.use_ = Some(r[0]);
        out.rename = Some(r[1]);
        out.duplicate = Some(r[2]);
        out.delete = Some(r[3]);
        out.new = Some([x + pad, foot_y, m.new + BUTTON_PAD, BUTTON_H]);
    }
    out
}

/// What a click on the page hit. Disabled buttons hit nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageHit {
    Back,
    Item(usize),
    New,
    Use,
    Rename,
    Duplicate,
    Delete,
    Save,
    Cancel,
    DeleteYes,
    Nothing,
}

pub fn page_hit(lay: &PageLayout, at: [f32; 2]) -> PageHit {
    let on = |r: Option<Rect>| r.is_some_and(|r| contains(r, at));
    if contains(lay.back, at) {
        return PageHit::Back;
    }
    if let Some(i) = lay.items.iter().position(|r| contains(*r, at)) {
        return PageHit::Item(i);
    }
    let buttons = [
        (lay.new, PageHit::New, true),
        (lay.use_, PageHit::Use, lay.can_use),
        (lay.rename, PageHit::Rename, true),
        (lay.duplicate, PageHit::Duplicate, true),
        (lay.delete, PageHit::Delete, lay.can_delete),
        (lay.save, PageHit::Save, true),
        (lay.cancel, PageHit::Cancel, true),
        (lay.delete_yes, PageHit::DeleteYes, true),
    ];
    buttons
        .into_iter()
        .find(|(r, _, enabled)| *enabled && on(*r))
        .map_or(PageHit::Nothing, |(_, hit, _)| hit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_broken_index_gives_one_default_profile() {
        assert_eq!(Profiles::parse(""), Profiles::default());
        assert_eq!(Profiles::parse("profile x name\nactive 9\n"), Profiles::default());
    }

    #[test]
    fn index_round_trips_names_order_and_active() {
        let p = Profiles {
            list: vec![
                Profile { id: 3, name: "Video recording".into() },
                Profile { id: 1, name: String::new() },
            ],
            active: 1,
        };
        assert_eq!(Profiles::parse(&p.to_text()), p);
    }

    #[test]
    fn unknown_active_falls_back_to_the_first_and_duplicates_are_skipped() {
        let p = Profiles::parse("active 7\nprofile 2 A\nprofile 2 B\nprofile 5 C\n");
        assert_eq!(p.list.iter().map(|x| x.id).collect::<Vec<_>>(), vec![2, 5]);
        assert_eq!(p.active, 2);
    }

    #[test]
    fn names_are_one_trimmed_line_of_limited_length() {
        assert_eq!(clean_name("  a\nb  "), "a b");
        assert_eq!(clean_name(&"x".repeat(50)).chars().count(), NAME_MAX);
    }

    #[test]
    fn add_duplicate_rename_delete() {
        let mut p = Profiles::default();
        assert_eq!(p.add("Night"), 1);
        assert_eq!(p.duplicate(0, "Default (copy)"), Some(1));
        assert_eq!(p.list.iter().map(|x| x.id).collect::<Vec<_>>(), vec![1, 3, 2]);
        assert!(!p.rename(2, "   "));
        assert!(p.rename(2, "Quiet night"));
        assert_eq!(p.list[2].name, "Quiet night");
        assert!(!p.can_delete(0), "active");
        assert_eq!(p.delete(1), Some(3));
        assert!(p.set_active(1));
        assert_eq!(p.active, 2);
        assert_eq!(p.delete(0), Some(1));
        assert!(!p.can_delete(0), "last one");
    }

    fn measure() -> PageMeasure {
        PageMeasure {
            back: 30.0,
            new: 60.0,
            use_: 40.0,
            rename: 45.0,
            duplicate: 50.0,
            delete: 40.0,
            save: 30.0,
            cancel: 35.0,
            delete_yes: 35.0,
        }
    }

    #[test]
    fn page_buttons_follow_the_mode_and_disable_for_the_active_row() {
        let mut p = Profiles::default();
        p.add("Second");
        let panel = [0.0, 0.0, 600.0, 300.0];
        let mut st = PageState { open: true, ..PageState::default() };
        let lay = page_layout(panel, &measure(), &p, &st);
        assert_eq!(lay.items.len(), 2);
        assert!(!lay.can_use && !lay.can_delete && lay.note.is_some());
        let use_ = lay.use_.unwrap();
        assert_eq!(page_hit(&lay, [use_[0] + 2.0, use_[1] + 2.0]), PageHit::Nothing);
        let d = lay.delete.unwrap();
        assert!(d[0] + d[2] <= 592.0 + 0.01);
        assert_eq!(page_hit(&lay, [lay.items[1][0] + 5.0, lay.items[1][1] + 5.0]), PageHit::Item(1));

        st.sel = 1;
        let lay = page_layout(panel, &measure(), &p, &st);
        assert!(lay.can_use && lay.can_delete && lay.note.is_none());
        let use_ = lay.use_.unwrap();
        assert_eq!(page_hit(&lay, [use_[0] + 2.0, use_[1] + 2.0]), PageHit::Use);

        st.delete = true;
        let lay = page_layout(panel, &measure(), &p, &st);
        assert!(lay.new.is_none() && lay.use_.is_none());
        let yes = lay.delete_yes.unwrap();
        assert_eq!(page_hit(&lay, [yes[0] + 2.0, yes[1] + 2.0]), PageHit::DeleteYes);
        assert!(lay.foot_room > 300.0);

        st.delete = false;
        st.edit = Some(1);
        let lay = page_layout(panel, &measure(), &p, &st);
        assert!(lay.save.is_some() && lay.cancel.is_some() && lay.rename.is_none());
    }

    #[test]
    fn rows_that_would_cover_the_footer_are_left_out() {
        let mut p = Profiles::default();
        for i in 0..20 {
            p.add(&format!("P{i}"));
        }
        let lay = page_layout([0.0, 0.0, 600.0, 200.0], &measure(), &p, &PageState::default());
        let last = lay.items.last().unwrap();
        assert!(last[1] + last[3] <= 200.0 - 8.0 - 20.0 - 6.0);
    }
}
