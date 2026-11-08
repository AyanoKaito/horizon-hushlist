// Hushlist screen model: which rows the Recent list shows, where everything is
// drawn, and what a click hits. No game types, so it is tested standalone; the
// trait mod (lib.rs) only draws these rectangles and forwards input.
use crate::core::{Settings, group_key};
use std::collections::{HashMap, HashSet};

/// One row's height in GUI pixels: a name line and a detail line, with air around them.
pub const ROW_H: f32 = 26.0;
/// Gap between rows.
pub const ROW_GAP: f32 = 3.0;
/// Baselines inside a row: name line, detail line, and a single centred line.
pub const NAME_Y: f32 = 5.0;
pub const DETAIL_Y: f32 = 15.0;
pub const MID_Y: f32 = 9.0;
/// Extra left inset of a sound shown inside its group.
pub const CHILD_INDENT: f32 = 12.0;
/// A start counts as fresh (row highlight) this long, in seconds.
pub const FLASH_SECS: f32 = 0.9;
/// Groups of sounds that usually play for a long time; they start collapsed.
pub const CONSTANT_FAMILIES: [&str; 4] = ["weather", "music", "music_disc", "ambient"];
/// Category chips in their default order: the game's sound source index and
/// the translation key of the chip. Master and voice have no chip.
pub const CATEGORIES: [(usize, &str); 9] = [
    (4, "cat.block"),
    (6, "cat.neutral"),
    (5, "cat.hostile"),
    (7, "cat.player"),
    (3, "cat.weather"),
    (8, "cat.ambient"),
    (1, "cat.music"),
    (2, "cat.record"),
    (10, "cat.ui"),
];
/// Longest search text.
pub const QUERY_MAX: usize = 40;
/// Cooldown slider steps in seconds; 0 is "default" (no cooldown).
pub const RATES: [u32; 16] = [0, 1, 2, 3, 5, 10, 15, 20, 30, 45, 60, 90, 120, 180, 300, 600];

/// Slider step of a cooldown: the first step at or above it.
pub fn rate_index(secs: u32) -> usize {
    RATES.iter().position(|r| *r >= secs).unwrap_or(RATES.len() - 1)
}

/// Cooldown step for a pointer at `x` on the slider.
pub fn rate_value(slider: Rect, x: f32) -> usize {
    let t = ((x - slider[0] - 2.0) / (slider[2] - 4.0)).clamp(0.0, 1.0);
    (t * (RATES.len() - 1) as f32).round() as usize
}

/// `30 s`, `1.5 min` (`30 с`, `1,5 мин` in Russian), in the language `lang`.
pub fn rate_text(secs: u32, lang: &str) -> String {
    use crate::i18n::with_n;
    if secs < 60 {
        return with_n(lang, "cooldown.secs", secs);
    }
    let mins = secs as f32 / 60.0;
    let mut n = if mins.fract() == 0.0 { format!("{}", mins as u32) } else { format!("{mins:.1}") };
    if lang.starts_with("ru") {
        n = n.replace('.', ",");
    }
    with_n(lang, "cooldown.mins", n)
}

/// "30 s cooldown" for the badge and the drawer, or "default" for none.
pub fn cooldown_text(secs: u32, lang: &str) -> String {
    use crate::i18n::text;
    if secs == 0 {
        return text(lang, "cooldown.off").to_owned();
    }
    text(lang, "cooldown.badge").replace("{time}", &rate_text(secs, lang))
}

/// Mobs counted as hostile when guessing a category.
const HOSTILE: [&str; 42] = [
    "zombie", "husk", "drowned", "skeleton", "stray", "bogged", "wither_skeleton", "creeper", "spider", "cave_spider",
    "enderman", "endermite", "silverfish", "slime", "magma_cube", "ghast", "blaze", "witch", "vindicator", "evoker",
    "illusioner", "pillager", "ravager", "vex", "guardian", "elder_guardian", "shulker", "phantom", "piglin",
    "piglin_brute", "zombified_piglin", "hoglin", "zoglin", "warden", "breeze", "creaking", "wither", "ender_dragon",
    "zombie_villager", "parched", "zombie_nautilus", "camel_husk",
];
/// Entity sounds that belong to the player's own actions.
const PLAYER_ENTITIES: [&str; 13] = [
    "player", "generic", "experience_orb", "item", "arrow", "fishing_bobber", "ender_pearl", "snowball", "egg",
    "experience_bottle", "splash_potion", "lingering_potion", "wind_charge",
];

/// The category a sound most likely plays in, from its ID. The game picks the category
/// at each play, so this is a guess for sounds not heard yet; a heard sound's real one wins.
pub fn guess_source(id: &str) -> usize {
    let mut p = crate::core::normalize(id).split('.');
    let (a, b) = (p.next().unwrap_or(""), p.next().unwrap_or(""));
    match a {
        "music" => 1,
        "music_disc" => 2,
        "weather" => 3,
        "ui" => 10,
        "block" if b == "note_block" => 2,
        "block" => 4,
        "item" | "enchant" => 7,
        "entity" if PLAYER_ENTITIES.contains(&b) => 7,
        "entity" if HOSTILE.contains(&b) => 5,
        "entity" => 6,
        _ => 8,
    }
}

/// The chip's translation key for a sound source index.
pub fn category_key(source: usize) -> Option<&'static str> {
    CATEGORIES.iter().find(|(s, _)| *s == source).map(|(_, k)| *k)
}

/// The list tabs. Zones has no list yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Recent,
    All,
    Changed,
    Zones,
}

impl Tab {
    pub const ORDER: [Tab; 4] = [Tab::Recent, Tab::All, Tab::Changed, Tab::Zones];
    /// Tabs that can be opened so far.
    pub fn ready(self) -> bool {
        matches!(self, Tab::Recent | Tab::All | Tab::Changed)
    }
}

/// Order of the All tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortAll {
    #[default]
    Alpha,
    Category,
    Size,
}

impl SortAll {
    pub const ORDER: [SortAll; 3] = [SortAll::Alpha, SortAll::Category, SortAll::Size];
    pub fn key(self) -> &'static str {
        match self {
            SortAll::Alpha => "sort.alpha",
            SortAll::Category => "sort.category",
            SortAll::Size => "sort.size",
        }
    }
}

/// Order of the Changed tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortChanged {
    #[default]
    Alpha,
    Recent,
}

impl SortChanged {
    pub const ORDER: [SortChanged; 2] = [SortChanged::Alpha, SortChanged::Recent];
    pub fn key(self) -> &'static str {
        match self {
            SortChanged::Alpha => "sort.alpha",
            SortChanged::Recent => "sort.recent",
        }
    }
}

/// Text widths the layout needs, measured in the game's font.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Measure {
    /// Chip labels in drawing order.
    pub chips: Vec<f32>,
    /// Tab labels with their counts.
    pub tabs: [f32; 4],
    /// Sort menu items.
    pub sort_items: Vec<f32>,
    /// "Reset all", "Reset", "Cancel".
    pub buttons: [f32; 3],
}

impl Measure {
    pub fn chips(chips: Vec<f32>) -> Self {
        Measure { chips, ..Measure::default() }
    }
}

/// A row's identity: a group (by group key), a sound (by sound event ID), or
/// the "N more in this group" button under an expanded group.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Group(String),
    Sound(String),
    More(String),
    /// The open "less often" drawer under a sound.
    Cooldown(String),
}

/// Every sound event of the game by group, sorted, for showing a group's unheard sounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub groups: HashMap<String, Vec<String>>,
    /// Sounds without a group, sorted.
    pub loose: Vec<String>,
    /// Number of sound events in the game.
    pub total: usize,
}

impl Catalog {
    pub fn from_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Self {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        let mut total = 0;
        let mut loose = Vec::new();
        for id in ids {
            total += 1;
            match group_key(id) {
                Some(g) => groups.entry(g).or_default().push(crate::core::normalize(id).to_owned()),
                None => loose.push(crate::core::normalize(id).to_owned()),
            }
        }
        loose.sort();
        loose.dedup();
        for v in groups.values_mut() {
            v.sort();
            v.dedup();
        }
        Catalog { groups, loose, total }
    }
}

/// What the list knows about one heard sound.
#[derive(Clone, Debug, PartialEq)]
pub struct HeardView {
    pub id: String,
    /// Seconds since its last start.
    pub secs: f32,
    pub count: u32,
    /// Still getting live updates (a long or looping sound).
    pub playing: bool,
    /// The game's sound source index of the last play (its category).
    pub source: usize,
}

