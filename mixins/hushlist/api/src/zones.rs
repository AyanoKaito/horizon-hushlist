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
//! `zone <id>`, `world <key>`, `name <name>`, `shape <circle|square|points> <x> <z> <r>`,
//! `point <x> <z>` lines for a zone drawn by points (block corners), `height <low> <high>`,
//! the zone's settings lines, `end`.

use crate::core::Settings;
use crate::ui::{Rect, contains};

/// Most zones in one profile.
pub const ZONES_MAX: usize = 50;
/// Largest radius in blocks (a zone up to 129 by 129).
pub const RADIUS_MAX: i32 = 64;
pub const RADIUS_MIN: i32 = 2;
/// Height above and below the player when a zone is made.
pub const HEIGHT_HALF: i32 = 8;
/// Most corners of a zone drawn by points.
pub const POINTS_MAX: usize = 10;
/// Zone colours, by the zone's place in the profile.
pub const COLORS: [u32; 5] = [0x6F_F0_DA, 0xF5_B1_42, 0xB7_9C_FF, 0xFF_8A_7A, 0x7F_C8_FF];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Circle,
    Square,
    /// A polygon through corners the player clicked on the map.
    Points,
}

impl Shape {
    fn word(self) -> &'static str {
        match self {
            Shape::Circle => "circle",
            Shape::Square => "square",
            Shape::Points => "points",
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
    /// Corners (block corner coordinates) of a zone drawn by points.
    pub points: Vec<[i32; 2]>,
    pub settings: Settings,
    /// Block columns inside, counted once (the smallest zone wins on every sound start).
    pub area_cache: std::cell::OnceCell<u32>,
}

/// Whether block column `(dx, dz)` from the centre is inside a circle or square of radius `r`.
pub fn inside(shape: Shape, r: i32, dx: i32, dz: i32) -> bool {
    match shape {
        // A little more than the exact circle, so a radius-r circle reaches r blocks along each axis
        // and its edge looks round in blocks.
        Shape::Circle => (dx * dx + dz * dz) as f32 <= (r * r) as f32 + r as f32 * 0.6,
        Shape::Square => dx.abs() <= r && dz.abs() <= r,
        Shape::Points => false,
    }
}

/// Whether block column `(bx, bz)` (its middle) is inside the polygon through `points`.
pub fn in_polygon(points: &[[i32; 2]], bx: i32, bz: i32) -> bool {
    if points.len() < 3 {
        return false;
    }
    let (px, pz) = (f64::from(bx) + 0.5, f64::from(bz) + 0.5);
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, zi) = (f64::from(points[i][0]), f64::from(points[i][1]));
        let (xj, zj) = (f64::from(points[j][0]), f64::from(points[j][1]));
        if (zi > pz) != (zj > pz) && px < (xj - xi) * (pz - zi) / (zj - zi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Columns a polygon can cover: `[min x, min z, max x, max z]`, max excluded.
fn bounds(points: &[[i32; 2]]) -> [i32; 4] {
    let mut b = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for p in points {
        b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
    }
    b
}

pub fn polygon_area(points: &[[i32; 2]]) -> u32 {
    if points.len() < 3 {
        return 0;
    }
    let b = bounds(points);
    let mut n = 0;
    for bz in b[1]..b[3] {
        for bx in b[0]..b[2] {
            if in_polygon(points, bx, bz) {
                n += 1;
            }
        }
    }
    n
}

/// Centre column and the radius that covers a polygon, for distances and the editor's map.
pub fn polygon_centre(points: &[[i32; 2]]) -> (i32, i32, i32) {
    let b = bounds(points);
    let r = ((b[2] - b[0]).max(b[3] - b[1]) + 1) / 2;
    ((b[0] + b[2]).div_euclid(2), (b[1] + b[3]).div_euclid(2), r.max(1))
}

impl Zone {
    pub fn holds_column(&self, bx: i32, bz: i32) -> bool {
        match self.shape {
            Shape::Points => in_polygon(&self.points, bx, bz),
            _ => inside(self.shape, self.r, bx - self.x, bz - self.z),
        }
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
        *self.area_cache.get_or_init(|| match self.shape {
            Shape::Points => polygon_area(&self.points),
            _ => area(self.shape, self.r),
        })
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

/// Height of the see-through wall that shows a zone's border in the world, in blocks.
pub const WALL_H: f64 = 1.5;

/// One straight piece of a zone's border wall: its bottom edge on the ground, along x or z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub from: [f64; 3],
    pub to: [f64; 3],
}

/// The block columns a zone can cover: `[min x, min z, max x, max z]`, max excluded.
pub fn column_bounds(z: &Zone) -> [i32; 4] {
    match z.shape {
        Shape::Points => bounds(&z.points),
        _ => [z.x - z.r, z.z - z.r, z.x + z.r + 1, z.z + z.r + 1],
    }
}

/// The zone's border as wall pieces: every side of a zone column that faces a column outside,
/// standing on `ground(x, z)` (the y a player would stand at; None where not known). Pieces on
/// one line at the same height are joined.
pub fn walls(z: &Zone, ground: &dyn Fn(i32, i32) -> Option<i32>) -> Vec<Wall> {
    let b = column_bounds(z);
    let mut out = Vec::new();
    // Sides facing north and south: lines along x.
    for bz in b[1]..b[3] {
        for (dz, line) in [(-1, bz), (1, bz + 1)] {
            let mut run: Option<(i32, i32)> = None;
            for bx in b[0]..=b[2] {
                let face = bx < b[2] && z.holds_column(bx, bz) && !z.holds_column(bx, bz + dz);
                let y = if face { ground(bx, bz) } else { None };
                match (run, y) {
                    (Some((_, ry)), Some(y)) if y == ry => {}
                    (Some((s, ry)), _) => {
                        out.push(Wall { from: [f64::from(s), f64::from(ry), f64::from(line)], to: [f64::from(bx), f64::from(ry), f64::from(line)] });
                        run = y.map(|y| (bx, y));
                    }
                    (None, Some(y)) => run = Some((bx, y)),
                    (None, None) => {}
                }
            }
        }
    }
    // Sides facing west and east: lines along z.
    for bx in b[0]..b[2] {
        for (dx, line) in [(-1, bx), (1, bx + 1)] {
            let mut run: Option<(i32, i32)> = None;
            for bz in b[1]..=b[3] {
                let face = bz < b[3] && z.holds_column(bx, bz) && !z.holds_column(bx + dx, bz);
                let y = if face { ground(bx, bz) } else { None };
                match (run, y) {
                    (Some((_, ry)), Some(y)) if y == ry => {}
                    (Some((s, ry)), _) => {
                        out.push(Wall { from: [f64::from(line), f64::from(ry), f64::from(s)], to: [f64::from(line), f64::from(ry), f64::from(bz)] });
                        run = y.map(|y| (bz, y));
                    }
                    (None, Some(y)) => run = Some((bz, y)),
                    (None, None) => {}
                }
            }
        }
    }
    out
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
            let shape_ok = zone.shape != Shape::Points || (3..=POINTS_MAX).contains(&zone.points.len());
            if shape_ok && !zone.world.is_empty() && zone.r >= 1 && zone.low <= zone.high && !out.iter().any(|z: &Zone| z.id == zone.id) {
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
                zone.shape = match kind {
                    "square" => Shape::Square,
                    "points" => Shape::Points,
                    _ => Shape::Circle,
                };
                zone.x = x.parse().unwrap_or(0);
                zone.z = z.parse().unwrap_or(0);
                zone.r = r.parse().unwrap_or(0);
            }
        } else if let Some(pt) = line.strip_prefix("point ") {
            let f: Vec<i32> = pt.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            if let [x, z] = f[..] {
                zone.points.push([x, z]);
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
        if z.shape == Shape::Points {
            for p in &z.points {
                out.push_str(&format!("point {} {}\n", p[0], p[1]));
            }
        }
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
    /// Corners placed on the map (shape Points).
    pub points: Vec<[i32; 2]>,
    /// A right click on a corner: which one, and where the "Delete" / "Keep" pair appears.
    pub menu: Option<(usize, [f32; 2])>,
}

impl Editor {
    pub fn span(&self) -> i32 {
        if self.far { 128 } else { 64 }
    }
    pub fn area(&self) -> u32 {
        match self.shape {
            Shape::Points => polygon_area(&self.points),
            _ => area(self.shape, self.r),
        }
    }
    pub fn holds_column(&self, bx: i32, bz: i32) -> bool {
        match self.shape {
            Shape::Points => in_polygon(&self.points, bx, bz),
            _ => inside(self.shape, self.r, bx - self.x, bz - self.z),
        }
    }
    /// The zone can be saved: a polygon needs three corners.
    pub fn complete(&self) -> bool {
        self.shape != Shape::Points || self.points.len() >= 3
    }
    /// The map's north-west column.
    pub fn origin(&self) -> [i32; 2] {
        [self.x - self.span() / 2, self.z - self.span() / 2]
    }
    /// The block corner nearest to `at` (GUI pixels) on `map`, kept on the map.
    pub fn corner_at(&self, map: Rect, at: [f32; 2]) -> [i32; 2] {
        let span = self.span() as f32;
        let o = self.origin();
        let fx = ((at[0] - map[0]) / map[2] * span).round().clamp(0.0, span);
        let fz = ((at[1] - map[1]) / map[3] * span).round().clamp(0.0, span);
        [o[0] + fx as i32, o[1] + fz as i32]
    }

    /// Where corner `i` is drawn on `map`.
    pub fn corner_pos(&self, map: Rect, i: usize) -> [f32; 2] {
        let cell = map[2] / self.span() as f32;
        let o = self.origin();
        let p = self.points[i];
        [map[0] + (p[0] - o[0]) as f32 * cell, map[1] + (p[1] - o[1]) as f32 * cell]
    }

    /// The corner under the pointer (within a few pixels), the last placed first.
    pub fn point_at(&self, map: Rect, at: [f32; 2]) -> Option<usize> {
        (0..self.points.len()).rev().find(|i| {
            let p = self.corner_pos(map, *i);
            (p[0] - at[0]).abs() <= 4.0 && (p[1] - at[1]).abs() <= 4.0
        })
    }

    /// A left press on the map: grabs the corner under the pointer, or places a new one there
    /// (up to [`POINTS_MAX`]). Returns the corner to drag. Nothing for other shapes.
    pub fn press_map(&mut self, map: Rect, at: [f32; 2]) -> Option<usize> {
        if self.shape != Shape::Points || !contains(map, at) {
            return None;
        }
        if let Some(i) = self.point_at(map, at) {
            return Some(i);
        }
        if self.points.len() >= POINTS_MAX {
            return None;
        }
        let c = self.corner_at(map, at);
        self.points.push(c);
        Some(self.points.len() - 1)
    }

    /// Drags corner `i` to the block corner nearest the pointer.
    pub fn move_point(&mut self, i: usize, map: Rect, at: [f32; 2]) {
        let c = self.corner_at(map, at);
        if let Some(p) = self.points.get_mut(i) {
            *p = c;
        }
    }

    /// A right press on a corner opens its "Delete" / "Keep" pair under the pointer.
    pub fn open_menu(&mut self, map: Rect, at: [f32; 2]) {
        self.menu = if self.shape == Shape::Points { self.point_at(map, at).map(|i| (i, at)) } else { None };
    }
}

/// Measured text widths.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measure {
    pub new_zone: f32,
    pub circle: f32,
    pub square: f32,
    pub points: f32,
    pub clear: f32,
    /// The hint above "Clear points" takes two lines.
    pub hint_two_lines: bool,
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
    let pitch = CARD_H + CARD_GAP;
    let over = (count as f32 * pitch - CARD_GAP - area[3]).max(0.0);
    (over / pitch).ceil() * pitch
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
    pub points: Rect,
    /// "Clear points", where the radius slider is for the other shapes.
    pub clear: Rect,
    /// Section labels: "Shape" and "What the zone takes", and the lines above them.
    pub shape_label_at: [f32; 2],
    pub summary_label_at: [f32; 2],
    pub rules: [f32; 2],
    /// The summary's third row: who is in the zone now.
    pub now_at: [f32; 2],
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
    // The map grows with the window, up to 384 points: twice that in pixels at GUI scale 2,
    // which is the map image's size, so blocks stay sharp.
    let side = h.min(w * 0.62).min(384.0).floor();
    let map = [x, y, side, side];
    let fx = x + side + 12.0;
    let fw = (x + w - fx).max(80.0);
    let seg = |at: f32, top: f32, a: f32, b: f32| -> (Rect, Rect) {
        let r1 = [at, top, a + 16.0, 16.0];
        let r2 = [at + a + 16.0, top, b + 16.0, 16.0];
        (r1, r2)
    };
    let seg3 = |at: f32, top: f32, a: f32, b: f32, c: f32| -> (Rect, Rect, Rect) {
        let (r1, r2) = seg(at, top, a, b);
        (r1, r2, [r2[0] + r2[2], top, c + 16.0, 16.0])
    };
    // Three sections on the right: the name, the shape, and what the zone takes. The map's own
    // controls (its scale) sit on the map.
    let mut cy = y + 2.0;
    let title_at = [fx, cy];
    cy += 20.0;
    let name_label_at = [fx, cy];
    cy += 12.0;
    let name = [fx, cy, fw.min(220.0), 16.0];
    cy += 28.0;
    let rule1 = cy;
    cy += 10.0;
    let shape_label_at = [fx, cy];
    cy += 12.0;
    let (circle, square, points) = seg3(fx, cy, m.circle, m.square, m.points);
    cy += 26.0;
    let radius_label_at = [fx, cy];
    let clear = [fx, cy - 2.0, m.clear + 16.0, 16.0];
    cy += 12.0;
    let slider = [fx, cy, fw.min(220.0), 12.0];
    cy += 24.0;
    let rule2 = cy;
    cy += 10.0;
    let summary_label_at = [fx, cy];
    cy += 15.0;
    let area_at = [fx, cy];
    let height_at = [fx, cy + 14.0];
    let now_at = [fx, cy + 28.0];
    let total = m.near + m.far + 32.0;
    let near = [x + side - 6.0 - total, y + 6.0, m.near + 16.0, 16.0];
    let far = [near[0] + near[2], y + 6.0, m.far + 16.0, 16.0];
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
        points,
        clear,
        shape_label_at,
        summary_label_at,
        rules: [rule1, rule2],
        now_at,
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
/// The "Delete" and "Keep" buttons under the pointer at `at`, kept inside `bounds`.
pub fn menu_rects(at: [f32; 2], delete_w: f32, keep_w: f32, bounds: Rect) -> (Rect, Rect) {
    let (dw, kw) = (delete_w + 12.0, keep_w + 12.0);
    let w = dw + 3.0 + kw;
    let x = (at[0] + 4.0).min(bounds[0] + bounds[2] - w).max(bounds[0]);
    let y = (at[1] + 6.0).min(bounds[1] + bounds[3] - 14.0).max(bounds[1]);
    ([x, y, dw, 14.0], [x + dw + 3.0, y, kw, 14.0])
}

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
    Points,
    Clear,
    Map,
    Near,
    Far,
    Slider,
    Save,
    Cancel,
    Nothing,
}

/// `points`: the editor draws a polygon, so the slider's place holds "Clear points".
pub fn editor_hit(lay: &EditorLayout, at: [f32; 2], points: bool) -> EditorHit {
    let slider = [lay.slider[0] - 2.0, lay.slider[1] - 3.0, lay.slider[2] + 4.0, lay.slider[3] + 6.0];
    let none = [0.0; 4];
    let (slider, clear) = if points { (none, lay.clear) } else { (slider, none) };
    [
        // The scale switch lies on the map, so it is asked first.
        (lay.near, EditorHit::Near),
        (lay.far, EditorHit::Far),
        (lay.map, EditorHit::Map),
        (lay.name, EditorHit::Name),
        (lay.circle, EditorHit::Circle),
        (lay.square, EditorHit::Square),
        (lay.points, EditorHit::Points),
        (clear, EditorHit::Clear),
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

/// The row of an open zone, under the main tabs: back, the zone's name and shape, and
/// "Edit shape" and "Delete..." on the right, or the delete confirmation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeadLayout {
    pub back: Rect,
    pub name_at: [f32; 2],
    /// Room for the name and, after it, the shape or the delete question.
    pub name_room: f32,
    pub edit: Option<Rect>,
    pub delete: Option<Rect>,
    pub yes: Option<Rect>,
    pub no: Option<Rect>,
}

/// `row` is the zone's row (18 high); widths: back, edit, delete, yes, no.
pub fn head_layout(row: Rect, back_w: f32, buttons: [f32; 4], confirm: bool) -> HeadLayout {
    let [x, y, w, h] = row;
    let by = y + (h - 16.0) / 2.0;
    let back = [x, by, back_w + 16.0, 16.0];
    let name_at = [back[0] + back[2] + 12.0, y + (h - 7.0) / 2.0];
    let right = x + w;
    let [edit_w, del_w, yes_w, no_w] = buttons.map(|b| b + 16.0);
    let mut out = HeadLayout { back, name_at, ..HeadLayout::default() };
    let left_of_buttons = if confirm {
        let no = [right - no_w, by, no_w, 16.0];
        let yes = [no[0] - 5.0 - yes_w, by, yes_w, 16.0];
        out.yes = Some(yes);
        out.no = Some(no);
        yes[0]
    } else {
        let delete = [right - del_w, by, del_w, 16.0];
        let edit = [delete[0] - 5.0 - edit_w, by, edit_w, 16.0];
        out.edit = Some(edit);
        out.delete = Some(delete);
        edit[0]
    };
    out.name_room = (left_of_buttons - 12.0 - name_at[0]).max(0.0);
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
        let c = Zone { id: 5, points: vec![[0, 0], [5, 0], [5, 3]], ..zone(Shape::Points, 3, 1, 3) };
        let zones = vec![a, b, c];
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
    fn border_walls_follow_the_edge_and_the_ground() {
        let flat = |_: i32, _: i32| Some(64);
        let z = zone(Shape::Square, 0, 0, 1);
        let mut w = walls(&z, &flat);
        w.sort_by(|a, b| a.from.partial_cmp(&b.from).unwrap());
        assert_eq!(w.len(), 4, "one joined piece a side");
        assert!(w.contains(&Wall { from: [-1.0, 64.0, -1.0], to: [2.0, 64.0, -1.0] }));
        assert!(w.contains(&Wall { from: [2.0, 64.0, -1.0], to: [2.0, 64.0, 2.0] }));
        // A step in the ground splits a side.
        let step = |x: i32, _: i32| Some(if x >= 1 { 65 } else { 64 });
        assert_eq!(walls(&z, &step).len(), 6);
        // Unknown ground: no wall there.
        assert!(walls(&z, &|_, _| None).is_empty());
        // A circle's border is closed: every piece has the same length on each axis in total.
        let c = zone(Shape::Circle, 0, 0, 5);
        let total = |w: &[Wall], along_x: bool| -> f64 {
            w.iter().filter(|p| (p.from[2] == p.to[2]) == along_x).map(|p| (p.to[0] - p.from[0]).abs() + (p.to[2] - p.from[2]).abs()).sum()
        };
        let cw = walls(&c, &flat);
        assert_eq!(total(&cw, true), total(&cw, false));
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
        let m = Measure { circle: 30.0, square: 40.0, points: 30.0, clear: 50.0, near: 30.0, far: 30.0, save: 50.0, cancel: 40.0, new_zone: 0.0, hint_two_lines: false };
        let lay = editor_layout([0.0, 0.0, 600.0, 270.0], &m);
        assert_eq!(lay.map[2], 270.0);
        assert!(lay.save[0] + lay.save[2] < lay.cancel[0]);
        assert_eq!(editor_hit(&lay, [lay.square[0] + 3.0, lay.square[1] + 3.0], false), EditorHit::Square);
        assert_eq!(editor_hit(&lay, [lay.points[0] + 3.0, lay.points[1] + 3.0], false), EditorHit::Points);
        assert_eq!(editor_hit(&lay, [lay.clear[0] + 3.0, lay.clear[1] + 8.0], true), EditorHit::Clear);
        assert_eq!(editor_hit(&lay, [lay.map[0] + 10.0, lay.map[1] + 10.0], true), EditorHit::Map);
    }

    #[test]
    fn a_polygon_holds_the_blocks_whose_middles_are_inside() {
        // Corners of an 8 by 8 block square from (-4,-4) to (4,4): columns -4..=3.
        let pts = vec![[-4, -4], [4, -4], [4, 4], [-4, 4]];
        assert_eq!(polygon_area(&pts), 64);
        assert!(in_polygon(&pts, -4, -4) && in_polygon(&pts, 3, 3));
        assert!(!in_polygon(&pts, 4, 0) && !in_polygon(&pts, 0, -5));
        let tri = vec![[0, 0], [6, 0], [0, 6]];
        assert!(in_polygon(&tri, 0, 0) && !in_polygon(&tri, 5, 5));
        assert_eq!(polygon_centre(&pts), (0, 0, 4));
        let z = Zone { points: pts, ..zone(Shape::Points, 0, 0, 4) };
        assert_eq!(z.area(), 64);
        assert!(z.holds([3.9, 64.0, -3.9]) && !z.holds([4.1, 64.0, 0.0]));
    }

    #[test]
    fn map_presses_place_and_grab_corners() {
        let mut e = Editor { id: None, name: String::new(), typing: false, shape: Shape::Points, x: 100, z: 50, r: 8, low: 0, high: 10, far: false, points: Vec::new(), menu: None };
        let map = [0.0, 0.0, 256.0, 256.0];
        // 4 px a block when near: the middle of the map is the corner (100, 50).
        assert_eq!(e.press_map(map, [128.0, 128.0]), Some(0));
        assert_eq!(e.points, vec![[100, 50]]);
        // A press on a corner grabs it instead of placing another.
        assert_eq!(e.press_map(map, [129.0, 127.0]), Some(0));
        assert_eq!(e.points.len(), 1);
        e.move_point(0, map, [137.0, 120.0]);
        assert_eq!(e.points, vec![[102, 48]]);
        // Dragged past the edge, it stays on the map.
        e.move_point(0, map, [-50.0, 300.0]);
        assert_eq!(e.points, vec![[68, 82]]);
        assert!(!e.complete());
        // Right press on the corner: the menu for it, where the pointer is.
        let at = e.corner_pos(map, 0);
        e.open_menu(map, at);
        assert_eq!(e.menu, Some((0, at)));
        e.open_menu(map, [200.0, 200.0]);
        assert_eq!(e.menu, None);
        for i in 0..12 {
            e.press_map(map, [10.0 + i as f32 * 8.0, 20.0]);
        }
        assert_eq!(e.points.len(), POINTS_MAX);
        assert!(e.complete());
        e.shape = Shape::Circle;
        assert_eq!(e.press_map(map, [200.0, 200.0]), None);
        assert_eq!(e.points.len(), POINTS_MAX);
        let (d, k) = menu_rects([250.0, 250.0], 30.0, 30.0, map);
        assert!(d[0] + d[2] < k[0] && k[0] + k[2] <= 256.0 && d[1] + d[3] <= 256.0);
    }
}
