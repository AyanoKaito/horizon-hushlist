//! Hushlist trait mod: per-sound volume with a live list of recent sounds.
//! The launcher links this crate into the game and calls `hooks()`.
mod core;
mod i18n;
mod ui;

use crate::core::{Engine, MOD_ID, MOD_VERSION, SETTINGS_FILE, Settings, group_key, normalize, nudged};
use crate::ui::{Hit, HeardView, Key, ListState, Rect, Row, approach, fade, mix};
use horizon_client::lang;
use horizon_client::mod_api::{
    ClientTickPostEvent, Flow, Hooks, Host, HudBatch, KeyEvent, ModsEvent, NativeMod,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent,
    ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, SoundPlayEvent,
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
    Play = 5,
    Snowflake = 6,
    Close = 7,
    ChevronRight = 8,
    ChevronDown = 9,
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
    /// When the live order was last refreshed.
    live_at: Option<Instant>,
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
            })
            .collect()
    }

    fn rows(&mut self, now: Instant) -> (Vec<HeardView>, Vec<Row>, usize) {
        let views = self.views(now);
        // The live order waits while a slider is held, so the row does not move under the pointer.
        if !self.list.frozen
            && self.drag.is_none()
            && self.live_at.is_none_or(|t| now.saturating_duration_since(t) >= LIVE_REORDER)
        {
            self.list.live = Some(ui::snapshot(&views));
            self.live_at = Some(now);
        }
        let (mut rows, mut fresh) = ui::build(&views, &self.engine.settings, &self.list, &self.catalog);
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
            (rows, fresh) = ui::build(&views, &self.engine.settings, &self.list, &self.catalog);
        }
        (views, rows, fresh)
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
            Key::Sound(id) => s.sounds.insert(id.clone(), value),
            Key::Group(g) => s.groups.insert(g.clone(), value),
            Key::More(_) => return,
        };
        self.changed(now);
    }

    fn reset(&mut self, key: &Key, now: Instant) {
        let s = &mut self.engine.settings;
        match key {
            Key::Sound(id) => s.sounds.remove(id),
            Key::Group(g) => s.groups.remove(g),
            Key::More(_) => return,
        };
        self.list.before_mute.remove(key);
        self.changed(now);
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
        event.mods.push(NativeMod {
            id: MOD_ID.to_owned(),
            title: format!("Hushlist {MOD_VERSION}"),
            description: "Turn down individual sounds: a live list of what you hear, with a volume for each sound and each group.".to_owned(),
            keys: vec![[OPEN_KEY.to_owned(), "Open Hushlist".to_owned(), OPEN_KEY_DEFAULT.to_owned()]],
        });
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
            self.lock().upload_icons(host);
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
        let lay = ui::layout(st.gui[0], st.gui[1], st.list.frozen && fresh > 0);
        match ui::hit(&lay, &rows, st.list.scroll, event.screen.mouse) {
            Hit::Close => st.close = true,
            Hit::Freeze => {
                st.list.frozen = !st.list.frozen;
                st.list.snapshot = st.list.frozen.then(|| ui::snapshot(&views));
            }
            Hit::Banner => st.list.snapshot = Some(ui::snapshot(&views)),
            Hit::Speaker(i) => st.toggle_mute(&rows[i], now),
            Hit::Slider(i, v) => {
                st.drag = Some(rows[i].key.clone());
                st.set_volume(&rows[i].key, v, now);
            }
            Hit::Reset(i) => st.reset(&rows[i].key, now),
            Hit::Row(i) => match &rows[i].key {
                Key::Group(g) => {
                    let open = !rows[i].open;
                    st.list.open.insert(g.clone(), open);
                }
                Key::Sound(id) => st.preview = Some(id.clone()),
                Key::More(g) => {
                    if !st.list.show_all.remove(g) {
                        st.list.show_all.insert(g.clone());
                    }
                }
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
        let lay = ui::layout(st.gui[0], st.gui[1], st.list.frozen && fresh > 0);
        if let Some((i, r)) = ui::visible(rows.len(), lay.list, st.list.scroll)
            .into_iter()
            .find(|(i, _)| rows[*i].key == key)
        {
            let slider = ui::columns(r, rows[i].child).slider;
            let v = ui::slider_value(slider, event.screen.mouse[0]);
            st.set_volume(&key, v, now);
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
fn dump_rows(st: &mut State, rows: &[Row], list: Rect, now: Instant) {
    let Some(trace) = std::env::var_os("HUSHLIST_TRACE") else { return };
    if st.rows_dumped.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(250)) {
        return;
    }
    st.rows_dumped = Some(now);
    let items: Vec<String> = ui::visible(rows.len(), list, st.list.scroll)
        .into_iter()
        .map(|(i, r)| {
            format!(
                "{{\"id\":\"{}\",\"child\":{},\"own\":{},\"volume\":{},\"count\":{},\"rect\":[{},{},{},{}]}}",
                row_id(&rows[i]), rows[i].child, rows[i].own, rows[i].volume, rows[i].count, r[0], r[1], r[2], r[3]
            )
        })
        .collect();
    let mut path = std::path::PathBuf::from(trace);
    path.as_mut_os_string().push(".rows.json");
    let _ = std::fs::write(path, format!("[{}]", items.join(",")));
}

fn row_id(row: &Row) -> String {
    match &row.key {
        Key::Group(g) => format!("g:{g}"),
        Key::Sound(s) => format!("{}:{s}", if row.child { "c" } else { "s" }),
        Key::More(g) => format!("m:{g}"),
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
    let lay = ui::layout(b.width, b.height, st.list.frozen && fresh > 0);
    st.list.scroll = st.list.scroll.min(ui::max_scroll(rows.len(), lay.list));
    let p = lay.panel;

    rrect(host, b, p, 6.0, fade(BG, open_alpha));
    host.fill(b, [p[0] + 6.0, p[1], p[2] - 12.0, 1.0], fade(EDGE, open_alpha));

    host.fill(b, [lay.title[0], lay.title[1] + 1.0, 4.0, 4.0], argb(AMBER));
    host.text(b, "Hushlist", [lay.title[0] + 8.0, lay.title[1]], TEXT, false);
    let close_hover = ui::contains(lay.close, mouse);
    if close_hover {
        rrect(host, b, lay.close, 3.0, ROW_HOVER);
    }
    let close_tone = if close_hover { Tone::Text } else { Tone::Dim };
    icon(host, b, Icon::Close, close_tone, [lay.close[0] + 1.0, lay.close[1], ICON_SIZE, lay.close[3]]);

    // Tabs: only Recent exists so far.
    let t = lay.tabs;
    let mut x = t[0] + 2.0;
    for (i, label) in [tr("tab.recent"), tr("tab.all"), tr("tab.changed"), tr("tab.zones")]
        .into_iter()
        .enumerate()
    {
        let w = host.text_width(label);
        host.text(b, label, [x, t[1] + 2.0], if i == 0 { TEXT } else { FAINT }, false);
        if i == 0 {
            rrect(host, b, [x - 1.0, t[1] + t[3] - 2.0, w + 2.0, 2.0], 1.0, argb(TEXT));
        }
        x += w + 14.0;
    }
    host.fill(b, [p[0] + 1.0, t[1] + t[3], p[2] - 2.0, 1.0], EDGE);

    st.anim.banner = approach(st.anim.banner, if lay.banner.is_some() { 1.0 } else { 0.0 }, dt, 16.0);
    if let Some(r) = lay.banner {
        rrect(host, b, r, 3.0, fade(BANNER, st.anim.banner));
        if ui::contains(r, mouse) {
            rrect(host, b, r, 3.0, 0x14_FF_FF_FF);
        }
        let text = i18n::with_n(&code, if fresh == 1 { "banner.new.one" } else { "banner.new.other" }, fresh);
        host.text(b, &text, [r[0] + 6.0, r[1] + 3.0], AMBER, false);
        let hint = tr("banner.hint");
        host.text(b, hint, [r[0] + r[2] - host.text_width(hint) - 6.0, r[1] + 3.0], DIM, false);
    }

    if rows.is_empty() {
        let l = lay.list;
        let a = tr("empty.title");
        let c = tr("empty.text");
        host.text(b, a, [l[0] + (l[2] - host.text_width(a)) / 2.0, l[1] + 30.0], TEXT, false);
        host.text(b, c, [l[0] + (l[2] - host.text_width(c)) / 2.0, l[1] + 44.0], DIM, false);
    }

    let list = lay.list;
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
        if let Key::More(_) = &row.key {
            // A quiet button, not a sound row: no background, a thin outline, one label.
            let label = if row.open {
                tr("group.hide_unheard").to_owned()
            } else {
                i18n::with_n(&code, "group.more", row.count)
            };
            let w = host.text_width(&label) + 16.0;
            let btn = [c.name[0] - 4.0, r[1] + 4.0, w, r[3] - 8.0];
            let edge = fade(mix(EDGE, 0xFF_FF_FF_FF, 0.15 * h), reveal);
            rrect(host, b, btn, 3.0, edge);
            rrect(host, b, [btn[0] + 1.0, btn[1] + 1.0, btn[2] - 2.0, btn[3] - 2.0], 2.0, fade(mix(BG, ROW_HOVER, h), reveal));
            let rgb = if hover { TEXT } else { DIM };
            host.text(b, &label, [btn[0] + 8.0, btn[1] + 3.0], rgb, false);
            continue;
        }
        rrect(host, b, bg_rect, 3.0, fade(bg, reveal));

        let (name, detail) = match &row.key {
            Key::Group(g) => {
                let chevron = if row.open { Icon::ChevronDown } else { Icon::ChevronRight };
                icon(host, b, chevron, Tone::Dim, c.ind);
                let detail = if row.open {
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
                } else {
                    group_key(sid).map_or_else(|| tr("sound.no_group").to_owned(), |g| group_name(&g)) + " · "
                };
                (sound_name(&st.subtitles, sid), format!("{source}minecraft:{sid}"))
            }
            Key::More(_) => unreachable!("drawn above"),
        };
        // The game font has no italics for mods, so a name made from the ID is drawn dimmer.
        let technical = matches!(&row.key, Key::Sound(sid) if technical_name(&st.subtitles, sid));
        let name_rgb = if row.volume == 0 { FAINT } else if technical { DIM } else { TEXT };
        host.text(b, &fit(host, &name, c.name[2]), [c.name[0], r[1] + 3.0], name_rgb, false);
        host.text(b, &fit(host, &detail, c.name[2]), [c.name[0], r[1] + 12.0], FAINT, false);

        let when = ui::when_text(row, &code);
        let when_rgb = if row.playing || row.flash { AMBER } else { DIM };
        host.text(b, &when, [c.when[0] + c.when[2] - host.text_width(&when), r[1] + 3.0], when_rgb, false);
        if !row.playing && row.count > 0 {
            let count = format!("×{}", row.count);
            host.text(b, &count, [c.when[0] + c.when[2] - host.text_width(&count), r[1] + 12.0], FAINT, false);
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
            rrect(host, b, [c.speaker[0] - 2.0, r[1] + 3.0, 16.0, r[3] - 6.0], 3.0, ROW_HOVER);
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
        host.text(b, &pct, [c.pct[0] + c.pct[2] - host.text_width(&pct), r[1] + 7.0], rgb, false);
        if row.own {
            if hover && ui::contains(c.reset, mouse) {
                rrect(host, b, [c.reset[0] - 1.0, r[1] + 3.0, 16.0, r[3] - 6.0], 3.0, ROW_HOVER);
            }
            icon(host, b, Icon::Reset, Tone::Own, [c.reset[0] + 1.0, c.reset[1], ICON_SIZE, c.reset[3]]);
        }
    }
    dump_rows(st, &rows, list, now);
    st.anim.y.retain(|k, _| seen.contains(k));
    st.anim.reveal.retain(|k, _| seen.contains(k));
    st.anim.hover.retain(|k, _| seen.contains(k));
    st.anim.volume.retain(|k, _| seen.contains(k));

    // Freeze: the button changes colour smoothly.
    let f = lay.freeze;
    st.anim.freeze = approach(st.anim.freeze, if st.list.frozen { 1.0 } else { 0.0 }, dt, 16.0);
    let mut colour = mix(FREEZE_OFF, FREEZE_ON, st.anim.freeze);
    if ui::contains(f, mouse) {
        colour = mix(colour, 0xFF_FF_FF_FF, 0.08);
    }
    rrect(host, b, f, 4.0, colour);
    let (label_rgb, tone) = if st.anim.freeze > 0.5 { (ICE_TEXT, Tone::IceText) } else { (TEXT, Tone::Text) };
    icon(host, b, Icon::Snowflake, tone, [f[0] + 4.0, f[1], ICON_SIZE, f[3]]);
    let label = if st.list.frozen { tr("freeze.on") } else { tr("freeze.off") };
    host.text(b, label, [f[0] + 20.0, f[1] + 5.0], label_rgb, false);
}