/// A top-level entry before flattening: a group with its heard sounds, or a sound without a group.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: Key,
    /// Heard sound IDs, most recent first. Empty for a sound entry.
    pub kids: Vec<String>,
}

/// One drawn row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: Key,
    /// A sound drawn inside its group.
    pub child: bool,
    /// Group rows: expanded.
    pub open: bool,
    /// Something in this row is still playing.
    pub playing: bool,
    /// Started within [`FLASH_SECS`].
    pub flash: bool,
    /// Seconds since the latest start in this row.
    pub secs: f32,
    pub count: u32,
    /// Group rows: heard sound IDs, most recent first.
    pub heard: Vec<String>,
    /// Effective volume, percent.
    pub volume: u8,
    /// Set by hand on this row (shows reset).
    pub own: bool,
    /// Below 100 % because of its group.
    pub inherited: bool,
    /// Sound rows and drawers: cooldown in seconds, 0 for none.
    pub cooldown: u32,
}

/// The list's order frozen by the Freeze button.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub order: Vec<Key>,
    pub kids: HashMap<String, Vec<String>>,
}

/// Screen state that survives between frames.
#[derive(Clone, Debug, Default)]
pub struct ListState {
    /// Groups the player expanded (true) or collapsed (false); others use the default.
    pub open: HashMap<String, bool>,
    pub frozen: bool,
    pub snapshot: Option<Snapshot>,
    /// While not frozen, the order refreshed about once a second, so rows have time to glide.
    pub live: Option<Snapshot>,
    /// List scroll in GUI pixels.
    pub scroll: f32,
    /// Volume before mute, to restore it.
    pub before_mute: HashMap<Key, u8>,
    /// Expanded groups that also show the sounds not heard yet.
    pub show_all: HashSet<String>,
    /// Search text as typed; matching ignores case. Empty shows everything.
    pub query: String,
    /// The search field takes the keyboard.
    pub search_focus: bool,
    /// Selected category chips (sound source indices), in the order they were picked.
    pub cats: Vec<usize>,
    /// Flat list instead of groups.
    pub flat: bool,
    /// The sound whose "less often" drawer is open (one at a time).
    pub drawer: Option<String>,
    pub tab: Tab,
    /// Expanded or collapsed groups in the Changed tab (expanded by default).
    pub open_changed: HashMap<String, bool>,
    pub sort_changed: SortChanged,
    pub sort_all: SortAll,
    /// Expanded groups in the All tab (collapsed by default).
    pub open_all: HashMap<String, bool>,
    /// The sort menu is open.
    pub sort_open: bool,
    /// "Reset all" asks for confirmation.
    pub confirm_reset: bool,
}

impl ListState {
    /// Groups start expanded, except families of constant sounds (weather,
    /// music, discs, ambience) and groups with a sound that keeps playing (a
    /// portal): those would bury the rest.
    pub fn is_open(&self, group: &str, has_playing: bool) -> bool {
        self.open
            .get(group)
            .copied()
            .unwrap_or(!(has_playing || CONSTANT_FAMILIES.contains(&group)))
    }

    /// Chips as drawn: the picked ones first, in the order they were picked, then the rest.
    pub fn chip_order(&self) -> Vec<usize> {
        let mut out = self.cats.clone();
        out.extend(CATEGORIES.iter().map(|(s, _)| *s).filter(|s| !self.cats.contains(s)));
        out
    }

    pub fn toggle_cat(&mut self, source: usize) {
        if let Some(i) = self.cats.iter().position(|s| *s == source) {
            self.cats.remove(i);
        } else {
            self.cats.push(source);
        }
    }

    /// Expanded state of a group in the current tab, for toggling it.
    pub fn group_open(&self, group: &str, has_playing: bool) -> bool {
        match self.tab {
            Tab::Changed => self.open_changed.get(group).copied().unwrap_or(true),
            Tab::All => self.open_all.get(group).copied().unwrap_or(false),
            _ => self.is_open(group, has_playing),
        }
    }

    pub fn set_group_open(&mut self, group: &str, open: bool) {
        match self.tab {
            Tab::Changed => self.open_changed.insert(group.to_owned(), open),
            Tab::All => self.open_all.insert(group.to_owned(), open),
            _ => self.open.insert(group.to_owned(), open),
        };
    }

    /// Search or chips hide part of the list.
    pub fn filtering(&self) -> bool {
        !self.query.is_empty() || !self.cats.is_empty()
    }
}

fn by_id<'a>(heard: &'a [HeardView]) -> HashMap<&'a str, &'a HeardView> {
    heard.iter().map(|h| (h.id.as_str(), h)).collect()
}

/// Live order: playing entries first, then by latest start. `flat` lists sounds without groups.
pub fn entries(heard: &[HeardView], flat: bool) -> Vec<Entry> {
    let mut groups: Vec<(String, Vec<&HeardView>)> = Vec::new();
    let mut out: Vec<(Entry, bool, f32)> = Vec::new();
    for h in heard {
        match group_key(&h.id).filter(|_| !flat) {
            Some(g) => match groups.iter_mut().find(|(k, _)| *k == g) {
                Some((_, kids)) => kids.push(h),
                None => groups.push((g, vec![h])),
            },
            None => out.push((
                Entry {
                    key: Key::Sound(h.id.clone()),
                    kids: Vec::new(),
                },
                h.playing,
                h.secs,
            )),
        }
    }
    for (g, mut kids) in groups {
        kids.sort_by(|a, b| a.secs.total_cmp(&b.secs).then_with(|| a.id.cmp(&b.id)));
        let playing = kids.iter().any(|k| k.playing);
        let secs = kids[0].secs;
        out.push((
            Entry {
                key: Key::Group(g),
                kids: kids.iter().map(|k| k.id.clone()).collect(),
            },
            playing,
            secs,
        ));
    }
    out.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.2.total_cmp(&b.2))
            .then_with(|| a.0.key.cmp(&b.0.key))
    });
    out.into_iter().map(|(e, _, _)| e).collect()
}

/// The order to freeze now.
pub fn snapshot(heard: &[HeardView], flat: bool) -> Snapshot {
    let es = entries(heard, flat);
    Snapshot {
        order: es.iter().map(|e| e.key.clone()).collect(),
        kids: es
            .iter()
            .filter_map(|e| match &e.key {
                Key::Group(g) => Some((g.clone(), e.kids.clone())),
                Key::Sound(_) | Key::More(_) | Key::Cooldown(_) => None,
            })
            .collect(),
    }
}

