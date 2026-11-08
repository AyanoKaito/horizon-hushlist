//! Hushlist trait mod: per-sound volume with a live list of recent sounds.
//! The launcher links this crate into the game and calls `hooks()`.
mod core;
mod i18n;
mod ui;

use crate::core::{Engine, MOD_ID, MOD_VERSION, SETTINGS_FILE, Settings, group_key, normalize, nudged};
use crate::ui::{Hit, HeardView, Key, ListState, Rect, Row, SortAll, SortChanged, Tab, approach, fade, mix};
use horizon_client::lang;
use horizon_client::mod_api::{
    ClientTickPostEvent, Flow, Hooks, Host, HudBatch, KeyEvent, ModsEvent, NativeMod,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent,
    ScreenCharTypedPreEvent, ScreenKeyPressedPreEvent, ScreenMouseScrolledPreEvent, ScreenRenderPostEvent,
    SettingsClickEvent, SettingsEvent, SoundPlayEvent,
};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

const OPEN_KEY: &str = "open";
const OPEN_KEY_DEFAULT: &str = "key.keyboard.h";
/// Saved this long after the last change, so a slider drag writes once.
const SAVE_DELAY: Duration = Duration::from_millis(400);
/// How long the nudged category volumes stay before they are restored.
const NUDGE_HOLD: Duration = Duration::from_millis(150);
/// How often the settings file is checked for edits made outside the game.
const RELOAD_EVERY: Duration = Duration::from_secs(1);
/// How often the live list may change its order.
const LIVE_REORDER: Duration = Duration::from_secs(1);

// Colours: fill takes ARGB, text takes RGB.
const BG: u32 = 0xF2_14_13_19;
const EDGE: u32 = 0x24_FF_FF_FF;
const ROW: u32 = 0x0D_FF_FF_FF;
const ROW_HOVER: u32 = 0x1A_FF_FF_FF;
const ROW_LONG: u32 = 0x14_F5_B1_42;
const ROW_FLASH: u32 = 0x3D_F5_B1_42;
const TRACK: u32 = 0x33_FF_FF_FF;
const THUMB: u32 = 0xFF_EE_EC_F4;
const BANNER: u32 = 0xFF_3A_30_20;
const FREEZE_ON: u32 = 0xFF_CF_EF_FF;
const FREEZE_OFF: u32 = 0xFF_2C_2B_33;
const TEXT: u32 = 0xEE_EC_F4;
const DIM: u32 = 0x9D_99_AD;
const FAINT: u32 = 0x6B_67_79;
const AMBER: u32 = 0xF5_B1_42;
const OWN: u32 = 0x6F_F0_DA;
const INH: u32 = 0x4E_7A_75;
const ICE_TEXT: u32 = 0x12_30_3F;
const FIELD: u32 = 0xFF_0E_0D_12;
const FIELD_EDGE_FOCUS: u32 = 0x55_FF_FF_FF;
const CHIP_ON_TEXT: u32 = 0x17_15_1E;
const DANGER: u32 = 0xFF_8A_7A;
const MENU: u32 = 0xFC_22_20_2A;

// Icon sheet (tools/make_icons.py): one column per icon, one row per colour,
// 24 px cells drawn into 12 GUI pixels. Uploaded into the mods' image slot.
const ICON_SHEET: &[u8] = include_bytes!("../data/icons.png");
const ICON_CELL: f32 = 24.0;
const ICON_SIZE: f32 = 12.0;
const MOD_IMAGE: &str = "horizon/modimage";

#[derive(Clone, Copy)]
enum Icon {
    SpeakerHigh = 0,
    SpeakerLow = 1,
    SpeakerOff = 2,
    Reset = 3,
    Timer = 4,
    Play = 5,
    Snowflake = 6,
    Close = 7,
    ChevronRight = 8,
    ChevronDown = 9,
    ViewList = 10,
    ViewGroups = 11,
    Sort = 12,
    Search = 13,
}

#[derive(Clone, Copy)]
enum Tone {
    Text = 0,
    Dim = 1,
    Faint = 2,
    Own = 3,
    Inherited = 4,
    IceText = 6,
}

pub fn hooks() -> Box<dyn Hooks> {
    Box::new(Hushlist::default())
}

#[derive(Default)]
struct Hushlist {
    state: Mutex<State>,
}

#[derive(Default)]
enum Refresh {
    #[default]
    Idle,
    Pending,
    /// Category volumes are nudged; the originals to restore and the nudge phase.
    Nudged(Instant, [f32; 11], bool),
    /// A change arrived while nudged: nudge again from the same originals.
    Again([f32; 11], bool),
}

#[derive(Default)]
struct State {
    engine: Engine,
    list: ListState,
    path: Option<PathBuf>,
    loaded: bool,
    modified: Option<SystemTime>,
    next_reload: Option<Instant>,
    dirty_at: Option<Instant>,
    refresh: Refresh,
    close: bool,
    /// The Mods page asks to confirm "Reset all sound settings".
    page_confirm: bool,
    preview: Option<String>,
    drag: Option<Key>,
    trace: Option<std::fs::File>,
    trace_opened: bool,
    started: Option<Instant>,
    /// GUI size of the last drawn frame, for laying out clicks the same way.
    gui: [f32; 2],
    icons: Option<(u32, u32, Vec<u8>)>,
    icons_decoded: bool,
    /// The icon sheet is in the image slot for the open screen.
    icons_live: bool,
    anim: Anim,
    /// Sound event ID to subtitle key, from the game's sounds.json.
    subtitles: std::collections::HashMap<String, String>,
    /// Every sound event of the game by group, from the same file.
    catalog: ui::Catalog,
    /// When the visible rows were last written for tests (`HUSHLIST_TRACE`).
    rows_dumped: Option<Instant>,
    /// Cooldown badges drawn in the last frame, with their sound: a click opens the drawer.
    badges: Vec<(Rect, String)>,
    /// Tab counts: heard recently, sounds in the game, settings.
    counts: [usize; 3],
    /// Display names and lowercase search text by row key, for the current language.
    /// The All tab sorts and searches about 2000 sounds; this keeps each frame cheap.
    texts: std::cell::RefCell<Texts>,
    /// When the live order was last refreshed.
    live_at: Option<Instant>,
}

#[derive(Default)]
struct Texts {
    lang: String,
    /// Key id (`g:` group, `s:` sound) to (display name, lowercase name + ID + group name).
    by_key: std::collections::HashMap<String, (String, String)>,
}

/// Animation state between frames. Positions and alphas ease toward their targets.
#[derive(Default)]
struct Anim {
    last: Option<Instant>,
    /// When the screen was (re)opened: the panel fades in.
    opened: Option<Instant>,
    /// Drawn row tops by row identity.
    y: std::collections::HashMap<String, f32>,
    /// Row background reveal (new rows) by row identity.
    reveal: std::collections::HashMap<String, f32>,
    hover: std::collections::HashMap<String, f32>,
    /// Drawn slider values by row identity.
    volume: std::collections::HashMap<String, f32>,
    banner: f32,
    freeze: f32,
}

