//! Zones: places whose sounds get their own volumes. No game types, so
//! `cargo test` covers it.
//!
//! A zone belongs to a profile and to one world or server. It is a column of
//! blocks (a circle or a square around a centre) between two heights. A sound
//! that starts inside a zone uses the zone's settings; what the zone leaves
//! untouched follows the profile's settings. Where zones overlap, the smallest
//! one wins.
//!
//! File: `profiles/<profile>.zones.txt`, one block per zone:
//! `zone <id>`, `world <key>`, `name <name>`, `shape <circle|square> <x> <z> <r>`,
//! `height <low> <high>`, the zone's settings lines, `end`.

use crate::core::Settings;
use crate::ui::{Rect, contains};

/// Most zones in one profile.
pub const ZONES_MAX: usize = 50;
/// Largest radius in blocks (a zone up to 129 by 129).
pub const RADIUS_MAX: i32 = 64;
pub const RADIUS_MIN: i32 = 2;
/// Height above and below the player when a zone is made.
pub const HEIGHT_HALF: i32 = 8;
/// Zone colours, by the zone's place in the profile.
pub const COLORS: [u32; 5] = [0x6F_F0_DA, 0xF5_B1_42, 0xB7_9C_FF, 0xFF_8A_7A, 0x7F_C8_FF];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Circle,
    Square,
}