/// The rows to draw and, while frozen, how many heard sounds the frozen order leaves out.
/// `matches` says whether a sound ID fits the search text (names live in the game, not here).
pub fn build(
    heard: &[HeardView],
    settings: &Settings,
    st: &ListState,
    catalog: &Catalog,
    matches: &dyn Fn(&str) -> bool,
) -> (Vec<Row>, usize) {
    let heard: Vec<HeardView> = heard
        .iter()
        .filter(|h| (st.cats.is_empty() || st.cats.contains(&h.source)) && matches(&h.id))
        .cloned()
        .collect();
    let heard = heard.as_slice();
    let live = entries(heard, st.flat);
    let mut fresh = 0;
    let order = if st.frozen { st.snapshot.as_ref() } else { st.live.as_ref() };
    let shown: Vec<Entry> = match order {
        Some(snap) => {
            for e in live.iter().filter(|_| st.frozen) {
                match &e.key {
                    Key::Group(g) if snap.order.contains(&e.key) => {
                        let known = snap.kids.get(g).map(Vec::as_slice).unwrap_or(&[]);
                        fresh += e.kids.iter().filter(|k| !known.contains(k)).count();
                    }
                    _ if !snap.order.contains(&e.key) => fresh += e.kids.len().max(1),
                    _ => {}
                }
            }
            snap.order
                .iter()
                .filter_map(|k| live.iter().find(|e| &e.key == k))
                .map(|e| match &e.key {
                    Key::Group(g) => {
                        let known = snap.kids.get(g).cloned().unwrap_or_default();
                        Entry {
                            key: e.key.clone(),
                            kids: known.into_iter().filter(|k| e.kids.contains(k)).collect(),
                        }
                    }
                    Key::Sound(_) | Key::More(_) | Key::Cooldown(_) => e.clone(),
                })
                .collect()
        }
        None => live,
    };
    let info = by_id(heard);
    let mut rows = Vec::new();
    for e in shown {
        match &e.key {
            Key::Sound(id) => {
                if let Some(h) = info.get(id.as_str()) {
                    rows.push(sound_row(h, settings, false));
                }
            }
            Key::Group(g) => {
                let kids: Vec<&HeardView> =
                    e.kids.iter().filter_map(|k| info.get(k.as_str()).copied()).collect();
                let Some(first) = kids.first() else { continue };
                let playing = kids.iter().any(|k| k.playing);
                let open = st.is_open(g, playing);
                let volume = settings.group_percent(g);
                rows.push(Row {
                    key: e.key.clone(),
                    child: false,
                    open,
                    playing,
                    flash: kids.iter().any(|k| k.secs < FLASH_SECS),
                    secs: first.secs,
                    count: kids.iter().map(|k| k.count).sum(),
                    heard: kids.iter().map(|k| k.id.clone()).collect(),
                    volume,
                    own: settings.groups.contains_key(g),
                    inherited: false,
                    cooldown: 0,
                });
                if open {
                    rows.extend(kids.iter().map(|k| sound_row(k, settings, true)));
                    let all = catalog.groups.get(g).map(Vec::as_slice).unwrap_or(&[]);
                    // Unheard sounds use their guessed category.
                    let rest: Vec<&String> = all
                        .iter()
                        .filter(|id| {
                            !e.kids.contains(id)
                                && matches(id)
                                && (st.cats.is_empty() || st.cats.contains(&guess_source(id)))
                        })
                        .collect();
                    let show_all = st.show_all.contains(g);
                    if show_all {
                        for id in &rest {
                            let unheard = HeardView {
                                id: (*id).clone(),
                                secs: f32::INFINITY,
                                count: 0,
                                playing: false,
                                source: 0,
                            };
                            rows.push(sound_row(&unheard, settings, true));
                        }
                    }
                    if !rest.is_empty() {
                        rows.push(more_row(g, rest.len(), show_all));
                    }
                }
            }
            Key::More(_) | Key::Cooldown(_) => {}
        }
    }
    // The open drawer sits right under its sound.
    if let Some(id) = &st.drawer
        && let Some(i) = rows.iter().position(|r| r.key == Key::Sound(id.clone()))
    {
        let child = rows[i].child;
        rows.insert(i + 1, cooldown_row(id, child, settings));
    }
    (rows, fresh)
}

fn cooldown_secs(settings: &Settings, id: &str) -> u32 {
    settings.cooldowns.get(id).copied().unwrap_or(0)
}

fn cooldown_row(id: &str, child: bool, settings: &Settings) -> Row {
    Row {
        key: Key::Cooldown(id.to_owned()),
        child,
        open: true,
        playing: false,
        flash: false,
        secs: f32::INFINITY,
        count: 0,
        heard: Vec::new(),
        volume: 100,
        own: cooldown_secs(settings, id) > 0,
        inherited: false,
        cooldown: cooldown_secs(settings, id),
    }
}

/// The Changed tab: groups and sounds the player set, sorted by name or by latest change.
/// Expanded groups show every sound of the group, since the group volume reaches all of them.
/// In the flat view, every sound quieter than normal or with a cooldown, one row each.
/// `name` gives a row's display name for sorting.
pub fn build_changed(
    heard: &[HeardView],
    settings: &Settings,
    st: &ListState,
    catalog: &Catalog,
    matches: &dyn Fn(&str) -> bool,
    name: &dyn Fn(&Key) -> String,
) -> Vec<Row> {
    let info = by_id(heard);
    let unheard = |id: &str| HeardView {
        id: id.to_owned(),
        secs: f32::INFINITY,
        count: 0,
        playing: false,
        source: usize::MAX,
    };
    let view = |id: &str| info.get(id).map_or_else(|| unheard(id), |h| (*h).clone());
    let source = |id: &str| info.get(id).map_or_else(|| guess_source(id), |h| h.source);
    let pass = |id: &str| matches(id) && (st.cats.is_empty() || st.cats.contains(&source(id)));
    let mut ids: Vec<String> = settings.sounds.keys().chain(settings.cooldowns.keys()).cloned().collect();
    ids.sort();
    ids.dedup();
    let sorted = |mut keyed: Vec<(Key, u64, Row, Vec<Row>)>| {
        match st.sort_changed {
            SortChanged::Alpha => keyed.sort_by_cached_key(|(k, _, _, _)| (name(k).to_lowercase(), k.clone())),
            SortChanged::Recent => {
                keyed.sort_by_cached_key(|(k, at, _, _)| (std::cmp::Reverse(*at), name(k).to_lowercase(), k.clone()))
            }
        }
        let mut rows = Vec::new();
        for (_, _, row, kids) in keyed {
            rows.push(row);
            rows.extend(kids);
        }
        rows
    };
    if st.flat {
        let mut all: Vec<String> = ids.clone();
        for g in settings.groups.keys() {
            if settings.group_percent(g) < 100 {
                all.extend(catalog.groups.get(g).into_iter().flatten().cloned());
            }
        }
        all.sort();
        all.dedup();
        let keyed = all
            .into_iter()
            .filter(|id| settings.volume_percent(id) < 100 || settings.cooldowns.contains_key(id))
            .filter(|id| pass(id))
            .map(|id| {
                let at = settings.sound_stamp(&id).max(group_key(&id).map_or(0, |g| settings.group_stamp(&g)));
                (Key::Sound(id.clone()), at, sound_row(&view(&id), settings, false), Vec::new())
            })
            .collect();
        return sorted(keyed);
    }
    let mut groups: Vec<String> = settings.groups.keys().cloned().collect();
    let mut loose = Vec::new();
    for id in &ids {
        match group_key(id) {
            Some(g) => groups.push(g),
            None => loose.push(id.clone()),
        }
    }
    groups.sort();
    groups.dedup();
    let mut keyed = Vec::new();
    for g in groups {
        let all: Vec<String> = match catalog.groups.get(&g) {
            Some(kids) => kids.clone(),
            None => ids.iter().filter(|id| group_key(id).as_deref() == Some(g.as_str())).cloned().collect(),
        };
        let changed_kids: Vec<&String> = ids.iter().filter(|id| group_key(id).as_deref() == Some(g.as_str())).collect();
        let shown = changed_kids.iter().any(|id| pass(id))
            || (settings.groups.contains_key(&g) && all.iter().any(|id| pass(id)));
        if !shown {
            continue;
        }
        let kids: Vec<HeardView> = all
            .iter()
            .filter(|id| st.cats.is_empty() || st.cats.contains(&source(id)))
            .map(|id| view(id))
            .collect();
        let heard_kids: Vec<&HeardView> = kids.iter().filter(|k| k.count > 0).collect();
        let open = st.open_changed.get(&g).copied().unwrap_or(true);
        let row = Row {
            key: Key::Group(g.clone()),
            child: false,
            open,
            playing: heard_kids.iter().any(|k| k.playing),
            flash: heard_kids.iter().any(|k| k.secs < FLASH_SECS),
            secs: heard_kids.iter().map(|k| k.secs).fold(f32::INFINITY, f32::min),
            count: heard_kids.iter().map(|k| k.count).sum(),
            heard: heard_kids.iter().map(|k| k.id.clone()).collect(),
            volume: settings.group_percent(&g),
            own: settings.groups.contains_key(&g),
            inherited: false,
            cooldown: 0,
        };
        let at = settings.group_stamp(&g).max(changed_kids.iter().map(|id| settings.sound_stamp(id)).max().unwrap_or(0));
        let kid_rows = if open { kids.iter().map(|k| sound_row(k, settings, true)).collect() } else { Vec::new() };
        keyed.push((Key::Group(g), at, row, kid_rows));
    }
    for id in loose.into_iter().filter(|id| pass(id)) {
        let at = settings.sound_stamp(&id);
        keyed.push((Key::Sound(id.clone()), at, sound_row(&view(&id), settings, false), Vec::new()));
    }
    let mut rows = sorted(keyed);
    if let Some(id) = &st.drawer
        && let Some(i) = rows.iter().position(|r| r.key == Key::Sound(id.clone()))
    {
        let child = rows[i].child;
        rows.insert(i + 1, cooldown_row(id, child, settings));
    }
    rows
}