impl Hushlist {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Hushlist's own text for `key` in the game's current language.
fn tr(key: &'static str) -> &'static str {
    i18n::text(&lang::code(), key)
}

fn humanize(s: &str) -> String {
    let text = s.replace(['_', '.'], " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A sound's name in the game's language: its subtitle, a disc's song, or the ID made readable.
fn sound_name(subtitles: &std::collections::HashMap<String, String>, id: &str) -> String {
    let id = normalize(id);
    if let Some(disc) = id.strip_prefix("music_disc.") {
        return lang::get_or(&format!("jukebox_song.minecraft.{disc}"), &humanize(id));
    }
    match subtitles.get(id) {
        Some(key) => lang::get_or(key, &humanize(id)),
        None => humanize(id),
    }
}

/// Subtitle keys and the list of every sound event, read from the game's `sounds.json`.
fn read_sounds(host: &dyn Host) -> (std::collections::HashMap<String, String>, ui::Catalog) {
    let Some(bytes) = host.client_resources().read("assets/minecraft/sounds.json") else {
        return Default::default();
    };
    let Ok(serde_json::Value::Object(events)) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Default::default();
    };
    let catalog = ui::Catalog::from_ids(events.keys().map(String::as_str));
    let subtitles = events
        .into_iter()
        .filter_map(|(id, event)| Some((id, event.get("subtitle")?.as_str()?.to_owned())))
        .collect();
    (subtitles, catalog)
}

/// The sound has no subtitle, so its name is only its ID made readable.
fn technical_name(subtitles: &std::collections::HashMap<String, String>, id: &str) -> bool {
    let id = normalize(id);
    !id.starts_with("music_disc.") && !subtitles.contains_key(id)
}

/// A group's name: the mob, block or item it belongs to, or the family name.
fn group_name(group: &str) -> String {
    // Sound families and groups the game has no name for are in the mod's own translations.
    if let Some(name) = i18n::lookup(&lang::code(), &format!("group.{group}")) {
        return name.to_owned();
    }
    let (kind, name) = group.split_once('.').unwrap_or(("", group));
    let keys: &[String] = &match kind {
        "entity" => vec![format!("entity.minecraft.{name}")],
        "block" => vec![format!("block.minecraft.{name}"), format!("item.minecraft.{name}")],
        _ => vec![format!("item.minecraft.{name}"), format!("block.minecraft.{name}")],
    };
    for key in keys {
        let text = lang::get_or(key, "");
        if !text.is_empty() {
            return text;
        }
    }
    humanize(name)
}

/// The icon sheet as RGBA8.
fn decode_icons() -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(ICON_SHEET));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().ok()?;
    let (w, h) = (reader.info().width, reader.info().height);
    let mut buf = vec![0u8; w as usize * h as usize * 8];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.line_size * frame.height as usize];
    let rgba = match (frame.color_type, frame.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => data.to_vec(),
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            data.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 0xFF]).collect()
        }
        _ => return None,
    };
    Some((w, h, rgba))
}

impl State {
    fn views(&self, now: Instant) -> Vec<HeardView> {
        self.engine
            .recent
            .list()
            .into_iter()
            .filter(|h| now.saturating_duration_since(h.alive) <= crate::core::RECENT_WINDOW)
            .map(|h| HeardView {
                id: h.id.clone(),
                secs: now.saturating_duration_since(h.last).as_secs_f32(),
                count: h.count,
                playing: h.playing(now),
                source: h.source,
            })
            .collect()
    }

    fn rows(&mut self, now: Instant) -> (Vec<HeardView>, Vec<Row>, usize) {
        let views = self.views(now);
        self.counts = [views.len(), self.catalog.total, self.engine.settings.count()];
        if matches!(self.list.tab, Tab::Changed | Tab::All) {
            let name = |k: &Key| self.text_of(k).0;
            let build = if self.list.tab == Tab::All { ui::build_all } else { ui::build_changed };
            let rows = build(&views, &self.engine.settings, &self.list, &self.catalog, &self.matcher(), &name);
            return (views, rows, 0);
        }
        // The live order waits while a slider is held, so the row does not move under the pointer.
        if !self.list.frozen
            && self.drag.is_none()
            && self.live_at.is_none_or(|t| now.saturating_duration_since(t) >= LIVE_REORDER)
        {
            self.list.live = Some(ui::snapshot(&views, self.list.flat));
            self.live_at = Some(now);
        }
        let (mut rows, mut fresh) =
            ui::build(&views, &self.engine.settings, &self.list, &self.catalog, &self.matcher());
        // A group that collapsed because something in it keeps playing stays collapsed.
        let mut pinned = false;
        for row in &rows {
            if let Key::Group(g) = &row.key
                && row.playing
                && !self.list.open.contains_key(g)
            {
                self.list.open.insert(g.clone(), false);
                pinned = true;
            }
        }
        if pinned {
            (rows, fresh) = ui::build(&views, &self.engine.settings, &self.list, &self.catalog, &self.matcher());
        }
        (views, rows, fresh)
    }

    /// A row's display name and search text, cached per language.
    fn text_of(&self, key: &Key) -> (String, String) {
        let mut texts = self.texts.borrow_mut();
        let code = lang::code();
        if texts.lang != code {
            texts.by_key.clear();
            texts.lang = code;
        }
        let id = match key {
            Key::Group(g) => format!("g:{g}"),
            Key::Sound(s) => format!("s:{s}"),
            Key::More(_) | Key::Cooldown(_) => return Default::default(),
        };
        if let Some(hit) = texts.by_key.get(&id) {
            return hit.clone();
        }
        let entry = match key {
            Key::Group(g) => {
                let name = group_name(g);
                let hay = format!("{name} {g}").to_lowercase();
                (name, hay)
            }
            Key::Sound(s) => {
                let name = sound_name(&self.subtitles, s);
                let group = group_key(s).map(|g| group_name(&g)).unwrap_or_default();
                let hay = format!("{name} {} {group}", normalize(s)).to_lowercase();
                (name, hay)
            }
            Key::More(_) | Key::Cooldown(_) => unreachable!(),
        };
        texts.by_key.insert(id, entry.clone());
        entry
    }