impl Shape {
    fn word(self) -> &'static str {
        match self {
            Shape::Circle => "circle",
            Shape::Square => "square",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Zone {
    pub id: u32,
    /// `sp:<save folder>` or `server:<address>`.
    pub world: String,
    pub name: String,
    pub shape: Shape,
    /// Centre block column.
    pub x: i32,
    pub z: i32,
    pub r: i32,
    /// Lowest and highest block y, both inside.
    pub low: i32,
    pub high: i32,
    pub settings: Settings,
}

/// Whether block column `(dx, dz)` from the centre is inside a shape of radius `r`.
pub fn inside(shape: Shape, r: i32, dx: i32, dz: i32) -> bool {
    match shape {
        // A little more than the exact circle, so a radius-r circle reaches r blocks along each axis
        // and its edge looks round in blocks.
        Shape::Circle => (dx * dx + dz * dz) as f32 <= (r * r) as f32 + r as f32 * 0.6,
        Shape::Square => dx.abs() <= r && dz.abs() <= r,
    }
}

impl Zone {
    pub fn holds_column(&self, bx: i32, bz: i32) -> bool {
        inside(self.shape, self.r, bx - self.x, bz - self.z)
    }

    /// Whether a sound at `pos` (world coordinates) comes from this zone.
    pub fn holds(&self, pos: [f64; 3]) -> bool {
        if !pos.iter().all(|v| v.is_finite()) {
            return false;
        }
        let (bx, by, bz) = (pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32);
        by >= self.low && by <= self.high && self.holds_column(bx, bz)
    }

    /// Block columns inside.
    pub fn area(&self) -> u32 {
        area(self.shape, self.r)
    }

    /// Horizontal distance in whole blocks from a point to the centre.
    pub fn distance(&self, pos: [f64; 3]) -> u32 {
        let dx = pos[0] - (f64::from(self.x) + 0.5);
        let dz = pos[2] - (f64::from(self.z) + 0.5);
        (dx * dx + dz * dz).sqrt().round() as u32
    }
}

pub fn area(shape: Shape, r: i32) -> u32 {
    let mut n = 0;
    for dz in -r..=r {
        for dx in -r..=r {
            if inside(shape, r, dx, dz) {
                n += 1;
            }
        }
    }
    n
}

/// The smallest zone of `world` that holds `pos`; the first one on a tie.
pub fn smallest_at(zones: &[Zone], world: &str, pos: [f64; 3]) -> Option<usize> {
    let mut best: Option<(usize, u32)> = None;
    for (i, z) in zones.iter().enumerate() {
        if z.world == world && z.holds(pos) {
            let a = z.area();
            if best.is_none_or(|(_, b)| a < b) {
                best = Some((i, a));
            }
        }
    }
    best.map(|(i, _)| i)
}

/// A zone's colour: by its place among the profile's zones.
pub fn color(index: usize) -> u32 {
    COLORS[index % COLORS.len()]
}

pub fn parse(text: &str) -> Vec<Zone> {
    let mut out = Vec::new();
    let mut cur: Option<(Zone, String)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(id) = line.strip_prefix("zone ") {
            cur = id.trim().parse().ok().map(|id| (Zone { id, ..Zone::default() }, String::new()));
            continue;
        }
        let Some((zone, body)) = cur.as_mut() else { continue };
        if line == "end" {
            let (mut zone, body) = cur.take().unwrap_or_default();
            zone.settings = Settings::parse(&body);
            if !zone.world.is_empty() && zone.r >= 1 && zone.low <= zone.high && !out.iter().any(|z: &Zone| z.id == zone.id) {
                zone.r = zone.r.min(RADIUS_MAX);
                out.push(zone);
            }
        } else if let Some(w) = line.strip_prefix("world ") {
            zone.world = w.to_owned();
        } else if let Some(n) = line.strip_prefix("name ") {
            zone.name = crate::profiles::clean_name(n);
        } else if let Some(s) = line.strip_prefix("shape ") {
            let f: Vec<&str> = s.split_whitespace().collect();
            if let [kind, x, z, r] = f[..] {
                zone.shape = if kind == "square" { Shape::Square } else { Shape::Circle };
                zone.x = x.parse().unwrap_or(0);
                zone.z = z.parse().unwrap_or(0);
                zone.r = r.parse().unwrap_or(0);
            }
        } else if let Some(h) = line.strip_prefix("height ") {
            let f: Vec<i32> = h.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            if let [low, high] = f[..] {
                zone.low = low;
                zone.high = high;
            }
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

pub fn to_text(zones: &[Zone]) -> String {
    let mut out = String::from("# Hushlist zones of one profile\n");
    for z in zones {
        out.push_str(&format!(
            "zone {}\nworld {}\nname {}\nshape {} {} {} {}\nheight {} {}\n",
            z.id, z.world, z.name, z.shape.word(), z.x, z.z, z.r, z.low, z.high
        ));
        for line in z.settings.to_text().lines().filter(|l| !l.starts_with('#')) {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("end\n");
    }
    out
}

pub fn next_id(zones: &[Zone]) -> u32 {
    zones.iter().map(|z| z.id).max().unwrap_or(0) + 1
}

// ---------- screen ----------

/// The zone being made or reshaped.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    /// None for a new zone.
    pub id: Option<u32>,
    pub name: String,
    /// The name field has the keyboard.
    pub typing: bool,
    pub shape: Shape,
    pub x: i32,
    pub z: i32,
    pub r: i32,
    pub low: i32,
    pub high: i32,
    /// Map scale: 64 by 64 blocks when near, 128 by 128 when far.
    pub far: bool,
}

impl Editor {
    pub fn span(&self) -> i32 {
        if self.far { 128 } else { 64 }
    }
    pub fn area(&self) -> u32 {
        area(self.shape, self.r)
    }
}

/// Measured text widths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measure {
    pub new_zone: f32,
    pub circle: f32,
    pub square: f32,
    pub near: f32,
    pub far: f32,
    pub save: f32,
    pub cancel: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListLayout {
    /// "N of 50 zones".
    pub count_at: [f32; 2],
    pub new_zone: Rect,
    pub cards: Vec<(usize, Rect)>,
    /// The area the cards scroll in.
    pub area: Rect,
}

pub const CARD_H: f32 = 30.0;
pub const CARD_GAP: f32 = 3.0;

/// The Zones tab's list in `area` (under the tabs). `scroll` in GUI pixels.
pub fn list_layout(area: Rect, m: &Measure, count: usize, scroll: f32) -> ListLayout {
    let [x, y, w, h] = area;
    let new_zone = [x + w - m.new_zone - 18.0, y, m.new_zone + 18.0, 18.0];
    let top = y + 24.0;
    let bottom = y + h;
    let mut cards = Vec::new();
    for i in 0..count {
        let cy = top + i as f32 * (CARD_H + CARD_GAP) - scroll;
        if cy + CARD_H <= top - 0.5 || cy >= bottom {
            continue;
        }
        if cy < top - 0.5 || cy + CARD_H > bottom + 0.5 {
            continue;
        }
        cards.push((i, [x, cy, w, CARD_H]));
    }
    ListLayout { count_at: [x + 2.0, y + 5.0], new_zone, cards, area: [x, top, w, (bottom - top).max(0.0)] }
}

pub fn list_max_scroll(count: usize, area: Rect) -> f32 {
    (count as f32 * (CARD_H + CARD_GAP) - CARD_GAP - area[3]).max(0.0)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorLayout {
    pub map: Rect,
    pub form_x: f32,
    pub title_at: [f32; 2],
    pub name_label_at: [f32; 2],
    pub name: Rect,
    pub circle: Rect,
    pub square: Rect,
    pub zoom_label_at: [f32; 2],
    pub near: Rect,
    pub far: Rect,
    pub radius_label_at: [f32; 2],
    pub slider: Rect,
    pub height_at: [f32; 2],
    pub area_at: [f32; 2],
    pub save: Rect,
    pub cancel: Rect,
}

/// The editor in `area`: the map on the left, the form on the right.
pub fn editor_layout(area: Rect, m: &Measure) -> EditorLayout {
    let [x, y, w, h] = area;
    let side = h.min(w * 0.5).min(256.0).floor();
    let map = [x, y, side, side];
    let fx = x + side + 12.0;
    let fw = (x + w - fx).max(80.0);
    let seg = |at: f32, top: f32, a: f32, b: f32| -> (Rect, Rect) {
        let r1 = [at, top, a + 16.0, 16.0];
        let r2 = [at + a + 16.0, top, b + 16.0, 16.0];
        (r1, r2)
    };
    let mut cy = y + 2.0;
    let title_at = [fx, cy];
    cy += 16.0;
    let name_label_at = [fx, cy];
    cy += 11.0;
    let name = [fx, cy, fw.min(220.0), 16.0];
    cy += 24.0;
    let (circle, square) = seg(fx, cy, m.circle, m.square);
    cy += 26.0;
    let zoom_label_at = [fx, cy];
    cy += 11.0;
    let (near, far) = seg(fx, cy, m.near, m.far);
    cy += 26.0;
    let radius_label_at = [fx, cy];
    cy += 12.0;
    let slider = [fx, cy, fw.min(220.0), 12.0];
    cy += 22.0;
    let height_at = [fx, cy];
    cy += 13.0;
    let area_at = [fx, cy];
    let foot = y + h - 20.0;
    let cancel = [x + w - m.cancel - 18.0, foot, m.cancel + 18.0, 20.0];
    let save = [cancel[0] - 5.0 - m.save - 18.0, foot, m.save + 18.0, 20.0];
    EditorLayout {
        map,
        form_x: fx,
        title_at,
        name_label_at,
        name,
        circle,
        square,
        zoom_label_at,
        near,
        far,
        radius_label_at,
        slider,
        height_at,
        area_at,
        save,
        cancel,
    }
}

/// Radius for a pointer at `px` on the slider.
pub fn slider_radius(slider: Rect, px: f32) -> i32 {
    let t = ((px - slider[0] - 2.0) / (slider[2] - 4.0)).clamp(0.0, 1.0);
    RADIUS_MIN + (t * (RADIUS_MAX - RADIUS_MIN) as f32).round() as i32
}

pub fn slider_pos(slider: Rect, r: i32) -> f32 {
    let t = (r - RADIUS_MIN) as f32 / (RADIUS_MAX - RADIUS_MIN) as f32;
    slider[0] + 2.0 + t * (slider[2] - 4.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorHit {
    Name,
    Circle,
    Square,
    Near,
    Far,
    Slider,
    Save,
    Cancel,
    Nothing,
}

pub fn editor_hit(lay: &EditorLayout, at: [f32; 2]) -> EditorHit {
    let slider = [lay.slider[0] - 2.0, lay.slider[1] - 3.0, lay.slider[2] + 4.0, lay.slider[3] + 6.0];
    [
        (lay.name, EditorHit::Name),
        (lay.circle, EditorHit::Circle),
        (lay.square, EditorHit::Square),
        (lay.near, EditorHit::Near),
        (lay.far, EditorHit::Far),
        (slider, EditorHit::Slider),
        (lay.save, EditorHit::Save),
        (lay.cancel, EditorHit::Cancel),
    ]
    .into_iter()
    .find(|(r, _)| contains(*r, at))
    .map_or(EditorHit::Nothing, |(_, h)| h)
}

/// The header of an open zone, in the tab row: back, then (after the name and the
/// sub-tabs) "Edit shape" and "Delete..." on the right, or the delete confirmation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadLayout {
    pub back: Rect,
    pub name_at: [f32; 2],
    pub name_room: f32,
    /// Where the sub-tabs start.
    pub tabs_x: f32,
    pub edit: Option<Rect>,
    pub delete: Option<Rect>,
    pub yes: Option<Rect>,
    pub no: Option<Rect>,
}

/// `row` is the tab row; widths: back, name, edit, delete, yes, no.
pub fn head_layout(row: Rect, back_w: f32, name_w: f32, buttons: [f32; 4], confirm: bool) -> HeadLayout {
    let [x, y, w, _] = row;
    let back = [x, y - 1.0, back_w + 16.0, 16.0];
    let name_room = name_w.min(110.0);
    let name_at = [back[0] + back[2] + 14.0, y + 3.0];
    let tabs_x = name_at[0] + name_room + 14.0;
    let right = x + w;
    let [edit_w, del_w, yes_w, no_w] = buttons.map(|b| b + 16.0);
    let mut out = HeadLayout { back, name_at, name_room, tabs_x, ..HeadLayout::default() };
    if confirm {
        let no = [right - no_w, y - 1.0, no_w, 16.0];
        out.yes = Some([no[0] - 5.0 - yes_w, y - 1.0, yes_w, 16.0]);
        out.no = Some(no);
    } else {
        let delete = [right - del_w, y - 1.0, del_w, 16.0];
        out.edit = Some([delete[0] - 5.0 - edit_w, y - 1.0, edit_w, 16.0]);
        out.delete = Some(delete);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(shape: Shape, x: i32, z: i32, r: i32) -> Zone {
        Zone { id: 1, world: "sp:w".into(), name: "Farm".into(), shape, x, z, r, low: 60, high: 76, ..Zone::default() }
    }

    #[test]
    fn circle_reaches_the_radius_along_axes_but_not_the_corners() {
        let z = zone(Shape::Circle, 0, 0, 9);
        assert!(z.holds_column(9, 0) && z.holds_column(0, -9));
        assert!(!z.holds_column(9, 9));
        assert!(!z.holds_column(10, 0));
        let s = zone(Shape::Square, 0, 0, 4);
        assert!(s.holds_column(4, -4) && !s.holds_column(5, 0));
        assert_eq!(s.area(), 81);
    }

    #[test]
    fn a_sound_counts_by_its_block_and_height() {
        let z = zone(Shape::Square, 10, -5, 2);
        assert!(z.holds([12.9, 60.0, -3.1]));
        assert!(!z.holds([13.0, 60.0, -5.0]));
        assert!(!z.holds([10.0, 77.0, -5.0]), "above");
        assert!(!z.holds([10.0, 59.9, -5.0]), "below");
        assert!(!z.holds([f64::NAN, 60.0, -5.0]));
    }

    #[test]
    fn the_smallest_zone_of_this_world_wins() {
        let big = Zone { id: 1, ..zone(Shape::Square, 0, 0, 10) };
        let small = Zone { id: 2, ..zone(Shape::Circle, 2, 2, 3) };
        let other = Zone { id: 3, world: "server:x".into(), ..zone(Shape::Square, 2, 2, 1) };
        let zones = vec![big, small, other];
        assert_eq!(smallest_at(&zones, "sp:w", [2.5, 64.0, 2.5]), Some(1));
        assert_eq!(smallest_at(&zones, "sp:w", [-8.5, 64.0, 0.5]), Some(0));
        assert_eq!(smallest_at(&zones, "sp:w", [40.0, 64.0, 0.0]), None);
        assert_eq!(smallest_at(&zones, "server:x", [2.5, 64.0, 2.5]), Some(2));
    }

    #[test]
    fn file_round_trips_zones_with_their_settings() {
        let mut a = zone(Shape::Circle, -120, 44, 9);
        a.settings.groups.insert("entity.chicken".into(), 0);
        a.settings.sounds.insert("entity.cow.ambient".into(), 30);
        let b = Zone { id: 4, world: "server:mc.example.com:25565".into(), name: "Store room".into(), ..zone(Shape::Square, 3, 3, 4) };
        let zones = vec![a, b];
        assert_eq!(parse(&to_text(&zones)), zones);
    }

    #[test]
    fn broken_blocks_are_skipped() {
        let text = "zone 1\nname No world\nshape circle 0 0 5\nheight 0 10\nend\nzone x\nend\nzone 2\nworld sp:a\nshape square 1 2 3\nheight 5 1\nend\n";
        assert!(parse(text).is_empty());
    }

    #[test]
    fn layering_untouched_sounds_follow_the_profile() {
        use crate::core::Engine;
        let mut e = Engine::default();
        e.settings.groups.insert("entity.chicken".into(), 50);
        e.settings.sounds.insert("entity.cow.ambient".into(), 40);
        let mut z = zone(Shape::Square, 0, 0, 5);
        z.settings.groups.insert("entity.chicken".into(), 0);
        e.zones = vec![z];
        e.world = Some("sp:w".into());
        let now = std::time::Instant::now();
        let inside = [0.5, 64.0, 0.5];
        let outside = [30.5, 64.0, 0.5];
        assert_eq!(e.decide("entity.chicken.ambient", 6, inside, false, true, now).factor, 0.0);
        assert_eq!(e.decide("entity.chicken.ambient", 6, outside, false, true, now).factor, 0.5);
        assert!((e.decide("entity.cow.ambient", 6, inside, false, true, now).factor - 0.4).abs() < 1e-6);
        // Relative sounds (music, the interface) come from no place.
        assert_eq!(e.decide("entity.chicken.ambient", 6, inside, true, true, now).factor, 0.5);
        // Another world: the zone does nothing.
        e.world = Some("sp:other".into());
        assert_eq!(e.factor("entity.chicken.ambient", inside, false), 0.5);
    }

    #[test]
    fn recent_here_keeps_a_sound_that_also_plays_outside() {
        use crate::core::Engine;
        let mut e = Engine::default();
        e.zones = vec![zone(Shape::Square, 0, 0, 5)];
        e.world = Some("sp:w".into());
        let t0 = std::time::Instant::now();
        e.decide("entity.chicken.ambient", 6, [0.5, 64.0, 0.5], false, true, t0);
        e.decide("entity.chicken.ambient", 6, [20.5, 64.0, 0.5], false, true, t0 + std::time::Duration::from_secs(1));
        let here = e.zone_recent.get(&1).expect("zone list");
        assert_eq!(here.list().len(), 1);
        assert_eq!(here.list()[0].count, 1);
        assert_eq!(e.recent.list()[0].count, 2);
    }

    #[test]
    fn display_settings_show_the_profile_under_the_zone() {
        let mut global = Settings::default();
        global.groups.insert("entity.cow".into(), 30);
        global.cooldowns.insert("entity.cow.ambient".into(), 10);
        let mut z = Settings::default();
        z.sounds.insert("entity.cow.step".into(), 0);
        z.base = Some(Box::new(global));
        assert_eq!(z.volume_percent("entity.cow.ambient"), 30);
        assert_eq!(z.volume_percent("entity.cow.step"), 0);
        assert_eq!(z.group_percent("entity.cow"), 30);
        assert_eq!(z.cooldown_secs("entity.cow.ambient"), 10);
        assert_eq!(z.count(), 1);
        assert!(!z.to_text().contains("group"));
    }

    #[test]
    fn list_cards_fit_and_scroll() {
        let m = Measure { new_zone: 60.0, ..Measure::default() };
        let lay = list_layout([0.0, 0.0, 400.0, 150.0], &m, 10, 0.0);
        assert!(lay.cards.iter().all(|(_, r)| r[1] + r[3] <= 150.0 + 0.5));
        assert_eq!(lay.cards[0].0, 0);
        let max = list_max_scroll(10, lay.area);
        let lay = list_layout([0.0, 0.0, 400.0, 150.0], &m, 10, max);
        assert_eq!(lay.cards.last().unwrap().0, 9);
    }

    #[test]
    fn editor_slider_maps_to_the_radius_range() {
        let s = [100.0, 0.0, 200.0, 12.0];
        assert_eq!(slider_radius(s, 0.0), RADIUS_MIN);
        assert_eq!(slider_radius(s, 1000.0), RADIUS_MAX);
        assert_eq!(slider_radius(s, slider_pos(s, 20)), 20);
        let lay = editor_layout([0.0, 0.0, 600.0, 270.0], &Measure { circle: 30.0, square: 40.0, near: 30.0, far: 30.0, save: 50.0, cancel: 40.0, new_zone: 0.0 });
        assert_eq!(lay.map[2], 256.0);
        assert!(lay.save[0] + lay.save[2] < lay.cancel[0]);
        assert_eq!(editor_hit(&lay, [lay.square[0] + 3.0, lay.square[1] + 3.0]), EditorHit::Square);
    }
}