/// The All tab: every sound of the game, groups collapsed by default, sorted by name,
/// category or group size. Search and chips narrow it like the other tabs.
pub fn build_all(
    heard: &[HeardView],
    settings: &Settings,
    st: &ListState,
    catalog: &Catalog,
    matches: &dyn Fn(&str) -> bool,
    name: &dyn Fn(&Key) -> String,
) -> Vec<Row> {
    let info = by_id(heard);
    let source = |id: &str| info.get(id).map_or_else(|| guess_source(id), |h| h.source);
    let pass = |id: &str| matches(id) && (st.cats.is_empty() || st.cats.contains(&source(id)));
    let view = |id: &str| {
        info.get(id).map_or_else(
            || HeardView { id: id.to_owned(), secs: f32::INFINITY, count: 0, playing: false, source: guess_source(id) },
            |h| (*h).clone(),
        )
    };
    let cat_rank = |s: usize| CATEGORIES.iter().position(|(c, _)| *c == s).unwrap_or(CATEGORIES.len());
    // (key, category rank, size, row, kids)
    let mut keyed: Vec<(Key, usize, usize, Row, Vec<Row>)> = Vec::new();
    if st.flat {
        let mut ids: Vec<&String> = catalog.groups.values().flatten().chain(catalog.loose.iter()).collect();
        ids.sort();
        for id in ids.into_iter().filter(|id| pass(id)) {
            keyed.push((Key::Sound(id.clone()), cat_rank(source(id)), 1, sound_row(&view(id), settings, false), Vec::new()));
        }
    } else {
        for (g, all) in &catalog.groups {
            let kids: Vec<&String> = all.iter().filter(|id| pass(id)).collect();
            if kids.is_empty() {
                continue;
            }
            let views: Vec<HeardView> = kids.iter().map(|id| view(id)).collect();
            let heard_kids: Vec<&HeardView> = views.iter().filter(|k| k.count > 0).collect();
            let open = st.open_all.get(g).copied().unwrap_or(false);
            let row = Row {
                key: Key::Group(g.clone()),
                child: false,
                open,
                playing: heard_kids.iter().any(|k| k.playing),
                flash: heard_kids.iter().any(|k| k.secs < FLASH_SECS),
                secs: heard_kids.iter().map(|k| k.secs).fold(f32::INFINITY, f32::min),
                count: heard_kids.iter().map(|k| k.count).sum(),
                heard: heard_kids.iter().map(|k| k.id.clone()).collect(),
                volume: settings.group_percent(g),
                own: settings.groups.contains_key(g),
                inherited: false,
                cooldown: 0,
            };
            let rank = cat_rank(source(&all[0]));
            let kid_rows = if open { views.iter().map(|k| sound_row(k, settings, true)).collect() } else { Vec::new() };
            keyed.push((Key::Group(g.clone()), rank, all.len(), row, kid_rows));
        }
        for id in catalog.loose.iter().filter(|id| pass(id)) {
            keyed.push((Key::Sound(id.clone()), cat_rank(source(id)), 1, sound_row(&view(id), settings, false), Vec::new()));
        }
    }
    match st.sort_all {
        SortAll::Alpha => keyed.sort_by_cached_key(|(k, _, _, _, _)| (name(k).to_lowercase(), k.clone())),
        SortAll::Category => keyed.sort_by_cached_key(|(k, c, _, _, _)| (*c, name(k).to_lowercase(), k.clone())),
        SortAll::Size => {
            keyed.sort_by_cached_key(|(k, _, n, _, _)| (std::cmp::Reverse(*n), name(k).to_lowercase(), k.clone()))
        }
    }
    let mut rows = Vec::new();
    for (_, _, _, row, kids) in keyed {
        rows.push(row);
        rows.extend(kids);
    }
    if let Some(id) = &st.drawer
        && let Some(i) = rows.iter().position(|r| r.key == Key::Sound(id.clone()))
    {
        let child = rows[i].child;
        rows.insert(i + 1, cooldown_row(id, child, settings));
    }
    rows
}

/// The button under an expanded group: "N more in this group", or "heard only" while all are shown.
fn more_row(group: &str, rest: usize, showing_all: bool) -> Row {
    Row {
        key: Key::More(group.to_owned()),
        child: true,
        open: showing_all,
        playing: false,
        flash: false,
        secs: f32::INFINITY,
        count: rest as u32,
        heard: Vec::new(),
        volume: 100,
        own: false,
        inherited: false,
        cooldown: 0,
    }
}

fn sound_row(h: &HeardView, settings: &Settings, child: bool) -> Row {
    let volume = settings.volume_percent(&h.id);
    let own = settings.sounds.contains_key(&h.id);
    Row {
        key: Key::Sound(h.id.clone()),
        child,
        open: false,
        playing: h.playing,
        flash: h.secs < FLASH_SECS,
        secs: h.secs,
        count: h.count,
        heard: Vec::new(),
        volume,
        own,
        inherited: !own && volume < 100,
        cooldown: cooldown_secs(settings, &h.id),
    }
}

/// `[x, y, width, height]` in GUI pixels.
pub type Rect = [f32; 4];

pub fn contains(r: Rect, p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// Where the screen's parts are, for a GUI of `width` × `height`.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub panel: Rect,
    pub title: [f32; 2],
    pub search: Rect,
    /// The × inside the search field, shown while there is text.
    pub search_clear: Rect,
    pub close: Rect,
    pub tabs: Rect,
    /// Category chips in drawing order ([`ListState::chip_order`]).
    pub chips: Vec<Rect>,
    pub view_flat: Rect,
    pub view_group: Rect,
    /// Tab labels, clickable when ready.
    pub tab_rects: [Rect; 4],
    /// The sort button (All and Changed).
    pub sort: Option<Rect>,
    /// Sort menu items while the menu is open, under the sort button.
    pub sort_menu: Vec<Rect>,
    pub banner: Option<Rect>,
    pub list: Rect,
    pub freeze: Rect,
    /// The Freeze button exists (Recent only).
    pub show_freeze: bool,
    /// Changed tab footer: where its text starts, and its buttons.
    pub foot_text: [f32; 2],
    pub reset_all: Option<Rect>,
    pub confirm_yes: Option<Rect>,
    pub confirm_no: Option<Rect>,
}

/// Height of a chip and of the view buttons.
pub const CHIP_H: f32 = 14.0;

/// `m` holds measured text widths; chips wrap onto more lines when they do not fit.
pub fn layout(width: f32, height: f32, banner: bool, m: &Measure, st: &ListState) -> Layout {
    let chip_text = m.chips.as_slice();
    let w = (width - 24.0).clamp(300.0, 640.0).min(width);
    let h = (height - 24.0).max(160.0).min(height);
    let x = ((width - w) / 2.0).floor();
    let y = ((height - h) / 2.0).floor();
    // Inner margin of the panel.
    let pad = 8.0;
    let close = [x + w - pad - 16.0, y + 6.0, 16.0, 16.0];
    let search = [x + 76.0, y + 6.0, (close[0] - 8.0 - x - 76.0).max(40.0), 16.0];
    let search_clear = [search[0] + search[2] - 16.0, search[1], 16.0, 16.0];
    let tabs = [x + pad, y + 28.0, w - 2.0 * pad, 16.0];
    let tools_top = y + 52.0;
    let mut tab_rects = [[0.0; 4]; 4];
    let mut tx = tabs[0] + 2.0;
    for (i, tw) in m.tabs.iter().enumerate() {
        tab_rects[i] = [tx - 3.0, tabs[1], tw + 6.0, tabs[3]];
        tx += tw + 16.0;
    }
    let sorted_tab = matches!(st.tab, Tab::All | Tab::Changed);
    let sort = sorted_tab.then(|| [x + w - pad - 18.0, tools_top, 18.0, CHIP_H]);
    let right = sort.map_or(x + w - pad, |s| s[0] - 5.0);
    let view_group = [right - 18.0, tools_top, 18.0, CHIP_H];
    let view_flat = [view_group[0] - 18.0, tools_top, 18.0, CHIP_H];
    let chips_right = view_flat[0] - 10.0;
    let mut chips = Vec::with_capacity(chip_text.len());
    let (mut cx, mut cy) = (x + pad, tools_top);
    for tw in chip_text {
        let cw = tw + 14.0;
        if cx > x + pad && cx + cw > chips_right {
            cx = x + pad;
            cy += CHIP_H + 4.0;
        }
        chips.push([cx, cy, cw, CHIP_H]);
        cx += cw + 4.0;
    }
    let mut top = cy + CHIP_H + 8.0;
    let banner = banner.then(|| {
        let r = [x + pad, top, w - 2.0 * pad, 16.0];
        top += 19.0;
        r
    });
    let foot_y = y + h - pad - 20.0;
    let freeze = [x + w - pad - 92.0, foot_y, 92.0, 20.0];
    let mut sort_menu = Vec::new();
    if let (Some(s), true) = (sort, st.sort_open) {
        let mw = m.sort_items.iter().copied().fold(80.0, f32::max) + 18.0;
        for i in 0..m.sort_items.len() {
            sort_menu.push([s[0] + s[2] - mw, s[1] + s[3] + 4.0 + i as f32 * 16.0, mw, 16.0]);
        }
    }
    let changed = st.tab == Tab::Changed;
    let [reset_w, yes_w, no_w] = m.buttons.map(|b| b + 18.0);
    let right_edge = x + w - pad;
    let reset_all = (changed && !st.confirm_reset).then(|| [right_edge - reset_w, foot_y, reset_w, 20.0]);
    let confirm_no = (changed && st.confirm_reset).then(|| [right_edge - no_w, foot_y, no_w, 20.0]);
    let confirm_yes = confirm_no.map(|n| [n[0] - 5.0 - yes_w, foot_y, yes_w, 20.0]);
    Layout {
        panel: [x, y, w, h],
        title: [x + pad + 2.0, y + 10.0],
        search,
        search_clear,
        close,
        tabs,
        chips,
        view_flat,
        view_group,
        tab_rects,
        sort,
        sort_menu,
        banner,
        list: [x + pad, top, w - 2.0 * pad, (foot_y - 6.0 - top).max(0.0)],
        freeze,
        show_freeze: st.tab == Tab::Recent,
        foot_text: [x + pad + 2.0, foot_y + 6.0],
        reset_all,
        confirm_yes,
        confirm_no,
    }
}