    /// Whether a sound fits the search text: its name, ID, group name or category, any case.
    fn matcher(&self) -> impl Fn(&str) -> bool + '_ {
        let query = self.list.query.trim().to_lowercase();
        let code = lang::code();
        move |id: &str| {
            if query.is_empty() {
                return true;
            }
            let (_, hay) = self.text_of(&Key::Sound(normalize(id).to_owned()));
            if hay.contains(&query) {
                return true;
            }
            let source = self.engine.recent.get(normalize(id)).map_or_else(|| ui::guess_source(id), |h| h.source);
            ui::category_key(source).is_some_and(|cat| i18n::text(&code, cat).to_lowercase().contains(&query))
        }
    }

    /// Search, chips or view changed: the list takes the new shape at once.
    fn filter_changed(&mut self, now: Instant) {
        self.list.live = None;
        self.live_at = None;
        self.list.scroll = 0.0;
        if self.list.frozen {
            let views = self.views(now);
            self.list.snapshot = Some(ui::snapshot(&views, self.list.flat));
        }
    }

    /// The screen layout for this GUI size; chips are measured in the game's font.
    fn layout(&self, host: &dyn Host, fresh: usize) -> ui::Layout {
        let code = lang::code();
        let w = |key: &'static str| host.text_width(i18n::text(&code, key));
        let chips = self.list.chip_order().iter().map(|s| w(ui::category_key(*s).unwrap_or_default())).collect();
        let mut tabs = [0.0; 4];
        for (i, t) in Tab::ORDER.iter().enumerate() {
            tabs[i] = host.text_width(&tab_label(*t, &self.counts));
        }
        let m = ui::Measure {
            chips,
            tabs,
            sort_items: sort_keys(self.list.tab).iter().map(|k| w(k)).collect(),
            buttons: [w("changed.reset_all"), w("button.reset"), w("button.cancel")],
        };
        let banner = self.list.tab == Tab::Recent && self.list.frozen && fresh > 0;
        ui::layout(self.gui[0], self.gui[1], banner, &m, &self.list)
    }

    fn changed(&mut self, now: Instant) {
        self.dirty_at = Some(now);
        self.refresh = match std::mem::take(&mut self.refresh) {
            Refresh::Nudged(_, saved, phase) | Refresh::Again(saved, phase) => Refresh::Again(saved, phase),
            _ => Refresh::Pending,
        };
    }

    fn set_volume(&mut self, key: &Key, value: u8, now: Instant) {
        let s = &mut self.engine.settings;
        match key {
            Key::Sound(id) => s.stamps.insert(format!("sound {id}"), unix_now()),
            Key::Group(g) => s.stamps.insert(format!("group {g}"), unix_now()),
            Key::More(_) | Key::Cooldown(_) => return,
        };
        match key {
            Key::Sound(id) => s.sounds.insert(id.clone(), value),
            Key::Group(g) => s.groups.insert(g.clone(), value),
            Key::More(_) | Key::Cooldown(_) => return,
        };
        self.changed(now);
    }

    fn reset(&mut self, key: &Key, now: Instant) {
        let s = &mut self.engine.settings;
        match key {
            Key::Sound(id) => s.stamps.remove(&format!("sound {id}")),
            Key::Group(g) => s.stamps.remove(&format!("group {g}")),
            Key::More(_) | Key::Cooldown(_) => return,
        };
        match key {
            Key::Sound(id) => s.sounds.remove(id),
            Key::Group(g) => s.groups.remove(g),
            Key::More(_) | Key::Cooldown(_) => return,
        };
        self.list.before_mute.remove(key);
        self.changed(now);
    }

    /// Sets a sound's cooldown (0 removes it). It applies to the next start, so only the file changes.
    fn set_cooldown(&mut self, id: &str, secs: u32, now: Instant) {
        let s = &mut self.engine.settings;
        if secs == 0 {
            s.cooldowns.remove(id);
            s.stamps.remove(&format!("cooldown {id}"));
        } else {
            s.cooldowns.insert(id.to_owned(), secs);
            s.stamps.insert(format!("cooldown {id}"), unix_now());
        }
        self.dirty_at = Some(now);
    }

    fn toggle_drawer(&mut self, id: &str) {
        self.list.drawer = if self.list.drawer.as_deref() == Some(id) { None } else { Some(id.to_owned()) };
    }

    fn toggle_mute(&mut self, row: &Row, now: Instant) {
        if row.volume > 0 {
            self.list.before_mute.insert(row.key.clone(), row.volume);
            self.set_volume(&row.key, 0, now);
        } else {
            let back = self.list.before_mute.remove(&row.key).unwrap_or(100);
            self.set_volume(&row.key, back, now);
        }
    }

    fn load(&mut self, host: &dyn Host, now: Instant) {
        self.loaded = true;
        (self.subtitles, self.catalog) = read_sounds(host);
        if let Ok(dir) = host.config_dir() {
            self.path = Some(dir.join(MOD_ID).join(SETTINGS_FILE));
        }
        self.reload(now);
    }

    /// Reads the settings file when it changed outside the game.
    fn reload(&mut self, now: Instant) {
        self.next_reload = Some(now + RELOAD_EVERY);
        let Some(path) = self.path.clone() else { return };
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if modified == self.modified || self.dirty_at.is_some() {
            return;
        }
        self.modified = modified;
        let settings = Settings::parse(&std::fs::read_to_string(&path).unwrap_or_default());
        if settings != self.engine.settings {
            self.engine.settings = settings;
            self.changed_external();
            self.trace_line(now, "\"event\":\"reload\"");
        }
    }

    fn changed_external(&mut self) {
        self.refresh = match std::mem::take(&mut self.refresh) {
            Refresh::Nudged(_, saved, phase) | Refresh::Again(saved, phase) => Refresh::Again(saved, phase),
            _ => Refresh::Pending,
        };
    }

    fn save(&mut self) {
        self.dirty_at = None;
        let Some(path) = self.path.clone() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&path, self.engine.settings.to_text()).is_ok() {
            self.modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        }
    }

    fn upload_icons(&mut self, host: &mut dyn Host) {
        if !self.icons_decoded {
            self.icons_decoded = true;
            self.icons = decode_icons();
        }
        if let Some((w, h, rgba)) = &self.icons {
            self.icons_live = host.mod_image(*w, *h, rgba);
        }
    }

    fn trace_line(&mut self, now: Instant, body: &str) {
        if !self.trace_opened {
            self.trace_opened = true;
            self.started = Some(now);
            if let Some(path) = std::env::var_os("HUSHLIST_TRACE") {
                self.trace = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
            }
        }
        let Some(file) = self.trace.as_mut() else { return };
        let ms = self.started.map_or(0, |s| now.saturating_duration_since(s).as_millis());
        use std::io::Write;
        let _ = writeln!(file, "{{\"ms\":{ms},{body}}}");
    }
}

impl Hooks for Hushlist {
    fn mods(&self, _host: &mut dyn Host, event: &mut ModsEvent<'_>) {
        if event.mods.iter().any(|m| m.id == MOD_ID) {
            return;
        }
        // The Mods list is made again: start the page without a pending question.
        self.lock().page_confirm = false;
        event.mods.push(NativeMod {
            id: MOD_ID.to_owned(),
            title: format!("Hushlist {MOD_VERSION}"),
            description: tr("mod.description").to_owned(),
            keys: vec![[OPEN_KEY.to_owned(), tr("key.open").to_owned(), OPEN_KEY_DEFAULT.to_owned()]],
        });
    }

    /// The Mods page button: reset everything, with a confirmation step.
    fn settings(&self, _host: &mut dyn Host, event: &mut SettingsEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let st = self.lock();
        if st.page_confirm {
            event.buttons.push(tr("page.reset_yes").to_owned());
            event.buttons.push(tr("button.cancel").to_owned());
            return;
        }
        event.buttons.push(tr("page.reset").to_owned());
    }

    fn settings_click(&self, host: &mut dyn Host, event: &mut SettingsClickEvent<'_>) {
        if event.mod_id != MOD_ID {
            return;
        }
        let now = Instant::now();
        let mut st = self.lock();
        if !st.loaded {
            st.load(&*host, now);
        }
        match (st.page_confirm, event.button) {
            (true, 0) => {
                st.page_confirm = false;
                st.engine.settings = Settings::default();
                st.list.before_mute.clear();
                st.changed(now);
                st.save();
            }
            (true, _) => st.page_confirm = false,
            (false, _) => st.page_confirm = true,
        }
    }

    fn key(&self, host: &mut dyn Host, event: &mut KeyEvent<'_>) {
        if !event.pressed {
            return;
        }
        let bound = host.mod_key(MOD_ID, OPEN_KEY, OPEN_KEY_DEFAULT);
        if event.key != bound {
            return;
        }
        if host.mod_screen() == Some(MOD_ID) {
            host.mod_screen_close(MOD_ID);
        } else if host.playing() && host.mod_screen_open(MOD_ID) {
            let mut st = self.lock();
            st.list.search_focus = false;
            st.upload_icons(host);
        }
    }

    fn client_tick_post(&self, host: &mut dyn Host, _event: &mut ClientTickPostEvent) {
        let now = Instant::now();
        let mut st = self.lock();
        if !st.loaded {
            st.load(&*host, now);
        } else if st.next_reload.is_none_or(|t| now >= t) {
            st.reload(now);
        }
        if host.mod_screen() == Some(MOD_ID) {
            if !st.icons_live {
                st.upload_icons(host);
            }
        } else {
            st.icons_live = false;
        }
        if std::mem::take(&mut st.close) && host.mod_screen() == Some(MOD_ID) {
            host.mod_screen_close(MOD_ID);
        }
        if let Some(id) = st.preview.take()
            && let Some(p) = host.player()
        {
            host.play_local_sound(&format!("minecraft:{id}"), [p.x, p.y + 1.6, p.z], 1.0, 1.0, 0);
        }
        if st.dirty_at.is_some_and(|t| now.saturating_duration_since(t) >= SAVE_DELAY) {
            st.save();
        }
        st.refresh = match std::mem::take(&mut st.refresh) {
            Refresh::Pending => {
                let mut volumes = [0.0f32; 11];
                for (i, v) in volumes.iter_mut().enumerate() {
                    *v = horizon_client::settings::source_volume(i);
                }
                horizon_client::settings::set_source_volumes(nudged(volumes, false));
                Refresh::Nudged(now, volumes, false)
            }
            Refresh::Again(volumes, phase) => {
                horizon_client::settings::set_source_volumes(nudged(volumes, !phase));
                Refresh::Nudged(now, volumes, !phase)
            }
            Refresh::Nudged(at, volumes, _) if now.saturating_duration_since(at) >= NUDGE_HOLD => {
                horizon_client::settings::set_source_volumes(volumes);
                Refresh::Idle
            }
            other => other,
        };
    }

    fn sound_play(&self, event: &mut SoundPlayEvent<'_>) -> Flow {
        let now = Instant::now();
        let mut st = self.lock();
        let s = event.sound;
        // While the mod re-applies volumes, the game replays every playing sound through
        // this hook; those calls must not mark short sounds as still playing.
        let d = if !s.starting && !matches!(st.refresh, Refresh::Idle) {
            crate::core::Decision { factor: st.engine.settings.volume_factor(s.identifier), cancel: false }
        } else {
            st.engine.decide(s.identifier, s.source, s.position, s.starting, now)
        };
        if s.starting {
            let body = format!(
                "\"id\":\"{}\",\"source\":{},\"pos\":[{:.1},{:.1},{:.1}],\"starting\":true,\"factor\":{},\"cancel\":{}",
                normalize(s.identifier), s.source, s.position[0], s.position[1], s.position[2], d.factor, d.cancel
            );
            st.trace_line(now, &body);
        }
        if d.cancel {
            return Flow::Cancel;
        }
        if d.factor != 1.0 {
            event.volume *= d.factor;
        }
        Flow::Continue
    }

