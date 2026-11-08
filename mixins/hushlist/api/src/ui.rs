// Hushlist screen model: which rows the Recent list shows, where everything is
// drawn, and what a click hits. No game types, so it is tested standalone; the
// trait mod (lib.rs) only draws these rectangles and forwards input.
use crate::core::{Settings, group_key};
use std::collections::HashMap;

/// One row's height in GUI pixels: a name line and a detail line.
pub const ROW_H: f32 = 22.0;
/// Gap between rows.
pub const ROW_GAP: f32 = 2.0;
/// Extra left inset of a sound shown inside its group.
pub const CHILD_INDENT: f32 = 12.0;
/// A start counts as fresh (row highlight) this long, in seconds.
pub const FLASH_SECS: f32 = 0.9;
/// Groups of sounds that usually play for a long time; they start collapsed.
pub const CONSTANT_FAMILIES: [&str; 4] = ["weather", "music", "music_disc", "ambient"];

/// A row's identity: a group (by group key) or a sound (by sound event ID).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Group(String),
    Sound(String),
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
}

fn by_id<'a>(heard: &'a [HeardView]) -> HashMap<&'a str, &'a HeardView> {
    heard.iter().map(|h| (h.id.as_str(), h)).collect()
}

/// Live order: playing entries first, then by latest start.
pub fn entries(heard: &[HeardView]) -> Vec<Entry> {
    let mut groups: Vec<(String, Vec<&HeardView>)> = Vec::new();
    let mut out: Vec<(Entry, bool, f32)> = Vec::new();
    for h in heard {
        match group_key(&h.id) {
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
pub fn snapshot(heard: &[HeardView]) -> Snapshot {
    let es = entries(heard);
    Snapshot {
        order: es.iter().map(|e| e.key.clone()).collect(),
        kids: es
            .iter()
            .filter_map(|e| match &e.key {
                Key::Group(g) => Some((g.clone(), e.kids.clone())),
                Key::Sound(_) => None,
            })
            .collect(),
    }
}

/// The rows to draw and, while frozen, how many heard sounds the frozen order leaves out.
pub fn build(heard: &[HeardView], settings: &Settings, st: &ListState) -> (Vec<Row>, usize) {
    let live = entries(heard);
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
                    Key::Sound(_) => e.clone(),
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
                });
                if open {
                    rows.extend(kids.iter().map(|k| sound_row(k, settings, true)));
                }
            }
        }
    }
    (rows, fresh)
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
    }
}

/// `[x, y, width, height]` in GUI pixels.
pub type Rect = [f32; 4];

pub fn contains(r: Rect, p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

/// Where the screen's parts are, for a GUI of `width` × `height`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub panel: Rect,
    pub title: [f32; 2],
    pub close: Rect,
    pub tabs: Rect,
    pub banner: Option<Rect>,
    pub list: Rect,
    pub freeze: Rect,
}