/// The parts of one row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cols {
    /// Indicator: playing bars, the play icon on hover, a group's chevron.
    pub ind: Rect,
    pub name: Rect,
    pub when: Rect,
    pub speaker: Rect,
    pub slider: Rect,
    pub pct: Rect,
    pub reset: Rect,
    /// Sound rows: opens the "less often" drawer.
    pub timer: Rect,
}

/// Row parts, right to left. The slider grows with the screen.
pub fn columns(row: Rect, child: bool) -> Cols {
    let [x, y, w, h] = row;
    let timer = [x + w - 20.0, y, 14.0, h];
    let reset = [timer[0] - 16.0, y, 14.0, h];
    let pct = [reset[0] - 30.0, y, 28.0, h];
    let slider_w = (w * 0.28).clamp(70.0, 170.0);
    let slider = [pct[0] - 4.0 - slider_w, y, slider_w, h];
    let speaker = [slider[0] - 18.0, y, 14.0, h];
    let when = [speaker[0] - 42.0, y, 38.0, h];
    let left = x + 6.0 + if child { CHILD_INDENT } else { 0.0 };
    let ind = [left, y, 12.0, h];
    let name_x = left + 16.0;
    let name = [name_x, y, (when[0] - 4.0 - name_x).max(0.0), h];
    Cols {
        ind,
        name,
        when,
        speaker,
        slider,
        pct,
        reset,
        timer,
    }
}

/// Height of all rows.
pub fn content_height(rows: usize) -> f32 {
    rows as f32 * (ROW_H + ROW_GAP)
}

pub fn max_scroll(rows: usize, list: Rect) -> f32 {
    (content_height(rows) - list[3]).max(0.0)
}

/// Rows fully inside the list, with their rectangles.
pub fn visible(rows: usize, list: Rect, scroll: f32) -> Vec<(usize, Rect)> {
    let mut out = Vec::new();
    for i in 0..rows {
        let top = list[1] + i as f32 * (ROW_H + ROW_GAP) - scroll;
        if top < list[1] - 0.5 {
            continue;
        }
        if top + ROW_H > list[1] + list[3] + 0.5 {
            break;
        }
        out.push((i, [list[0], top, list[2], ROW_H]));
    }
    out
}

/// Slider value for a pointer at `x`, in steps of 5.
pub fn slider_value(slider: Rect, x: f32) -> u8 {
    let t = ((x - slider[0] - 2.0) / (slider[2] - 4.0)).clamp(0.0, 1.0);
    ((t * 20.0).round() * 5.0) as u8
}

/// What a click at a point hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Close,
    Search,
    SearchClear,
    /// A chip by its drawing position.
    Chip(usize),
    ViewFlat,
    ViewGroup,
    Tab(usize),
    Sort,
    SortItem(usize),
    /// A click outside the open sort menu: it only closes the menu.
    SortClose,
    ResetAll,
    ConfirmYes,
    ConfirmNo,
    /// A sound row's timer: open or close its drawer.
    Timer(usize),
    /// A drawer's slider, with the cooldown step under the pointer.
    CooldownSlider(usize, usize),
    CooldownReset(usize),
    Freeze,
    Banner,
    Speaker(usize),
    Slider(usize, u8),
    Reset(usize),
    /// The free part of a row: preview a sound or toggle a group.
    Row(usize),
    Nothing,
}

pub fn hit(lay: &Layout, rows: &[Row], scroll: f32, p: [f32; 2]) -> Hit {
    if let Some(i) = lay.sort_menu.iter().position(|r| contains(*r, p)) {
        return Hit::SortItem(i);
    }
    if lay.sort.is_some_and(|s| contains(s, p)) {
        return Hit::Sort;
    }
    if !lay.sort_menu.is_empty() {
        return Hit::SortClose;
    }
    if contains(lay.close, p) {
        return Hit::Close;
    }
    if let Some(i) = lay.tab_rects.iter().position(|r| contains(*r, p)) {
        return Hit::Tab(i);
    }
    if lay.reset_all.is_some_and(|r| contains(r, p)) {
        return Hit::ResetAll;
    }
    if lay.confirm_yes.is_some_and(|r| contains(r, p)) {
        return Hit::ConfirmYes;
    }
    if lay.confirm_no.is_some_and(|r| contains(r, p)) {
        return Hit::ConfirmNo;
    }
    if contains(lay.search_clear, p) {
        return Hit::SearchClear;
    }
    if contains(lay.search, p) {
        return Hit::Search;
    }
    if let Some(i) = lay.chips.iter().position(|c| contains(*c, p)) {
        return Hit::Chip(i);
    }
    if contains(lay.view_flat, p) {
        return Hit::ViewFlat;
    }
    if contains(lay.view_group, p) {
        return Hit::ViewGroup;
    }
    if lay.show_freeze && contains(lay.freeze, p) {
        return Hit::Freeze;
    }
    if lay.banner.is_some_and(|b| contains(b, p)) {
        return Hit::Banner;
    }
    for (i, r) in visible(rows.len(), lay.list, scroll) {
        if !contains(r, p) {
            continue;
        }
        if matches!(rows[i].key, Key::More(_)) {
            return Hit::Row(i);
        }
        let c = columns(r, rows[i].child);
        if matches!(rows[i].key, Key::Cooldown(_)) {
            return if contains(c.slider, p) {
                Hit::CooldownSlider(i, rate_value(c.slider, p[0]))
            } else if contains(c.reset, p) && rows[i].cooldown > 0 {
                Hit::CooldownReset(i)
            } else {
                Hit::Nothing
            };
        }
        if contains(c.timer, p) {
            return if matches!(rows[i].key, Key::Sound(_)) { Hit::Timer(i) } else { Hit::Nothing };
        }
        return if contains(c.speaker, p) {
            Hit::Speaker(i)
        } else if contains(c.slider, p) {
            Hit::Slider(i, slider_value(c.slider, p[0]))
        } else if contains(c.reset, p) && rows[i].own {
            Hit::Reset(i)
        } else if contains(c.pct, p) || contains(c.reset, p) {
            Hit::Nothing
        } else {
            Hit::Row(i)
        };
    }
    Hit::Nothing
}

/// `now`, `5s`, `2m`, or `playing`, in the language `lang`.
pub fn when_text(row: &Row, lang: &str) -> String {
    use crate::i18n::{text, with_n};
    if row.playing {
        return text(lang, "when.playing").to_owned();
    }
    if !row.secs.is_finite() {
        return String::new();
    }
    let s = row.secs.max(0.0);
    if s < 2.0 {
        text(lang, "when.now").to_owned()
    } else if s < 60.0 {
        with_n(lang, "when.seconds", s as u32)
    } else {
        with_n(lang, "when.minutes", (s / 60.0) as u32)
    }
}