    fn screen_render_post(&self, host: &dyn Host, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(MOD_ID) {
            return;
        }
        let mut st = self.lock();
        draw(&mut st, host, event.batch, event.screen.mouse);
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn Host,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        if event.button != 0 {
            return Flow::Cancel;
        }
        let now = Instant::now();
        let mut st = self.lock();
        let (views, rows, fresh) = st.rows(now);
        let lay = st.layout(host, fresh);
        if let Some(id) = st.badges.iter().find(|(r, _)| ui::contains(*r, event.screen.mouse)).map(|(_, id)| id.clone()) {
            st.list.search_focus = false;
            st.toggle_drawer(&id);
            return Flow::Cancel;
        }
        let hit = ui::hit(&lay, &rows, st.list.scroll, event.screen.mouse);
        // A click anywhere else gives the keyboard back to the game's keys.
        st.list.search_focus = matches!(hit, Hit::Search | Hit::SearchClear);
        if !matches!(hit, Hit::Sort | Hit::SortItem(_)) {
            st.list.sort_open = false;
        }
        if !matches!(hit, Hit::ResetAll | Hit::ConfirmYes | Hit::ConfirmNo) {
            st.list.confirm_reset = false;
        }
        match hit {
            Hit::Close => st.close = true,
            Hit::Tab(i) => {
                let tab = Tab::ORDER[i];
                if tab.ready() && tab != st.list.tab {
                    st.list.tab = tab;
                    st.filter_changed(now);
                    st.anim.y.clear();
                    st.anim.reveal.clear();
                }
            }
            Hit::Sort => st.list.sort_open = !st.list.sort_open,
            Hit::SortItem(i) => {
                match st.list.tab {
                    Tab::All => st.list.sort_all = SortAll::ORDER[i],
                    _ => st.list.sort_changed = SortChanged::ORDER[i],
                }
                st.list.sort_open = false;
            }
            Hit::SortClose => {}
            Hit::ResetAll => st.list.confirm_reset = true,
            Hit::ConfirmNo => st.list.confirm_reset = false,
            Hit::ConfirmYes => {
                st.list.confirm_reset = false;
                st.engine.settings = Settings::default();
                st.list.before_mute.clear();
                st.changed(now);
            }
            Hit::Search => {}
            Hit::SearchClear => {
                if !st.list.query.is_empty() {
                    st.list.query.clear();
                    st.filter_changed(now);
                }
            }
            Hit::Chip(i) => {
                if let Some(&source) = st.list.chip_order().get(i) {
                    st.list.toggle_cat(source);
                    st.filter_changed(now);
                }
            }
            Hit::ViewFlat | Hit::ViewGroup => {
                let flat = matches!(hit, Hit::ViewFlat);
                if st.list.flat != flat {
                    st.list.flat = flat;
                    st.filter_changed(now);
                    // A different kind of list: rows appear in place instead of gliding across each other.
                    st.anim.y.clear();
                    st.anim.reveal.clear();
                }
            }
            Hit::Freeze => {
                st.list.frozen = !st.list.frozen;
                let flat = st.list.flat;
                st.list.snapshot = st.list.frozen.then(|| ui::snapshot(&views, flat));
            }
            Hit::Banner => st.list.snapshot = Some(ui::snapshot(&views, st.list.flat)),
            Hit::Speaker(i) => st.toggle_mute(&rows[i], now),
            Hit::Slider(i, v) => {
                st.drag = Some(rows[i].key.clone());
                st.set_volume(&rows[i].key, v, now);
            }
            Hit::Reset(i) => st.reset(&rows[i].key, now),
            Hit::Timer(i) => {
                if let Key::Sound(id) = &rows[i].key {
                    st.toggle_drawer(id);
                }
            }
            Hit::CooldownSlider(i, step) => {
                if let Key::Cooldown(id) = &rows[i].key {
                    st.drag = Some(rows[i].key.clone());
                    st.set_cooldown(id, ui::RATES[step], now);
                }
            }
            Hit::CooldownReset(i) => {
                if let Key::Cooldown(id) = &rows[i].key {
                    st.set_cooldown(id, 0, now);
                }
            }
            Hit::Row(i) => match &rows[i].key {
                Key::Group(g) => {
                    let open = !rows[i].open;
                    st.list.set_group_open(g, open);
                }
                Key::Sound(id) => st.preview = Some(id.clone()),
                Key::More(g) => {
                    if !st.list.show_all.remove(g) {
                        st.list.show_all.insert(g.clone());
                    }
                }
                Key::Cooldown(_) => {}
            },
            Hit::Nothing => {}
        }
        Flow::Cancel
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn Host,
        _event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        self.lock().drag = None;
        Flow::Cancel
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn Host, event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let now = Instant::now();
        let mut st = self.lock();
        let Some(key) = st.drag.clone() else { return Flow::Cancel };
        let (_, rows, fresh) = st.rows(now);
        let lay = st.layout(host, fresh);
        if let Some((i, r)) = ui::visible(rows.len(), lay.list, st.list.scroll)
            .into_iter()
            .find(|(i, _)| rows[*i].key == key)
        {
            let slider = ui::columns(r, rows[i].child).slider;
            if let Key::Cooldown(id) = &key {
                let step = ui::rate_value(slider, event.screen.mouse[0]);
                st.set_cooldown(id, ui::RATES[step], now);
            } else {
                let v = ui::slider_value(slider, event.screen.mouse[0]);
                st.set_volume(&key, v, now);
            }
        }
        Flow::Cancel
    }

    /// While the search field has the keyboard, keys edit it and do nothing else
    /// (so H types an "h" instead of closing the screen). Ctrl+F puts the keyboard there.
    fn screen_key_pressed_pre(&self, host: &dyn Host, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = self.lock();
        let ctrl = event.modifiers & 2 != 0;
        if !st.list.search_focus {
            if ctrl && event.key == "key.keyboard.f" {
                st.list.search_focus = true;
                return Flow::Cancel;
            }
            return Flow::Continue;
        }
        match event.key {
            "key.keyboard.backspace" => {
                if ctrl {
                    st.list.query.clear();
                } else {
                    st.list.query.pop();
                }
                st.filter_changed(Instant::now());
            }
            "key.keyboard.escape" | "key.keyboard.enter" | "key.keyboard.keypad.enter" => st.list.search_focus = false,
            _ => {}
        }
        Flow::Cancel
    }

    fn screen_char_typed_pre(&self, host: &dyn Host, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = self.lock();
        if !st.list.search_focus {
            return Flow::Continue;
        }
        if !event.c.is_control() && st.list.query.chars().count() < ui::QUERY_MAX {
            st.list.query.push(event.c);
            st.filter_changed(Instant::now());
        }
        Flow::Cancel
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn Host, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        if host.mod_screen() != Some(MOD_ID) {
            return Flow::Continue;
        }
        let mut st = self.lock();
        st.list.scroll = (st.list.scroll - event.scroll[1] as f32 * 18.0).max(0.0);
        Flow::Cancel
    }
}

fn argb(rgb: u32) -> u32 {
    0xFF00_0000 | rgb
}

/// One icon, centred vertically in `r` at its left edge.
fn icon(host: &dyn Host, b: &mut HudBatch, icon: Icon, tone: Tone, r: Rect) {
    let src = [icon as u8 as f32 * ICON_CELL, tone as u8 as f32 * ICON_CELL, ICON_CELL, ICON_CELL];
    let y = (r[1] + (r[3] - ICON_SIZE) / 2.0).floor();
    host.sprite(b, MOD_IMAGE, src, [r[0], y, ICON_SIZE, ICON_SIZE]);
}

/// Text cut with an ellipsis to fit `width`: a binary search over character
/// counts (a handful of measurements), remembered per text and width.
fn fit(host: &dyn Host, text: &str, width: f32) -> String {
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<(String, u32), String>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    let key = (text.to_owned(), width.to_bits());
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    let out = if host.text_width(text) <= width {
        text.to_owned()
    } else {
        let chars: Vec<char> = text.chars().collect();
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let candidate: String = chars[..mid].iter().collect::<String>() + "...";
            if host.text_width(&candidate) <= width {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        chars[..lo].iter().collect::<String>().trim_end().to_owned() + "..."
    };
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 2048 {
            c.clear();
        }
        c.insert(key, out.clone());
    });
    out
}