pub fn layout(width: f32, height: f32, banner: bool) -> Layout {
    let w = (width - 24.0).clamp(300.0, 640.0).min(width);
    let h = (height - 24.0).max(160.0).min(height);
    let x = ((width - w) / 2.0).floor();
    let y = ((height - h) / 2.0).floor();
    let tabs = [x + 6.0, y + 22.0, w - 12.0, 14.0];
    let mut top = y + 40.0;
    let banner = banner.then(|| {
        let r = [x + 6.0, top, w - 12.0, 14.0];
        top += 16.0;
        r
    });
    let freeze = [x + w - 92.0, y + h - 22.0, 86.0, 18.0];
    Layout {
        panel: [x, y, w, h],
        title: [x + 8.0, y + 7.0],
        close: [x + w - 20.0, y + 4.0, 14.0, 14.0],
        tabs,
        banner,
        list: [x + 6.0, top, w - 12.0, (y + h - 26.0 - top).max(0.0)],
        freeze,
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
}

/// Row parts, right to left. The slider grows with the screen.
pub fn columns(row: Rect, child: bool) -> Cols {
    let [x, y, w, h] = row;
    let reset = [x + w - 16.0, y, 14.0, h];
    let pct = [reset[0] - 30.0, y, 28.0, h];
    let slider_w = (w * 0.28).clamp(70.0, 170.0);
    let slider = [pct[0] - 4.0 - slider_w, y, slider_w, h];
    let speaker = [slider[0] - 18.0, y, 14.0, h];
    let when = [speaker[0] - 42.0, y, 38.0, h];
    let left = x + 2.0 + if child { CHILD_INDENT } else { 0.0 };
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
    if contains(lay.close, p) {
        return Hit::Close;
    }
    if contains(lay.freeze, p) {
        return Hit::Freeze;
    }
    if lay.banner.is_some_and(|b| contains(b, p)) {
        return Hit::Banner;
    }
    for (i, r) in visible(rows.len(), lay.list, scroll) {
        if !contains(r, p) {
            continue;
        }
        let c = columns(r, rows[i].child);
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
        }
    }

    fn keys(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match &r.key {
                Key::Group(g) => format!("G:{g}"),
                Key::Sound(s) => format!("{}{s}", if r.child { "  " } else { "S:" }),
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
        let (rows, fresh) = build(&heard, &Settings::default(), &ListState::default());
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
            live: Some(snapshot(&before)),
            ..ListState::default()
        };
        let after = [h("entity.chicken.ambient", 0.2, false), h("entity.cow.ambient", 3.0, false)];
        let (rows, fresh) = build(&after, &Settings::default(), &st);
        assert_eq!(keys(&rows)[0], "G:entity.cow");
        assert_eq!(fresh, 0);
    }

    #[test]
    fn constant_families_start_collapsed() {
        let heard = [h("music.overworld.forest", 3.0, false), h("block.portal.ambient", 1.0, false)];
        let (rows, _) = build(&heard, &Settings::default(), &ListState::default());
        assert_eq!(keys(&rows), ["G:block.portal", "  block.portal.ambient", "G:music"]);
    }

    #[test]
    fn player_choice_overrides_default_expansion() {
        let heard = [h("weather.rain", 0.5, true), h("entity.cow.ambient", 1.0, false)];
        let mut st = ListState::default();
        st.open.insert("weather".into(), true);
        st.open.insert("entity.cow".into(), false);
        let (rows, _) = build(&heard, &Settings::default(), &st);
        assert_eq!(keys(&rows), ["G:weather", "  weather.rain", "G:entity.cow"]);
    }

    #[test]
    fn volumes_show_own_and_inherited_values() {
        let heard = [h("entity.chicken.ambient", 1.0, false), h("entity.chicken.egg", 2.0, false)];
        let s = Settings::parse("group entity.chicken 30\nsound entity.chicken.egg 50\n");
        let (rows, _) = build(&heard, &s, &ListState::default());
        assert_eq!((rows[0].volume, rows[0].own), (30, true));
        assert_eq!((rows[1].volume, rows[1].own, rows[1].inherited), (30, false, true));
        assert_eq!((rows[2].volume, rows[2].own, rows[2].inherited), (50, true, false));
    }

    #[test]
    fn frozen_list_keeps_order_and_counts_new_sounds() {
        let before = [h("entity.cow.ambient", 1.0, false), h("entity.chicken.ambient", 5.0, false)];
        let mut st = ListState {
            frozen: true,
            snapshot: Some(snapshot(&before)),
            ..ListState::default()
        };
        // Chickens play again, a chick and a zombie are new.
        let after = [
            h("entity.chicken.ambient", 0.2, false),
            h("entity.cow.ambient", 3.0, false),
            h("entity.chicken.step", 0.5, false),
            h("entity.zombie.ambient", 0.1, false),
        ];
        let (rows, fresh) = build(&after, &Settings::default(), &st);
        assert_eq!(
            keys(&rows),
            ["G:entity.cow", "  entity.cow.ambient", "G:entity.chicken", "  entity.chicken.ambient"]
        );
        assert_eq!(fresh, 2);
        // The banner applies them once; the list stays frozen.
        st.snapshot = Some(snapshot(&after));
        let (rows, fresh) = build(&after, &Settings::default(), &st);
        assert_eq!(fresh, 0);
        assert_eq!(keys(&rows)[0], "G:entity.zombie");
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn layout_parts_do_not_overlap() {
        for (w, h) in [(427.0, 240.0), (640.0, 360.0), (960.0, 540.0)] {
            let lay = layout(w, h, true);
            assert!(lay.panel[0] >= 0.0 && lay.panel[0] + lay.panel[2] <= w);
            let b = lay.banner.unwrap();
            assert!(b[1] + b[3] <= lay.list[1]);
            assert!(lay.list[1] + lay.list[3] <= lay.freeze[1]);
            for child in [false, true] {
                let c = columns([lay.list[0], 0.0, lay.list[2], ROW_H], child);
                let order = [c.ind, c.name, c.when, c.speaker, c.slider, c.pct, c.reset];
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
        let (rows, _) = build(&heard, &s, &ListState::default());
        let lay = layout(640.0, 360.0, false);
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