/// A rounded rectangle as non-overlapping pieces: three rectangles and, per
/// corner, a fan of triangles (each a quad with its last point repeated), so a
/// translucent fill is not drawn twice anywhere.
pub fn rounded(r: Rect, radius: f32) -> (Vec<Rect>, Vec<[[f32; 2]; 4]>) {
    let [x, y, w, h] = r;
    let rad = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    if rad < 0.5 {
        return (vec![r], Vec::new());
    }
    let rects = vec![
        [x, y + rad, w, h - 2.0 * rad],
        [x + rad, y, w - 2.0 * rad, rad],
        [x + rad, y + h - rad, w - 2.0 * rad, rad],
    ];
    const STEPS: usize = 6;
    let corners = [
        ([x + rad, y + rad], 180.0f32),
        ([x + w - rad, y + rad], 270.0),
        ([x + w - rad, y + h - rad], 0.0),
        ([x + rad, y + h - rad], 90.0),
    ];
    let mut tris = Vec::with_capacity(4 * STEPS);
    for (c, start) in corners {
        for i in 0..STEPS {
            let a0 = (start + 90.0 * i as f32 / STEPS as f32).to_radians();
            let a1 = (start + 90.0 * (i + 1) as f32 / STEPS as f32).to_radians();
            let p0 = [c[0] + rad * a0.cos(), c[1] + rad * a0.sin()];
            let p1 = [c[0] + rad * a1.cos(), c[1] + rad * a1.sin()];
            tris.push([c, p0, p1, p1]);
        }
    }
    (rects, tris)
}

/// Exponential approach of `current` to `target` over `dt` seconds: frame-rate independent easing.
pub fn approach(current: f32, target: f32, dt: f32, rate: f32) -> f32 {
    let k = 1.0 - (-dt.max(0.0) * rate).exp();
    let next = current + (target - current) * k;
    if (target - next).abs() < 0.05 { target } else { next }
}

/// `argb` with its alpha multiplied by `f` (0 to 1).
pub fn fade(argb: u32, f: f32) -> u32 {
    let a = ((argb >> 24) as f32 * f.clamp(0.0, 1.0)).round() as u32;
    (a << 24) | (argb & 0x00FF_FFFF)
}