/// Three bars: lit while the row's sound plays, moving with time.
fn bars(host: &dyn Host, b: &mut HudBatch, r: Rect, lit: bool, now_ms: u128) {
    let base = (r[1] + r[3] / 2.0 + 4.0).floor();
    for i in 0..3u128 {
        let h = if lit {
            let phase = ((now_ms / 120 + i * 2) % 4) as f32;
            2.0 + phase * 2.0
        } else {
            2.0
        };
        let colour = if lit { argb(AMBER) } else { argb(FAINT) };
        host.fill(b, [r[0] + 1.0 + i as f32 * 3.0, base - h, 2.0, h], colour);
    }
}

/// A filled rectangle with rounded corners.
fn rrect(host: &dyn Host, b: &mut HudBatch, r: Rect, radius: f32, argb: u32) {
    if argb >> 24 == 0 || r[2] <= 0.0 || r[3] <= 0.0 {
        return;
    }
    let (rects, tris) = ui::rounded(r, radius);
    for piece in rects {
        host.fill(b, piece, argb);
    }
    for q in tris {
        host.quad(b, q, argb);
    }
}

/// With `HUSHLIST_TRACE` set, writes the visible rows (identity and rectangle) to
/// `<trace>.rows.json`, so an automated test can find a row by its sound.
fn dump_rows(st: &mut State, rows: &[Row], lay: &ui::Layout, now: Instant) {
    let list = lay.list;
    let Some(trace) = std::env::var_os("HUSHLIST_TRACE") else { return };
    if st.rows_dumped.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(250)) {
        return;
    }
    st.rows_dumped = Some(now);
    let items: Vec<String> = ui::visible(rows.len(), list, st.list.scroll)
        .into_iter()
        .map(|(i, r)| {
            format!(
                "{{\"id\":\"{}\",\"child\":{},\"own\":{},\"volume\":{},\"count\":{},\"cooldown\":{},\"name\":{:?},\"rect\":[{},{},{},{}]}}",
                row_id(&rows[i]), rows[i].child, rows[i].own, rows[i].volume, rows[i].count, rows[i].cooldown,
                match &rows[i].key {
                    Key::Group(g) => group_name(g),
                    Key::Sound(s) => sound_name(&st.subtitles, s),
                    Key::More(_) | Key::Cooldown(_) => String::new(),
                },
                r[0], r[1], r[2], r[3]
            )
        })
        .collect();
    let mut path = std::path::PathBuf::from(trace);
    path.as_mut_os_string().push(".rows.json");
    let r = |r: Rect| format!("[{},{},{},{}]", r[0], r[1], r[2], r[3]);
    let chips: Vec<String> = lay.chips.iter().map(|c| r(*c)).collect();
    let chip_ids: Vec<String> = st.list.chip_order().iter().map(|s| s.to_string()).collect();
    let layout = format!(
        "{{\"search\":{},\"search_clear\":{},\"close\":{},\"chips\":[{}],\"chip_sources\":[{}],\"view_flat\":{},\"view_group\":{},\"banner\":{},\"list\":{},\"freeze\":{},\"query\":{:?},\"focus\":{},\"flat\":{},\"tab\":\"{:?}\",\"tabs\":[{}],\"sort\":{},\"sort_menu\":[{}],\"reset_all\":{},\"confirm_yes\":{},\"confirm_no\":{}}}",
        r(lay.search), r(lay.search_clear), r(lay.close), chips.join(","), chip_ids.join(","), r(lay.view_flat),
        r(lay.view_group), lay.banner.map_or("null".to_owned(), r), r(lay.list), r(lay.freeze), st.list.query,
        st.list.search_focus, st.list.flat, st.list.tab,
        lay.tab_rects.iter().map(|t| r(*t)).collect::<Vec<_>>().join(","),
        lay.sort.map_or("null".to_owned(), r),
        lay.sort_menu.iter().map(|t| r(*t)).collect::<Vec<_>>().join(","),
        lay.reset_all.map_or("null".to_owned(), r),
        lay.confirm_yes.map_or("null".to_owned(), r),
        lay.confirm_no.map_or("null".to_owned(), r)
    );
    let _ = std::fs::write(path, format!("{{\"layout\":{layout},\"rows\":[{}]}}", items.join(",")));
}

/// Sort menu items of a tab.
fn sort_keys(tab: Tab) -> Vec<&'static str> {
    match tab {
        Tab::All => SortAll::ORDER.iter().map(|s| s.key()).collect(),
        Tab::Changed => SortChanged::ORDER.iter().map(|s| s.key()).collect(),
        _ => Vec::new(),
    }
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn tab_key(tab: Tab) -> &'static str {
    match tab {
        Tab::Recent => "tab.recent",
        Tab::All => "tab.all",
        Tab::Changed => "tab.changed",
        Tab::Zones => "tab.zones",
    }
}

fn tab_count(tab: Tab, counts: &[usize; 3]) -> Option<usize> {
    match tab {
        Tab::Recent => Some(counts[0]),
        Tab::All => Some(counts[1]),
        Tab::Changed => Some(counts[2]),
        Tab::Zones => None,
    }
}

/// A tab's label with its count, as measured for the layout.
fn tab_label(tab: Tab, counts: &[usize; 3]) -> String {
    let label = tr(tab_key(tab));
    match tab_count(tab, counts) {
        Some(n) => format!("{label} {n}"),
        None => label.to_owned(),
    }
}

/// An outlined button with a centred label.
fn button(host: &dyn Host, b: &mut HudBatch, r: Rect, label: &str, rgb: u32, edge: u32, hover: bool) {
    rrect(host, b, r, 3.0, if hover { mix(edge, 0xFF_FF_FF_FF, 0.25) } else { edge });
    rrect(host, b, [r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0], 2.0, argb(if hover { 0x24_22_2C } else { 0x16_15_1B }));
    host.text(b, label, [r[0] + (r[2] - host.text_width(label)) / 2.0, r[1] + 6.0], rgb, false);
}

/// The Changed tab's footer: how many changes, and "Reset all" with its confirmation.
fn draw_footer(st: &State, host: &dyn Host, b: &mut HudBatch, lay: &ui::Layout, mouse: [f32; 2]) {
    if st.list.tab != Tab::Changed {
        return;
    }
    let code = lang::code();
    let at = lay.foot_text;
    if let (Some(yes), Some(no)) = (lay.confirm_yes, lay.confirm_no) {
        // The question takes two lines when it does not fit in one.
        let room = yes[0] - 8.0 - at[0];
        let q = tr("changed.reset_q");
        if host.text_width(q) <= room {
            host.text(b, q, at, TEXT, false);
        } else {
            let mut first = String::new();
            let mut words = q.split(' ').peekable();
            while let Some(word) = words.peek() {
                let next = if first.is_empty() { (*word).to_owned() } else { format!("{first} {word}") };
                if host.text_width(&next) > room && !first.is_empty() {
                    break;
                }
                first = next;
                words.next();
            }
            let rest: Vec<&str> = words.collect();
            host.text(b, &first, [at[0], at[1] - 5.0], TEXT, false);
            host.text(b, &fit(host, &rest.join(" "), room), [at[0], at[1] + 4.0], DIM, false);
        }
        button(host, b, yes, tr("button.reset"), DANGER, 0x80_FF_8A_7A, ui::contains(yes, mouse));
        button(host, b, no, tr("button.cancel"), TEXT, EDGE, ui::contains(no, mouse));
        return;
    }
    let n = st.engine.settings.count();
    if n == 0 {
        host.text(b, tr("changed.none"), at, DIM, false);
        return;
    }
    host.text(b, &i18n::with_n(&code, "changed.count", n), at, DIM, false);
    if let Some(r) = lay.reset_all {
        button(host, b, r, tr("changed.reset_all"), TEXT, EDGE, ui::contains(r, mouse));
    }
}

/// The sort menu, drawn over the list.
fn draw_sort_menu(st: &State, host: &dyn Host, b: &mut HudBatch, lay: &ui::Layout, mouse: [f32; 2]) {
    let Some(first) = lay.sort_menu.first() else { return };
    let last = lay.sort_menu[lay.sort_menu.len() - 1];
    let panel = [first[0] - 3.0, first[1] - 3.0, first[2] + 6.0, last[1] + last[3] - first[1] + 6.0];
    rrect(host, b, [panel[0] - 1.0, panel[1] - 1.0, panel[2] + 2.0, panel[3] + 2.0], 5.0, 0x30_FF_FF_FF);
    rrect(host, b, panel, 4.0, MENU);
    let selected = match st.list.tab {
        Tab::All => SortAll::ORDER.iter().position(|s| *s == st.list.sort_all),
        _ => SortChanged::ORDER.iter().position(|s| *s == st.list.sort_changed),
    };
    for (i, (r, key)) in lay.sort_menu.iter().zip(sort_keys(st.list.tab)).enumerate() {
        let on = selected == Some(i);
        let hover = ui::contains(*r, mouse);
        if on || hover {
            rrect(host, b, *r, 3.0, if on { 0x24_FF_FF_FF } else { 0x14_FF_FF_FF });
        }
        host.text(b, tr(key), [r[0] + 9.0, r[1] + 4.0], if on || hover { TEXT } else { DIM }, false);
    }
}

fn row_id(row: &Row) -> String {
    match &row.key {
        Key::Group(g) => format!("g:{g}"),
        Key::Sound(s) => format!("{}:{s}", if row.child { "c" } else { "s" }),
        Key::More(g) => format!("m:{g}"),
        Key::Cooldown(s) => format!("k:{s}"),
    }
}