/// Channel-wise blend of two ARGB colours, `t` = 0 gives `a`.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 0xFF) as f32, ((b >> s) & 0xFF) as f32);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(24) | ch(16) | ch(8) | ch(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(id: &str, secs: f32, playing: bool) -> HeardView {
        HeardView {
            id: id.to_owned(),
            secs,
            count: 1,
            playing,
            source: 6,
        }
    }

    fn keys(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match &r.key {
                Key::Group(g) => format!("G:{g}"),
                Key::Sound(s) => format!("{}{s}", if r.child { "  " } else { "S:" }),
                Key::More(g) => format!("M:{g}"),
                Key::Cooldown(s) => format!("K:{s}"),
            })
            .collect()
    }

    #[test]
    fn sounds_are_grouped_and_playing_groups_come_first_collapsed() {
        let heard = [
            h("entity.chicken.ambient", 0.5, false),
            h("entity.chicken.step", 3.0, false),
            h("weather.rain", 40.0, true),
            h("entity.villager.ambient", 2.0, false),
            h("item.nautilus_saddle_equip", 5.0, false),
        ];
        let (rows, fresh) = build(&heard, &Settings::default(), &ListState::default(), &Catalog::default(), &|_| true);
        assert_eq!(fresh, 0);
        assert_eq!(
            keys(&rows),
            [
                "G:weather",
                "G:entity.chicken",
                "  entity.chicken.ambient",
                "  entity.chicken.step",
                "G:entity.villager",
                "  entity.villager.ambient",
                "S:item.nautilus_saddle_equip",
            ]
        );
        assert!(!rows[0].open && rows[0].playing);
        assert!(rows[1].open && rows[1].flash);
        assert_eq!(rows[1].count, 2);
    }

    #[test]
    fn live_order_refreshes_on_its_own_schedule_and_never_counts_new_sounds() {
        let before = [h("entity.cow.ambient", 1.0, false), h("entity.chicken.ambient", 5.0, false)];
        let st = ListState {
            live: Some(snapshot(&before, false)),
            ..ListState::default()
        };
        let after = [h("entity.chicken.ambient", 0.2, false), h("entity.cow.ambient", 3.0, false)];
        let (rows, fresh) = build(&after, &Settings::default(), &st, &Catalog::default(), &|_| true);
        assert_eq!(keys(&rows)[0], "G:entity.cow");
        assert_eq!(fresh, 0);
    }

    #[test]
    fn constant_families_start_collapsed() {
        let heard = [h("music.overworld.forest", 3.0, false), h("block.portal.ambient", 1.0, false)];
        let (rows, _) = build(&heard, &Settings::default(), &ListState::default(), &Catalog::default(), &|_| true);
        assert_eq!(keys(&rows), ["G:block.portal", "  block.portal.ambient", "G:music"]);
    }

    #[test]
    fn player_choice_overrides_default_expansion() {
        let heard = [h("weather.rain", 0.5, true), h("entity.cow.ambient", 1.0, false)];
        let mut st = ListState::default();
        st.open.insert("weather".into(), true);
        st.open.insert("entity.cow".into(), false);
        let (rows, _) = build(&heard, &Settings::default(), &st, &Catalog::default(), &|_| true);
        assert_eq!(keys(&rows), ["G:weather", "  weather.rain", "G:entity.cow"]);
    }

    #[test]
    fn volumes_show_own_and_inherited_values() {
        let heard = [h("entity.chicken.ambient", 1.0, false), h("entity.chicken.egg", 2.0, false)];
        let s = Settings::parse("group entity.chicken 30\nsound entity.chicken.egg 50\n");
        let (rows, _) = build(&heard, &s, &ListState::default(), &Catalog::default(), &|_| true);
        assert_eq!((rows[0].volume, rows[0].own), (30, true));
        assert_eq!((rows[1].volume, rows[1].own, rows[1].inherited), (30, false, true));
        assert_eq!((rows[2].volume, rows[2].own, rows[2].inherited), (50, true, false));
    }

    #[test]
    fn frozen_list_keeps_order_and_counts_new_sounds() {
        let before = [h("entity.cow.ambient", 1.0, false), h("entity.chicken.ambient", 5.0, false)];
        let mut st = ListState {
            frozen: true,
            snapshot: Some(snapshot(&before, false)),
            ..ListState::default()
        };
        // Chickens play again, a chick and a zombie are new.
        let after = [
            h("entity.chicken.ambient", 0.2, false),
            h("entity.cow.ambient", 3.0, false),
            h("entity.chicken.step", 0.5, false),
            h("entity.zombie.ambient", 0.1, false),
        ];
        let (rows, fresh) = build(&after, &Settings::default(), &st, &Catalog::default(), &|_| true);
        assert_eq!(
            keys(&rows),
            ["G:entity.cow", "  entity.cow.ambient", "G:entity.chicken", "  entity.chicken.ambient"]
        );
        assert_eq!(fresh, 2);
        // The banner applies them once; the list stays frozen.
        st.snapshot = Some(snapshot(&after, false));
        let (rows, fresh) = build(&after, &Settings::default(), &st, &Catalog::default(), &|_| true);
        assert_eq!(fresh, 0);
        assert_eq!(keys(&rows)[0], "G:entity.zombie");
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn layout_parts_do_not_overlap() {
        for (w, h) in [(427.0, 240.0), (640.0, 360.0), (960.0, 540.0)] {
            let lay = layout(w, h, true, &Measure::default(), &ListState::default());
            assert!(lay.panel[0] >= 0.0 && lay.panel[0] + lay.panel[2] <= w);
            let b = lay.banner.unwrap();
            assert!(b[1] + b[3] <= lay.list[1]);
            assert!(lay.list[1] + lay.list[3] <= lay.freeze[1]);
            for child in [false, true] {
                let c = columns([lay.list[0], 0.0, lay.list[2], ROW_H], child);
                let order = [c.ind, c.name, c.when, c.speaker, c.slider, c.pct, c.reset, c.timer];
                for pair in order.windows(2) {
                    assert!(pair[0][0] + pair[0][2] <= pair[1][0] + 0.01, "{w}: {pair:?}");
                }
                assert!(c.name[2] > 40.0, "name column too narrow at {w}");
            }
        }
        let wide = columns([0.0, 0.0, 600.0, ROW_H], false);
        let narrow = columns([0.0, 0.0, 300.0, ROW_H], false);
        assert!(wide.slider[2] > narrow.slider[2]);
    }

    #[test]
    fn clicks_hit_the_part_under_the_pointer() {
        let heard = [h("entity.chicken.ambient", 1.0, false), h("entity.chicken.egg", 2.0, false)];
        let s = Settings::parse("sound entity.chicken.egg 50\n");
        let (rows, _) = build(&heard, &s, &ListState::default(), &Catalog::default(), &|_| true);
        let lay = layout(640.0, 360.0, false, &Measure::default(), &ListState::default());
        let vis = visible(rows.len(), lay.list, 0.0);
        let mid = |r: Rect| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
        let c1 = columns(vis[1].1, true);
        let c2 = columns(vis[2].1, true);
        assert_eq!(hit(&lay, &rows, 0.0, mid(c1.speaker)), Hit::Speaker(1));
        assert_eq!(hit(&lay, &rows, 0.0, mid(c1.name)), Hit::Row(1));
        assert_eq!(hit(&lay, &rows, 0.0, mid(c2.reset)), Hit::Reset(2));
        // No reset on a row without its own value.
        assert_eq!(hit(&lay, &rows, 0.0, mid(c1.reset)), Hit::Nothing);
        assert_eq!(hit(&lay, &rows, 0.0, [c1.slider[0] + 1.0, mid(c1.slider)[1]]), Hit::Slider(1, 0));
        assert_eq!(
            hit(&lay, &rows, 0.0, [c1.slider[0] + c1.slider[2] - 1.0, mid(c1.slider)[1]]),
            Hit::Slider(1, 100)
        );
        assert_eq!(hit(&lay, &rows, 0.0, mid(lay.freeze)), Hit::Freeze);
        assert_eq!(hit(&lay, &rows, 0.0, mid(lay.close)), Hit::Close);
    }

    #[test]
    fn only_rows_fully_inside_the_list_are_visible() {
        let list = [0.0, 100.0, 300.0, 3.5 * (ROW_H + ROW_GAP)];
        assert_eq!(visible(10, list, 0.0).len(), 3);
        let v = visible(10, list, 2.0 * (ROW_H + ROW_GAP));
        assert_eq!(v[0].0, 2);
        assert!((v[0].1[1] - 100.0).abs() < 0.01);
        assert_eq!(max_scroll(3, list), 0.0);
        assert!(max_scroll(10, list) > 0.0);
    }

    #[test]
    fn slider_snaps_to_steps_of_five() {
        let r = [10.0, 0.0, 104.0, ROW_H];
        assert_eq!(slider_value(r, 0.0), 0);
        assert_eq!(slider_value(r, 62.0), 50);
        assert_eq!(slider_value(r, 500.0), 100);
        assert_eq!(slider_value(r, 12.0 + 100.0 * 0.31), 30);
    }

    fn tri_area(q: &[[f32; 2]; 4]) -> f32 {
        let [a, b, c, _] = *q;
        ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
    }

    #[test]
    fn rounded_pieces_cover_the_rectangle_minus_corners_once() {
        let (rects, tris) = rounded([10.0, 20.0, 100.0, 22.0], 4.0);
        let rect_area: f32 = rects.iter().map(|r| r[2] * r[3]).sum();
        let fan_area: f32 = tris.iter().map(tri_area).sum();
        let ideal = 100.0 * 22.0 - (4.0 - std::f32::consts::PI) * 16.0;
        assert!((rect_area + fan_area - ideal).abs() < 1.0, "{}", rect_area + fan_area);
        for q in &tris {
            for p in q {
                assert!(p[0] >= 10.0 - 1e-3 && p[0] <= 110.0 + 1e-3 && p[1] >= 20.0 - 1e-3 && p[1] <= 42.0 + 1e-3);
            }
        }
        // Radius never exceeds half the short side; zero radius is a plain rectangle.
        let (r2, _) = rounded([0.0, 0.0, 10.0, 4.0], 9.0);
        assert!(r2.iter().all(|r| r[2] >= 0.0 && r[3] >= 0.0));
        assert_eq!(rounded([0.0, 0.0, 5.0, 5.0], 0.0), (vec![[0.0, 0.0, 5.0, 5.0]], Vec::new()));
    }

    #[test]
    fn easing_reaches_the_target_independent_of_frame_rate() {
        let mut slow = 0.0;
        for _ in 0..10 {
            slow = approach(slow, 100.0, 0.05, 14.0);
        }
        let mut fast = 0.0;
        for _ in 0..40 {
            fast = approach(fast, 100.0, 0.0125, 14.0);
        }
        assert!((slow - fast).abs() < 1.0);
        assert_eq!(approach(99.97, 100.0, 0.016, 14.0), 100.0);
        assert_eq!(approach(5.0, 100.0, 0.0, 14.0), 5.0);
    }

    #[test]
    fn colour_helpers_scale_alpha_and_blend() {
        assert_eq!(fade(0x80_11_22_33, 0.5), 0x40_11_22_33);
        assert_eq!(fade(0xFF_11_22_33, 2.0), 0xFF_11_22_33);
        assert_eq!(mix(0xFF_00_00_00, 0xFF_FF_FF_FF, 0.0), 0xFF_00_00_00);
        assert_eq!(mix(0xFF_00_00_00, 0xFF_FF_FF_FF, 1.0), 0xFF_FF_FF_FF);
        assert_eq!(mix(0x00_00_00_00, 0xFF_FF_00_00, 0.5), 0x80_80_00_00);
    }

    #[test]
    fn expanded_group_offers_its_unheard_sounds() {
        let heard = [h("entity.cow.ambient", 1.0, false)];
        let catalog = Catalog::from_ids(["entity.cow.ambient", "minecraft:entity.cow.hurt", "entity.cow.step", "ui.button.click"]);
        let (rows, _) = build(&heard, &Settings::default(), &ListState::default(), &catalog, &|_| true);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].key, Key::More("entity.cow".into()));
        assert_eq!((rows[2].count, rows[2].open), (2, false));

        let mut st = ListState::default();
        st.show_all.insert("entity.cow".into());
        let (rows, _) = build(&heard, &Settings::default(), &st, &catalog, &|_| true);
        assert_eq!(keys(&rows), ["G:entity.cow", "  entity.cow.ambient", "  entity.cow.hurt", "  entity.cow.step", "M:entity.cow"]);
        assert!(rows[4].open);
        assert_eq!(when_text(&rows[2], "en_us"), "");
        assert_eq!(rows[2].count, 0);
    }

    #[test]
    fn chips_filter_by_category_and_search_by_id() {
        let mut rain = h("weather.rain", 0.5, false);
        rain.source = 3;
        let heard = [rain, h("entity.cow.ambient", 1.0, false), h("entity.chicken.ambient", 2.0, false)];
        let mut st = ListState::default();
        st.toggle_cat(3);
        let (rows, _) = build(&heard, &Settings::default(), &st, &Catalog::default(), &|_| true);
        assert_eq!(keys(&rows), ["G:weather"]);
        assert_eq!(st.chip_order()[0], 3);
        st.toggle_cat(3);
        assert!(!st.filtering());
        let (rows, _) = build(&heard, &Settings::default(), &st, &Catalog::default(), &|id| id.contains("chicken"));
        assert_eq!(keys(&rows), ["G:entity.chicken", "  entity.chicken.ambient"]);
    }

    #[test]
    fn flat_view_lists_sounds_without_groups() {
        let heard = [h("entity.cow.ambient", 1.0, false), h("entity.cow.step", 0.5, false), h("ui.button.click", 2.0, false)];
        let st = ListState { flat: true, ..ListState::default() };
        let (rows, _) = build(&heard, &Settings::default(), &st, &Catalog::default(), &|_| true);
        assert_eq!(keys(&rows), ["S:entity.cow.step", "S:entity.cow.ambient", "S:ui.button.click"]);
    }

    #[test]
    fn chips_wrap_when_the_row_is_full() {
        let lay = layout(640.0, 360.0, false, &Measure::chips(vec![40.0; 9]), &ListState::default());
        assert_eq!(lay.chips.len(), 9);
        assert!(lay.chips.iter().all(|c| c[0] + c[2] <= lay.view_flat[0]));
        let narrow = layout(320.0, 360.0, false, &Measure::chips(vec![40.0; 9]), &ListState::default());
        assert!(narrow.chips[8][1] > narrow.chips[0][1]);
        assert!(narrow.list[1] > lay.list[1]);
        assert_eq!(hit(&lay, &[], 0.0, [lay.chips[2][0] + 1.0, lay.chips[2][1] + 1.0]), Hit::Chip(2));
    }

    #[test]
    fn open_drawer_sits_under_its_sound_and_its_parts_are_hit() {
        let heard = [h("entity.cow.ambient", 1.0, false), h("entity.cow.step", 2.0, false)];
        let s = Settings::parse("cooldown entity.cow.step 30\n");
        let st = ListState { drawer: Some("entity.cow.step".into()), ..ListState::default() };
        let (rows, _) = build(&heard, &s, &st, &Catalog::default(), &|_| true);
        assert_eq!(keys(&rows), ["G:entity.cow", "  entity.cow.ambient", "  entity.cow.step", "K:entity.cow.step"]);
        assert_eq!((rows[2].cooldown, rows[3].cooldown), (30, 30));
        let lay = layout(640.0, 360.0, false, &Measure::default(), &ListState::default());
        let vis = visible(rows.len(), lay.list, 0.0);
        let mid = |r: Rect| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
        let c = columns(vis[3].1, true);
        assert_eq!(hit(&lay, &rows, 0.0, mid(c.reset)), Hit::CooldownReset(3));
        assert_eq!(hit(&lay, &rows, 0.0, [c.slider[0] + c.slider[2] - 1.0, mid(c.slider)[1]]), Hit::CooldownSlider(3, 15));
        assert_eq!(hit(&lay, &rows, 0.0, mid(columns(vis[1].1, true).timer)), Hit::Timer(1));
        assert_eq!(hit(&lay, &rows, 0.0, mid(columns(vis[0].1, false).timer)), Hit::Nothing);
    }

    #[test]
    fn cooldown_steps_and_text() {
        assert_eq!(rate_index(30), 8);
        assert_eq!(rate_index(31), 9);
        assert_eq!(rate_index(9999), 15);
        assert_eq!(RATES[rate_index(0)], 0);
        assert_eq!(rate_text(30, "en_us"), "30 s");
        assert_eq!(rate_text(90, "en_us"), "1.5 min");
        assert_eq!(rate_text(120, "en_us"), "2 min");
        assert_eq!(cooldown_text(0, "en_us"), "default");
        assert_eq!(cooldown_text(45, "en_us"), "45 s cooldown");
        assert!(rate_text(90, "ru_ru").contains("1,5"));
    }

    #[test]
    fn changed_tab_lists_set_groups_with_all_their_sounds_and_sorts() {
        let s = Settings::parse("group entity.cow 40 100\nsound entity.chicken.egg 0 300\ncooldown ui.toast.in 10 200\n");
        let catalog = Catalog::from_ids(["entity.cow.ambient", "entity.cow.step", "entity.chicken.egg", "entity.chicken.ambient"]);
        let heard = [h("entity.cow.step", 3.0, false)];
        let st = ListState { tab: Tab::Changed, ..ListState::default() };
        let name = |k: &Key| match k {
            Key::Group(g) | Key::Sound(g) | Key::More(g) | Key::Cooldown(g) => g.clone(),
        };
        let rows = build_changed(&heard, &s, &st, &catalog, &|_| true, &name);
        assert_eq!(
            keys(&rows),
            ["G:entity.chicken", "  entity.chicken.ambient", "  entity.chicken.egg", "G:entity.cow", "  entity.cow.ambient", "  entity.cow.step", "G:ui", "  ui.toast.in"]
        );
        assert!(rows[3].own && rows[4].inherited);
        assert_eq!(rows[5].count, 1);
        let recent = ListState { sort_changed: SortChanged::Recent, ..st.clone() };
        let rows = build_changed(&heard, &s, &recent, &catalog, &|_| true, &name);
        assert_eq!(rows[0].key, Key::Group("entity.chicken".into()));
        assert_eq!(rows[rows.len() - 3].key, Key::Group("entity.cow".into()));
        let flat = ListState { flat: true, ..st.clone() };
        let rows = build_changed(&heard, &s, &flat, &catalog, &|_| true, &name);
        assert_eq!(keys(&rows), ["S:entity.chicken.egg", "S:entity.cow.ambient", "S:entity.cow.step", "S:ui.toast.in"]);
    }

    #[test]
    fn tabs_sort_menu_and_footer_are_hit() {
        let m = Measure { tabs: [40.0; 4], sort_items: vec![50.0, 60.0], buttons: [50.0, 40.0, 40.0], ..Measure::default() };
        let mid = |r: Rect| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
        let recent = layout(640.0, 360.0, false, &m, &ListState::default());
        assert!(recent.sort.is_none() && recent.reset_all.is_none() && recent.show_freeze);
        assert_eq!(hit(&recent, &[], 0.0, mid(recent.tab_rects[2])), Hit::Tab(2));
        let mut st = ListState { tab: Tab::Changed, ..ListState::default() };
        let lay = layout(640.0, 360.0, false, &m, &st);
        assert!(!lay.show_freeze);
        assert_eq!(hit(&lay, &[], 0.0, mid(lay.sort.unwrap())), Hit::Sort);
        assert_eq!(hit(&lay, &[], 0.0, mid(lay.reset_all.unwrap())), Hit::ResetAll);
        assert!(lay.view_group[0] + lay.view_group[2] <= lay.sort.unwrap()[0]);
        st.sort_open = true;
        let lay = layout(640.0, 360.0, false, &m, &st);
        assert_eq!(lay.sort_menu.len(), 2);
        assert_eq!(hit(&lay, &[], 0.0, mid(lay.sort_menu[1])), Hit::SortItem(1));
        assert_eq!(hit(&lay, &[], 0.0, mid(lay.close)), Hit::SortClose);
        st.sort_open = false;
        st.confirm_reset = true;
        let lay = layout(640.0, 360.0, false, &m, &st);
        assert_eq!(hit(&lay, &[], 0.0, mid(lay.confirm_yes.unwrap())), Hit::ConfirmYes);
        assert_eq!(hit(&lay, &[], 0.0, mid(lay.confirm_no.unwrap())), Hit::ConfirmNo);
    }

    #[test]
    fn all_tab_lists_every_sound_collapsed_and_sorts() {
        let catalog = Catalog::from_ids([
            "entity.cow.ambient", "entity.cow.step", "entity.zombie.ambient", "block.stone.break", "block.stone.step",
            "block.stone.hit", "ambient.cave",
        ]);
        assert_eq!(catalog.total, 7);
        let st = ListState { tab: Tab::All, ..ListState::default() };
        let name = |k: &Key| match k {
            Key::Group(g) | Key::Sound(g) | Key::More(g) | Key::Cooldown(g) => g.clone(),
        };
        let rows = build_all(&[], &Settings::default(), &st, &catalog, &|_| true, &name);
        assert_eq!(keys(&rows), ["G:ambient", "G:block.stone", "G:entity.cow", "G:entity.zombie"]);
        let mut open = st.clone();
        open.open_all.insert("entity.cow".into(), true);
        open.sort_all = SortAll::Size;
        let rows = build_all(&[], &Settings::default(), &open, &catalog, &|_| true, &name);
        assert_eq!(keys(&rows)[..4], ["G:block.stone", "G:entity.cow", "  entity.cow.ambient", "  entity.cow.step"]);
        let mut hostile = st.clone();
        hostile.cats = vec![5];
        let rows = build_all(&[], &Settings::default(), &hostile, &catalog, &|_| true, &name);
        assert_eq!(keys(&rows), ["G:entity.zombie"]);
        let by_cat = ListState { sort_all: SortAll::Category, ..st.clone() };
        let rows = build_all(&[], &Settings::default(), &by_cat, &catalog, &|_| true, &name);
        assert_eq!(keys(&rows), ["G:block.stone", "G:entity.cow", "G:entity.zombie", "G:ambient"]);
    }

    #[test]
    fn guessed_categories() {
        assert_eq!(guess_source("minecraft:entity.zombie.ambient"), 5);
        assert_eq!(guess_source("entity.cow.ambient"), 6);
        assert_eq!(guess_source("entity.player.hurt"), 7);
        assert_eq!(guess_source("block.note_block.harp"), 2);
        assert_eq!(guess_source("music_disc.cat"), 2);
        assert_eq!(guess_source("ambient.cave"), 8);
    }

    #[test]
    fn when_text_reads_naturally() {
        let mut r = Row {
            key: Key::Sound("a.b.c".into()),
            child: false,
            open: false,
            playing: false,
            flash: false,
            secs: 0.5,
            count: 1,
            heard: Vec::new(),
            volume: 100,
            own: false,
            inherited: false,
            cooldown: 0,
        };
        assert_eq!(when_text(&r, "en_us"), "now");
        r.secs = 12.4;
        assert_eq!(when_text(&r, "en_us"), "12s");
        assert_ne!(when_text(&r, "ru_ru"), "12s");
        r.secs = 130.0;
        assert_eq!(when_text(&r, "en_us"), "2m");
        r.playing = true;
        assert_eq!(when_text(&r, "en_us"), "playing");
    }
}