fn draw(st: &mut State, host: &dyn Host, b: &mut HudBatch, mouse: [f32; 2]) {
    let now = Instant::now();
    let now_ms = st.started.map_or(0, |s| now.saturating_duration_since(s).as_millis());
    // A gap in drawing means the screen was closed and opened again.
    if st.anim.last.is_none_or(|l| now.saturating_duration_since(l) > Duration::from_millis(300)) {
        st.anim = Anim {
            opened: Some(now),
            ..Anim::default()
        };
    }
    let dt = st
        .anim
        .last
        .map_or(0.0, |l| now.saturating_duration_since(l).as_secs_f32().min(0.1));
    st.anim.last = Some(now);
    let open_alpha = st
        .anim
        .opened
        .map_or(1.0, |o| (now.saturating_duration_since(o).as_secs_f32() / 0.14).min(1.0));
    let code = lang::code();
    st.gui = [b.width, b.height];
    let (_, rows, fresh) = st.rows(now);
    let lay = st.layout(host, fresh);
    st.list.scroll = st.list.scroll.min(ui::max_scroll(rows.len(), lay.list));
    let p = lay.panel;

    rrect(host, b, p, 7.0, fade(BG, open_alpha));
    host.fill(b, [p[0] + 6.0, p[1], p[2] - 12.0, 1.0], fade(EDGE, open_alpha));

    host.fill(b, [lay.title[0], lay.title[1] + 2.0, 4.0, 4.0], argb(AMBER));
    host.text(b, "Hushlist", [lay.title[0] + 9.0, lay.title[1]], TEXT, false);
    let close_hover = ui::contains(lay.close, mouse);
    if close_hover {
        rrect(host, b, lay.close, 3.0, ROW_HOVER);
    }
    let close_tone = if close_hover { Tone::Text } else { Tone::Dim };
    icon(host, b, Icon::Close, close_tone, [lay.close[0] + 2.0, lay.close[1], ICON_SIZE, lay.close[3]]);

    // Search field: placeholder, typed text (its end when long), caret while it has the keyboard.
    let s = lay.search;
    let focus = st.list.search_focus;
    let s_hover = ui::contains(s, mouse);
    let s_edge = if focus { FIELD_EDGE_FOCUS } else if s_hover { 0x38_FF_FF_FF } else { EDGE };
    rrect(host, b, s, 3.0, s_edge);
    rrect(host, b, [s[0] + 1.0, s[1] + 1.0, s[2] - 2.0, s[3] - 2.0], 2.0, FIELD);
    icon(host, b, Icon::Search, if focus { Tone::Text } else { Tone::Dim }, [s[0] + 4.0, s[1], ICON_SIZE, s[3]]);
    let tx = s[0] + 20.0;
    let room = (s[2] - 20.0 - 18.0).max(0.0);
    if st.list.query.is_empty() {
        if !focus {
            let ph = fit(host, tr("search.placeholder"), room);
            host.text(b, &ph, [tx, s[1] + 4.0], FAINT, false);
        }
    } else {
        let mut shown: &str = &st.list.query;
        while host.text_width(shown) > room {
            let mut chars = shown.chars();
            chars.next();
            shown = chars.as_str();
        }
        host.text(b, shown, [tx, s[1] + 4.0], TEXT, false);
        let clear_hover = ui::contains(lay.search_clear, mouse);
        let tone = if clear_hover { Tone::Text } else { Tone::Faint };
        icon(host, b, Icon::Close, tone, [lay.search_clear[0] + 2.0, s[1], ICON_SIZE, s[3]]);
    }
    if focus && (now_ms / 530) % 2 == 0 {
        let mut shown: &str = &st.list.query;
        while host.text_width(shown) > room {
            let mut chars = shown.chars();
            chars.next();
            shown = chars.as_str();
        }
        let cx = tx + host.text_width(shown) + 1.0;
        host.fill(b, [cx, s[1] + 4.0, 1.0, 8.0], argb(TEXT));
    }

    // Tabs with counts; the ones not built yet are faint and do nothing.
    let t = lay.tabs;
    for (i, tab) in Tab::ORDER.iter().enumerate() {
        let r = lay.tab_rects[i];
        let x = r[0] + 3.0;
        let label = tr(tab_key(*tab));
        let active = *tab == st.list.tab;
        let rgb = if active {
            TEXT
        } else if !tab.ready() {
            FAINT
        } else if ui::contains(r, mouse) {
            TEXT
        } else {
            DIM
        };
        host.text(b, label, [x, t[1] + 3.0], rgb, false);
        if let Some(n) = tab_count(*tab, &st.counts) {
            let n = n.to_string();
            host.text(b, &n, [x + host.text_width(label) + host.text_width(" "), t[1] + 3.0], FAINT, false);
        }
        if active {
            rrect(host, b, [x - 1.0, t[1] + t[3] - 2.0, r[2] - 4.0, 2.0], 1.0, argb(TEXT));
        }
    }
    host.fill(b, [p[0] + 1.0, t[1] + t[3], p[2] - 2.0, 1.0], EDGE);

    // Category chips: picked ones are filled and move to the front.
    let order = st.list.chip_order();
    for (i, r) in lay.chips.iter().enumerate() {
        let Some(&source) = order.get(i) else { break };
        let label = tr(ui::category_key(source).unwrap_or_default());
        let hover = ui::contains(*r, mouse);
        let rgb = if st.list.cats.contains(&source) {
            rrect(host, b, *r, 7.0, argb(TEXT));
            CHIP_ON_TEXT
        } else {
            rrect(host, b, *r, 7.0, if hover { 0x40_FF_FF_FF } else { EDGE });
            rrect(host, b, [r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0], 6.0, argb(0x16_15_1B));
            if hover { TEXT } else { DIM }
        };
        host.text(b, label, [r[0] + 7.0, r[1] + 3.0], rgb, false);
    }
    // View: flat list or groups, a two-part switch.
    let (vf, vg) = (lay.view_flat, lay.view_group);
    let whole = [vf[0], vf[1], vg[0] + vg[2] - vf[0], vf[3]];
    rrect(host, b, whole, 3.0, EDGE);
    rrect(host, b, [whole[0] + 1.0, whole[1] + 1.0, whole[2] - 2.0, whole[3] - 2.0], 2.0, argb(0x16_15_1B));
    let on = if st.list.flat { vf } else { vg };
    rrect(host, b, [on[0] + 1.0, on[1] + 1.0, on[2] - 2.0, on[3] - 2.0], 2.0, 0x30_FF_FF_FF);
    for (r, ic, active) in [(vf, Icon::ViewList, st.list.flat), (vg, Icon::ViewGroups, !st.list.flat)] {
        let tone = if active || ui::contains(r, mouse) { Tone::Text } else { Tone::Dim };
        icon(host, b, ic, tone, [r[0] + 3.0, r[1], ICON_SIZE, r[3]]);
    }
    if let Some(s) = lay.sort {
        let hot = st.list.sort_open || ui::contains(s, mouse);
        rrect(host, b, s, 3.0, if hot { 0x40_FF_FF_FF } else { EDGE });
        rrect(host, b, [s[0] + 1.0, s[1] + 1.0, s[2] - 2.0, s[3] - 2.0], 2.0, argb(if st.list.sort_open { 0x2E_2C_36 } else { 0x16_15_1B }));
        icon(host, b, Icon::Sort, if hot { Tone::Text } else { Tone::Dim }, [s[0] + 3.0, s[1], ICON_SIZE, s[3]]);
    }

    st.anim.banner = approach(st.anim.banner, if lay.banner.is_some() { 1.0 } else { 0.0 }, dt, 16.0);
    if let Some(r) = lay.banner {
        rrect(host, b, r, 3.0, fade(BANNER, st.anim.banner));
        if ui::contains(r, mouse) {
            rrect(host, b, r, 3.0, 0x14_FF_FF_FF);
        }
        let text = i18n::with_n(&code, if fresh == 1 { "banner.new.one" } else { "banner.new.other" }, fresh);
        host.text(b, &text, [r[0] + 8.0, r[1] + 4.0], AMBER, false);
        let hint = tr("banner.hint");
        host.text(b, hint, [r[0] + r[2] - host.text_width(hint) - 8.0, r[1] + 4.0], DIM, false);
    }

    if rows.is_empty() && st.list.tab == Tab::Changed && !st.list.filtering() {
        let l = lay.list;
        let a = tr("changed.none");
        host.text(b, a, [l[0] + (l[2] - host.text_width(a)) / 2.0, l[1] + 30.0], DIM, false);
    } else if rows.is_empty() && st.list.filtering() {
        let l = lay.list;
        let a = tr("empty.nothing");
        host.text(b, a, [l[0] + (l[2] - host.text_width(a)) / 2.0, l[1] + 30.0], DIM, false);
    } else if rows.is_empty() && st.list.tab == Tab::Recent {
        let l = lay.list;
        let a = tr("empty.title");
        let c = tr("empty.text");
        host.text(b, a, [l[0] + (l[2] - host.text_width(a)) / 2.0, l[1] + 30.0], TEXT, false);
        host.text(b, c, [l[0] + (l[2] - host.text_width(c)) / 2.0, l[1] + 44.0], DIM, false);
    }

    let list = lay.list;
    st.badges.clear();
    let had_rows = !st.anim.y.is_empty();
    let mut seen = std::collections::HashSet::new();
    for (i, target) in ui::visible(rows.len(), list, st.list.scroll) {
        let row = &rows[i];
        let id = row_id(row);
        seen.insert(id.clone());
        // Rows glide to their new place; a row added later slides in from slightly above.
        let first = !st.anim.y.contains_key(&id);
        let from = if first && had_rows { target[1] - 6.0 } else { target[1] };
        let y = approach(*st.anim.y.get(&id).unwrap_or(&from), target[1], dt, 14.0);
        st.anim.y.insert(id.clone(), y);
        let start_reveal = if first && had_rows { 0.0 } else { 1.0 };
        let reveal = approach(*st.anim.reveal.get(&id).unwrap_or(&start_reveal), 1.0, dt, 12.0);
        st.anim.reveal.insert(id.clone(), reveal);
        if y < list[1] - 1.0 || y + ui::ROW_H > list[1] + list[3] + 1.0 {
            continue;
        }
        let r = [target[0], y, target[2], target[3]];
        let c = ui::columns(r, row.child);
        let hover = ui::contains(target, mouse);
        let h = approach(*st.anim.hover.get(&id).unwrap_or(&0.0), if hover { 1.0 } else { 0.0 }, dt, 18.0);
        st.anim.hover.insert(id.clone(), h);

        // Background: base, playing tint, hover, and a flash that fades out.
        let left = if row.child { r[0] + ui::CHILD_INDENT } else { r[0] };
        let bg_rect = [left, r[1], r[0] + r[2] - left, r[3]];
        let mut bg = if row.playing { ROW_LONG } else { ROW };
        bg = mix(bg, ROW_HOVER, h);
        if row.secs < ui::FLASH_SECS {
            let f = 1.0 - row.secs / ui::FLASH_SECS;
            bg = mix(bg, ROW_FLASH, f * f * (3.0 - 2.0 * f));
        }
        if let Key::Cooldown(_) = &row.key {
            // The "less often" drawer: label and value on the left, a stepped slider under the
            // volume slider, its own reset under the volume reset.
            let left = if row.child { r[0] + ui::CHILD_INDENT } else { r[0] };
            rrect(host, b, [left, r[1] - ui::ROW_GAP, r[0] + r[2] - left, r[3] + ui::ROW_GAP], 3.0, fade(ROW, reveal));
            let label = tr("cooldown.label");
            host.text(b, label, [c.name[0], r[1] + ui::MID_Y], TEXT, false);
            let value = ui::cooldown_text(row.cooldown, &code);
            let vrgb = if row.cooldown > 0 { OWN } else { DIM };
            host.text(b, &value, [c.name[0] + host.text_width(label) + 8.0, r[1] + ui::MID_Y], vrgb, false);
            let s = c.slider;
            let cy = (s[1] + s[3] / 2.0).floor();
            let steps = (ui::RATES.len() - 1) as f32;
            let at = ui::rate_index(row.cooldown) as f32 / steps;
            rrect(host, b, [s[0] + 2.0, cy - 1.5, s[2] - 4.0, 3.0], 1.5, TRACK);
            for k in 1..ui::RATES.len() - 1 {
                let tx = (s[0] + 2.0 + (s[2] - 4.0) * k as f32 / steps).floor();
                host.fill(b, [tx, cy + 3.0, 1.0, 2.0], 0x40_FF_FF_FF);
            }
            let filled = (s[2] - 4.0) * at;
            rrect(host, b, [s[0] + 2.0, cy - 1.5, filled, 3.0], 1.5, argb(if row.cooldown > 0 { OWN } else { DIM }));
            let grow = if st.drag.as_ref() == Some(&row.key) || (hover && ui::contains(s, mouse)) { 1.0 } else { 0.0 };
            rrect(
                host,
                b,
                [s[0] + 2.0 + filled - 2.5 - grow * 0.5, cy - 5.0 - grow, 5.0 + grow, 10.0 + grow * 2.0],
                2.5,
                THUMB,
            );
            if row.cooldown > 0 {
                if hover && ui::contains(c.reset, mouse) {
                    rrect(host, b, [c.reset[0] - 1.0, r[1] + 4.0, 16.0, r[3] - 8.0], 3.0, ROW_HOVER);
                }
                icon(host, b, Icon::Reset, Tone::Own, [c.reset[0] + 1.0, c.reset[1], ICON_SIZE, c.reset[3]]);
            }
            continue;
        }
        if let Key::More(_) = &row.key {
            // A quiet button, not a sound row: no background, a thin outline, one label.
            let label = if row.open {
                tr("group.hide_unheard").to_owned()
            } else {
                i18n::with_n(&code, "group.more", row.count)
            };
            let w = host.text_width(&label) + 16.0;
            let btn = [c.name[0] - 4.0, r[1] + 5.0, w, r[3] - 10.0];
            let edge = fade(mix(EDGE, 0xFF_FF_FF_FF, 0.15 * h), reveal);
            rrect(host, b, btn, 3.0, edge);
            rrect(host, b, [btn[0] + 1.0, btn[1] + 1.0, btn[2] - 2.0, btn[3] - 2.0], 2.0, fade(mix(BG, ROW_HOVER, h), reveal));
            let rgb = if hover { TEXT } else { DIM };
            host.text(b, &label, [btn[0] + 8.0, btn[1] + 4.0], rgb, false);
            continue;
        }
        rrect(host, b, bg_rect, 3.0, fade(bg, reveal));

        let (name, detail) = match &row.key {
            Key::Group(g) => {
                let chevron = if row.open { Icon::ChevronDown } else { Icon::ChevronRight };
                icon(host, b, chevron, Tone::Dim, c.ind);
                let detail = if st.list.tab == Tab::All {
                    // The All tab is about the game's sounds, not what was heard.
                    i18n::with_n(&code, "group.sounds", st.catalog.groups.get(g).map_or(0, Vec::len))
                } else if row.open {
                    i18n::with_n(&code, "group.heard_count", row.heard.len())
                } else {
                    let names: Vec<String> = row.heard.iter().map(|id| sound_name(&st.subtitles, id)).collect();
                    tr("group.heard_list").replace("{list}", &names.join(", "))
                };
                (group_name(g), detail)
            }
            Key::Sound(sid) => {
                let over_controls = mouse[0] >= c.speaker[0];
                if hover && !over_controls {
                    icon(host, b, Icon::Play, Tone::Text, c.ind);
                } else {
                    bars(host, b, c.ind, row.playing || row.flash, now_ms);
                }
                let source = if row.child {
                    String::new()
                } else if let (true, true, Some(g)) = (st.list.flat, row.inherited, group_key(sid)) {
                    tr("group.from").replace("{name}", &group_name(&g)) + " · "
                } else {
                    group_key(sid).map_or_else(|| tr("sound.no_group").to_owned(), |g| group_name(&g)) + " · "
                };
                (sound_name(&st.subtitles, sid), format!("{source}minecraft:{sid}"))
            }
            Key::More(_) | Key::Cooldown(_) => unreachable!("drawn above"),
        };
        // The game font has no italics for mods, so a name made from the ID is drawn dimmer.
        let technical = matches!(&row.key, Key::Sound(sid) if technical_name(&st.subtitles, sid));
        let name_rgb = if row.volume == 0 { FAINT } else if technical { DIM } else { TEXT };
        let badge = (row.cooldown > 0).then(|| ui::cooldown_text(row.cooldown, &code));
        let badge_w = badge.as_ref().map_or(0.0, |t| host.text_width(t) + 10.0);
        let name_room = if badge.is_some() { (c.name[2] - badge_w - 4.0).max(0.0) } else { c.name[2] };
        let shown_name = fit(host, &name, name_room);
        host.text(b, &shown_name, [c.name[0], r[1] + ui::NAME_Y], name_rgb, false);
        if let (Some(text), Key::Sound(sid)) = (&badge, &row.key) {
            let bx = c.name[0] + host.text_width(&shown_name) + 4.0;
            let br = [bx, r[1] + ui::NAME_Y - 1.0, badge_w, 10.0];
            let edge = if ui::contains(br, mouse) { 0x99_6F_F0_DA } else { 0x59_6F_F0_DA };
            rrect(host, b, br, 5.0, edge);
            rrect(host, b, [br[0] + 1.0, br[1] + 1.0, br[2] - 2.0, br[3] - 2.0], 4.0, argb(0x1A_1F_22));
            host.text(b, text, [bx + 5.0, r[1] + ui::NAME_Y], OWN, false);
            st.badges.push((br, sid.clone()));
        }
        host.text(b, &fit(host, &detail, c.name[2]), [c.name[0], r[1] + ui::DETAIL_Y], FAINT, false);

        let when = ui::when_text(row, &code);
        let when_rgb = if row.playing || row.flash { AMBER } else { DIM };
        host.text(b, &when, [c.when[0] + c.when[2] - host.text_width(&when), r[1] + ui::NAME_Y], when_rgb, false);
        if !row.playing && row.count > 0 {
            let count = format!("×{}", row.count);
            host.text(b, &count, [c.when[0] + c.when[2] - host.text_width(&count), r[1] + ui::DETAIL_Y], FAINT, false);
        }

        let (rgb, tone) = if row.volume == 0 {
            (FAINT, Tone::Faint)
        } else if row.own {
            (OWN, Tone::Own)
        } else if row.inherited {
            (INH, Tone::Inherited)
        } else {
            (DIM, Tone::Dim)
        };
        let speaker = match row.volume {
            0 => Icon::SpeakerOff,
            1..50 => Icon::SpeakerLow,
            _ => Icon::SpeakerHigh,
        };
        if hover && ui::contains(c.speaker, mouse) {
            rrect(host, b, [c.speaker[0] - 2.0, r[1] + 4.0, 16.0, r[3] - 8.0], 3.0, ROW_HOVER);
        }
        icon(host, b, speaker, tone, c.speaker);

        // Slider: eases to a clicked value, follows the pointer while dragged.
        let dragging = st.drag.as_ref() == Some(&row.key);
        let target_volume = f32::from(row.volume);
        let shown = if dragging {
            target_volume
        } else {
            approach(*st.anim.volume.get(&id).unwrap_or(&target_volume), target_volume, dt, 16.0)
        };
        st.anim.volume.insert(id.clone(), shown);
        let s = c.slider;
        let cy = (s[1] + s[3] / 2.0).floor();
        rrect(host, b, [s[0] + 2.0, cy - 1.5, s[2] - 4.0, 3.0], 1.5, TRACK);
        let filled = (s[2] - 4.0) * shown / 100.0;
        let fill_rgb = if row.own || row.inherited { rgb } else { DIM };
        rrect(host, b, [s[0] + 2.0, cy - 1.5, filled, 3.0], 1.5, argb(fill_rgb));
        let grow = if dragging || (hover && ui::contains(s, mouse)) { 1.0 } else { 0.0 };
        rrect(
            host,
            b,
            [s[0] + 2.0 + filled - 2.5 - grow * 0.5, cy - 5.0 - grow, 5.0 + grow, 10.0 + grow * 2.0],
            2.5,
            THUMB,
        );

        let pct = format!("{}%", row.volume);
        host.text(b, &pct, [c.pct[0] + c.pct[2] - host.text_width(&pct), r[1] + ui::MID_Y], rgb, false);
        if row.own {
            if hover && ui::contains(c.reset, mouse) {
                rrect(host, b, [c.reset[0] - 1.0, r[1] + 4.0, 16.0, r[3] - 8.0], 3.0, ROW_HOVER);
            }
            icon(host, b, Icon::Reset, Tone::Own, [c.reset[0] + 1.0, c.reset[1], ICON_SIZE, c.reset[3]]);
        }
        if let Key::Sound(sid) = &row.key {
            let timer_hover = hover && ui::contains(c.timer, mouse);
            if timer_hover {
                rrect(host, b, [c.timer[0] - 1.0, r[1] + 4.0, 16.0, r[3] - 8.0], 3.0, ROW_HOVER);
            }
            let open = st.list.drawer.as_deref() == Some(sid.as_str());
            let tone = if row.cooldown > 0 {
                Tone::Own
            } else if open || timer_hover {
                Tone::Text
            } else {
                Tone::Faint
            };
            icon(host, b, Icon::Timer, tone, [c.timer[0] + 1.0, c.timer[1], ICON_SIZE, c.timer[3]]);
        }
    }
    dump_rows(st, &rows, &lay, now);
    st.anim.y.retain(|k, _| seen.contains(k));
    st.anim.reveal.retain(|k, _| seen.contains(k));
    st.anim.hover.retain(|k, _| seen.contains(k));
    st.anim.volume.retain(|k, _| seen.contains(k));

    draw_footer(st, host, b, &lay, mouse);
    draw_sort_menu(st, host, b, &lay, mouse);
    if !lay.show_freeze {
        return;
    }
    // Freeze: the button changes colour smoothly.
    let f = lay.freeze;
    st.anim.freeze = approach(st.anim.freeze, if st.list.frozen { 1.0 } else { 0.0 }, dt, 16.0);
    let mut colour = mix(FREEZE_OFF, FREEZE_ON, st.anim.freeze);
    if ui::contains(f, mouse) {
        colour = mix(colour, 0xFF_FF_FF_FF, 0.08);
    }
    rrect(host, b, f, 4.0, colour);
    let (label_rgb, tone) = if st.anim.freeze > 0.5 { (ICE_TEXT, Tone::IceText) } else { (TEXT, Tone::Text) };
    icon(host, b, Icon::Snowflake, tone, [f[0] + 6.0, f[1], ICON_SIZE, f[3]]);
    let label = if st.list.frozen { tr("freeze.on") } else { tr("freeze.off") };
    host.text(b, label, [f[0] + 22.0, f[1] + 6.0], label_rgb, false);
}
