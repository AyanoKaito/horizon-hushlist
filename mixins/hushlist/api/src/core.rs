// Hushlist core: per-sound volume, per-group volume, cooldowns and the recent-sound log.
// No game types: the trait mod (lib.rs) feeds it, and the standalone crate tests it.
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const MOD_ID: &str = "hushlist";
pub const MOD_VERSION: &str = "0.2.0";
pub const SETTINGS_FILE: &str = "settings.txt";
/// Sounds stay in the recent list this long after they last played.
pub const RECENT_WINDOW: Duration = Duration::from_secs(300);
/// At most this many distinct sounds are kept in the recent list.
pub const RECENT_MAX: usize = 200;
/// Longest cooldown accepted from settings.
pub const COOLDOWN_MAX_SECS: u32 = 600;
/// A sound counts as still playing while it gets live updates this recently.
pub const ALIVE_WINDOW: Duration = Duration::from_millis(600);
/// A sound counts as long (rain, a portal, a disc) once it has kept playing this long;
/// mob sounds that follow their mob get updates too, but only for about a second.
pub const LONG_AFTER: Duration = Duration::from_millis(1500);
/// Families whose sounds form one group regardless of the second ID segment.
pub const WHOLE_GROUPS: [&str; 6] = ["music", "music_disc", "weather", "ui", "event", "ambient"];

/// The sound event ID without the `minecraft:` namespace.
pub fn normalize(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// The source group of a sound event: the first two ID segments
/// (`entity.chicken.egg` -> `entity.chicken`), or the whole family for music,
/// discs, weather, interface, events and ambience. Two-segment IDs outside
/// those families have no group.
pub fn group_key(id: &str) -> Option<String> {
    let id = normalize(id);
    let mut parts = id.split('.');
    let first = parts.next().filter(|s| !s.is_empty())?;
    if WHOLE_GROUPS.contains(&first) {
        return Some(first.to_owned());
    }
    let second = parts.next().filter(|s| !s.is_empty())?;
    parts.next()?;
    Some(format!("{first}.{second}"))
}

/// What the player set. Volumes are percentages of the game's own volume.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Per-sound volume set by hand. Wins over the group volume.
    pub sounds: HashMap<String, u8>,
    /// Per-group volume. Applies to every sound of the group without its own volume.
    pub groups: HashMap<String, u8>,
    /// Minimum pause after a sound starts before it may start again, in seconds.
    pub cooldowns: HashMap<String, u32>,
    /// When each setting was last changed, Unix seconds, by `"<kind> <key>"`
    /// (`"sound entity.cow.ambient"`). Optional fourth field of a line; sorts "Recently changed".
    pub stamps: HashMap<String, u64>,
    /// Settings under these ones: what a zone's untouched sounds follow. Display only, never saved.
    pub base: Option<Box<Settings>>,
}

impl Settings {
    /// Reads the line format written by [`Settings::to_text`]. Unknown or
    /// malformed lines are ignored so a hand-edited file never stops the game.
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let (Some(kind), Some(key), Some(value), stamp, None) =
                (fields.next(), fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let stamp = match stamp.map(str::parse::<u64>) {
                Some(Ok(s)) => Some(s),
                Some(Err(_)) => continue,
                None => None,
            };
            let Ok(number) = value.parse::<u32>() else {
                continue;
            };
            let key = normalize(key).to_owned();
            if let Some(s) = stamp
                && matches!(kind, "sound" | "group" | "cooldown")
                && (kind != "cooldown" || number > 0)
            {
                settings.stamps.insert(format!("{kind} {key}"), s);
            }
            match kind {
                "sound" => {
                    settings.sounds.insert(key, number.min(100) as u8);
                }
                "group" => {
                    settings.groups.insert(key, number.min(100) as u8);
                }
                "cooldown" if number > 0 => {
                    settings.cooldowns.insert(key, number.min(COOLDOWN_MAX_SECS));
                }
                _ => {}
            }
        }
        settings
    }

    /// Deterministic text form: one setting per line, sorted.
    pub fn to_text(&self) -> String {
        let mut lines = vec!["# Hushlist settings".to_owned()];
        let line = |kind: &str, k: &str, v: String| match self.stamps.get(&format!("{kind} {k}")) {
            Some(s) => format!("{kind} {k} {v} {s}"),
            None => format!("{kind} {k} {v}"),
        };
        let mut groups: Vec<_> = self.groups.iter().collect();
        groups.sort();
        lines.extend(groups.into_iter().map(|(k, v)| line("group", k, v.to_string())));
        let mut sounds: Vec<_> = self.sounds.iter().collect();
        sounds.sort();
        lines.extend(sounds.into_iter().map(|(k, v)| line("sound", k, v.to_string())));
        let mut cooldowns: Vec<_> = self.cooldowns.iter().collect();
        cooldowns.sort();
        lines.extend(cooldowns.into_iter().map(|(k, v)| line("cooldown", k, v.to_string())));
        lines.join("\n") + "\n"
    }

    /// Effective volume in percent: the sound's own value, else its group's, else the base's, else 100.
    pub fn volume_percent(&self, id: &str) -> u8 {
        self.own_percent(id)
            .unwrap_or_else(|| self.base.as_ref().map_or(100, |b| b.volume_percent(id)))
    }

    /// The volume set here for the sound or its group, if any.
    pub fn own_percent(&self, id: &str) -> Option<u8> {
        let id = normalize(id);
        if let Some(v) = self.sounds.get(id) {
            return Some(*v);
        }
        group_key(id).and_then(|g| self.groups.get(&g).copied())
    }

    /// Group volume in percent: set here, else the base's, else 100.
    pub fn group_percent(&self, group: &str) -> u8 {
        match self.groups.get(group) {
            Some(v) => *v,
            None => self.base.as_ref().map_or(100, |b| b.group_percent(group)),
        }
    }

    /// Cooldown seconds: set here, else the base's, else 0.
    pub fn cooldown_secs(&self, id: &str) -> u32 {
        match self.cooldowns.get(normalize(id)) {
            Some(s) => *s,
            None => self.base.as_ref().map_or(0, |b| b.cooldown_secs(id)),
        }
    }

    /// Effective volume as a multiplier of the game's volume.
    pub fn volume_factor(&self, id: &str) -> f32 {
        f32::from(self.volume_percent(id)) / 100.0
    }

    pub fn cooldown(&self, id: &str) -> Option<Duration> {
        match self.cooldown_secs(id) {
            0 => None,
            s => Some(Duration::from_secs(u64::from(s))),
        }
    }

    /// How many settings there are (the Changed tab's count).
    pub fn count(&self) -> usize {
        self.sounds.len() + self.groups.len() + self.cooldowns.len()
    }

    /// Latest change of anything set on this sound (volume or cooldown), Unix seconds.
    pub fn sound_stamp(&self, id: &str) -> u64 {
        let a = self.stamps.get(&format!("sound {id}")).copied().unwrap_or(0);
        let b = self.stamps.get(&format!("cooldown {id}")).copied().unwrap_or(0);
        a.max(b)
    }

    pub fn group_stamp(&self, group: &str) -> u64 {
        self.stamps.get(&format!("group {group}")).copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.sounds.is_empty() && self.groups.is_empty() && self.cooldowns.is_empty()
    }
}

/// One sound in the recent list.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub id: String,
    /// Vanilla SoundSource index of the last play.
    pub source: usize,
    /// World position of the last play.
    pub position: [f64; 3],
    /// Last start.
    pub last: Instant,
    /// Last start or live update: a sound updated within [`ALIVE_WINDOW`] is still playing.
    pub alive: Instant,
    pub count: u32,
}

impl Heard {
    pub fn playing(&self, now: Instant) -> bool {
        self.alive.saturating_duration_since(self.last) >= LONG_AFTER
            && now.saturating_duration_since(self.alive) <= ALIVE_WINDOW
    }
}

/// Sounds the player actually heard recently, one entry per sound event.
#[derive(Clone, Debug, Default)]
pub struct Recent {
    entries: Vec<Heard>,
    /// Position of each ID in `entries`.
    index: HashMap<String, usize>,
}

impl Recent {
    pub fn record(&mut self, id: &str, source: usize, position: [f64; 3], now: Instant) {
        if self.entries.first().is_some_and(|e| now.saturating_duration_since(e.alive) > RECENT_WINDOW)
            || self.entries.len() >= RECENT_MAX
        {
            self.prune(now);
        }
        if let Some(&i) = self.index.get(id) {
            let entry = &mut self.entries[i];
            entry.source = source;
            entry.position = position;
            entry.last = now;
            entry.alive = now;
            entry.count = entry.count.saturating_add(1);
            return;
        }
        if self.entries.len() >= RECENT_MAX
            && let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last)
                .map(|(i, _)| i)
        {
            self.entries.swap_remove(oldest);
            self.reindex();
        }
        self.index.insert(id.to_owned(), self.entries.len());
        self.entries.push(Heard {
            id: id.to_owned(),
            source,
            position,
            last: now,
            alive: now,
            count: 1,
        });
    }

    /// A live update of a sound already in the list.
    pub fn touch(&mut self, id: &str, position: [f64; 3], now: Instant) {
        if let Some(&i) = self.index.get(id) {
            let entry = &mut self.entries[i];
            entry.alive = now;
            entry.position = position;
        }
    }

    /// Drops sounds not heard within [`RECENT_WINDOW`].
    pub fn prune(&mut self, now: Instant) {
        let before = self.entries.len();
        self.entries
            .retain(|e| now.saturating_duration_since(e.alive) <= RECENT_WINDOW);
        if self.entries.len() != before {
            self.reindex();
        }
    }

    fn reindex(&mut self) {
        self.index = self.entries.iter().enumerate().map(|(i, e)| (e.id.clone(), i)).collect();
    }

    /// Most recently heard first.
    pub fn list(&self) -> Vec<&Heard> {
        let mut list: Vec<_> = self.entries.iter().collect();
        list.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| a.id.cmp(&b.id)));
        list
    }

    pub fn get(&self, id: &str) -> Option<&Heard> {
        self.index.get(id).map(|&i| &self.entries[i])
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// What to do with one playback operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    /// Multiplier for the sound's base volume.
    pub factor: f32,
    /// Skip this start (cooldown).
    pub cancel: bool,
}

/// The mod's sound state.
#[derive(Debug, Default)]
pub struct Engine {
    pub settings: Settings,
    pub recent: Recent,
    last_start: HashMap<String, Instant>,
    /// Zones of the active profile, every world.
    pub zones: Vec<crate::zones::Zone>,
    /// The world played now ([`crate::zones::Zone::world`]); zones of other worlds do nothing.
    pub world: Option<String>,
    /// Sounds heard from each zone (by zone id), for its "Recent here": the main list keeps only
    /// where a sound played last, and the same sound may play inside and outside a zone in turn.
    pub zone_recent: HashMap<u32, Recent>,
}

impl Engine {
    /// Decides one playback operation. `starting` is true for starts and loop
    /// replays, false for live updates of a sound that is already playing.
    pub fn decide(
        &mut self,
        id: &str,
        source: usize,
        position: [f64; 3],
        relative: bool,
        starting: bool,
        now: Instant,
    ) -> Decision {
        let id = normalize(id);
        // A sound placed relative to the listener (music, the interface) comes from no place.
        let zone = if relative { None } else { self.zone_at(position) };
        let (volume, cooldown) = match zone.map(|i| &self.zones[i].settings) {
            Some(z) => (
                z.own_percent(id).unwrap_or_else(|| self.settings.volume_percent(id)),
                z.cooldown(id).or_else(|| self.settings.cooldown(id)),
            ),
            None => (self.settings.volume_percent(id), self.settings.cooldown(id)),
        };
        // The recent list keeps where a sound came from, for "Recent here"; a relative one came from nowhere.
        let position = if relative { [f64::NAN; 3] } else { position };
        if starting {
            if let Some(cooldown) = cooldown
                && let Some(last) = self.last_start.get(id)
                && now.saturating_duration_since(*last) < cooldown
            {
                return Decision {
                    factor: 1.0,
                    cancel: true,
                };
            }
            self.last_start.insert(id.to_owned(), now);
            self.recent.record(id, source, position, now);
        } else {
            self.recent.touch(id, position, now);
        }
        if let Some(zid) = zone.map(|i| self.zones[i].id) {
            let here = self.zone_recent.entry(zid).or_default();
            if starting {
                here.record(id, source, position, now);
            } else {
                here.touch(id, position, now);
            }
        }
        Decision {
            factor: f32::from(volume) / 100.0,
            cancel: false,
        }
    }

    /// The zone a sound at `position` belongs to: the smallest one of this world that holds it.
    pub fn zone_at(&self, position: [f64; 3]) -> Option<usize> {
        let world = self.world.as_deref()?;
        crate::zones::smallest_at(&self.zones, world, position)
    }

    /// The volume factor a sound would get now, without recording it (live re-apply).
    pub fn factor(&self, id: &str, position: [f64; 3], relative: bool) -> f32 {
        let id = normalize(id);
        let zone = if relative { None } else { self.zone_at(position) };
        let v = match zone {
            Some(i) => self.zones[i].settings.own_percent(id).unwrap_or_else(|| self.settings.volume_percent(id)),
            None => self.settings.volume_percent(id),
        };
        f32::from(v) / 100.0
    }
}

/// Category volumes moved by an inaudible amount. Horizon re-applies the sound
/// hook to every playing sound of a category whose option volume changed, so
/// writing these for a moment and then the originals makes a settings change
/// reach sounds that are already playing. Index 0 (master) is left alone.
pub fn nudged(volumes: [f32; 11], phase: bool) -> [f32; 11] {
    let step = if phase { 2.0e-4 } else { 1.0e-4 };
    let mut out = volumes;
    for v in out.iter_mut().skip(1) {
        *v = if *v > 0.5 { *v - step } else { *v + step };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, secs: u64) -> Instant {
        start + Duration::from_secs(secs)
    }

    #[test]
    fn groups_use_two_segments_or_whole_family() {
        assert_eq!(group_key("minecraft:entity.chicken.egg").as_deref(), Some("entity.chicken"));
        assert_eq!(group_key("entity.baby_chicken.ambient").as_deref(), Some("entity.baby_chicken"));
        assert_eq!(group_key("music.overworld.forest").as_deref(), Some("music"));
        assert_eq!(group_key("music_disc.pigstep").as_deref(), Some("music_disc"));
        assert_eq!(group_key("weather.rain").as_deref(), Some("weather"));
        assert_eq!(group_key("ambient.cave").as_deref(), Some("ambient"));
        assert_eq!(group_key("entity.tnt.primed").as_deref(), Some("entity.tnt"));
        assert_eq!(group_key("item.nautilus_saddle_equip"), None);
        assert_eq!(group_key("intentionally_empty"), None);
        assert_eq!(group_key(""), None);
    }

    #[test]
    fn own_volume_wins_over_group_and_group_over_default() {
        let s = Settings::parse("group entity.chicken 30\nsound minecraft:entity.chicken.egg 50\n");
        assert_eq!(s.volume_percent("entity.chicken.egg"), 50);
        assert_eq!(s.volume_percent("minecraft:entity.chicken.ambient"), 30);
        assert_eq!(s.volume_percent("entity.cow.ambient"), 100);
        assert_eq!(s.group_percent("entity.chicken"), 30);
        assert_eq!(s.group_percent("entity.cow"), 100);
        assert!((s.volume_factor("entity.chicken.ambient") - 0.3).abs() < 1e-6);
    }

    #[test]
    fn parse_ignores_bad_lines_and_clamps_values() {
        let s = Settings::parse(
            "# comment\n\nsound a.b.c 250\ngroup x.y\nsound a.b.d abc\ncooldown a.b.c 0\ncooldown a.b.e 9999\nvolume a.b.c 5\n",
        );
        assert_eq!(s.sounds.get("a.b.c"), Some(&100));
        assert_eq!(s.sounds.len(), 1);
        assert!(s.groups.is_empty());
        assert_eq!(s.cooldowns.get("a.b.c"), None);
        assert_eq!(s.cooldowns.get("a.b.e"), Some(&COOLDOWN_MAX_SECS));
    }

    #[test]
    fn text_round_trip_is_stable() {
        let s = Settings::parse("sound b.c.d 5\ngroup a.b 20\ncooldown b.c.d 30\nsound a.b.c 0\n");
        let text = s.to_text();
        assert_eq!(Settings::parse(&text), s);
        assert_eq!(text, Settings::parse(&text).to_text());
        assert!(text.find("group a.b").unwrap() < text.find("sound a.b.c").unwrap());
    }

    #[test]
    fn change_times_are_optional_and_kept() {
        let s = Settings::parse("sound a.b.c 40 1700000000\ngroup a.b 20\ncooldown a.b.c 10 1700000500\nsound a.b.d 5 x\n");
        assert_eq!(s.sounds.get("a.b.d"), None);
        assert_eq!(s.sound_stamp("a.b.c"), 1700000500);
        assert_eq!(s.group_stamp("a.b"), 0);
        assert_eq!(s.count(), 3);
        assert_eq!(Settings::parse(&s.to_text()), s);
        assert!(s.to_text().contains("sound a.b.c 40 1700000000\n"));
    }

    #[test]
    fn cooldown_skips_starts_inside_the_pause_only() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        e.settings = Settings::parse("cooldown entity.villager.ambient 10\n");
        let id = "minecraft:entity.villager.ambient";
        assert!(!e.decide(id, 6, [0.0; 3], false, true, t0).cancel);
        assert!(e.decide(id, 6, [0.0; 3], false, true, at(t0, 4)).cancel);
        assert!(e.decide(id, 6, [0.0; 3], false, true, at(t0, 9)).cancel);
        assert!(!e.decide(id, 6, [0.0; 3], false, true, at(t0, 10)).cancel);
        // The pause restarts from the last start that actually played.
        assert!(e.decide(id, 6, [0.0; 3], false, true, at(t0, 15)).cancel);
        assert!(!e.decide(id, 6, [0.0; 3], false, true, at(t0, 20)).cancel);
        // Live updates of a playing sound are never cancelled.
        assert!(!e.decide(id, 6, [0.0; 3], false, false, at(t0, 21)).cancel);
        // Other sounds are not affected.
        assert!(!e.decide("entity.villager.trade", 6, [0.0; 3], false, true, at(t0, 21)).cancel);
    }

    #[test]
    fn skipped_starts_are_not_heard() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        e.settings = Settings::parse("cooldown a.b.c 10\n");
        e.decide("a.b.c", 0, [0.0; 3], false, true, t0);
        e.decide("a.b.c", 0, [0.0; 3], false, true, at(t0, 1));
        assert_eq!(e.recent.list()[0].count, 1);
    }

    #[test]
    fn live_updates_mark_a_sound_as_playing() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        e.decide("weather.rain", 3, [0.0; 3], false, true, t0);
        assert!(!e.recent.get("weather.rain").unwrap().playing(t0));
        // Updated for half a second: a mob sound following its mob, not a long sound.
        e.decide("weather.rain", 3, [0.5; 3], false, false, t0 + Duration::from_millis(500));
        assert!(!e.recent.get("weather.rain").unwrap().playing(t0 + Duration::from_millis(550)));
        e.decide("weather.rain", 3, [1.0; 3], false, false, t0 + Duration::from_millis(1600));
        let h = e.recent.get("weather.rain").unwrap();
        assert!(h.playing(t0 + Duration::from_millis(1700)));
        assert!(!h.playing(t0 + Duration::from_secs(3)));
        assert_eq!(h.position, [1.0; 3]);
        assert_eq!(h.count, 1);
    }

    #[test]
    fn recent_lists_latest_first_and_counts_repeats() {
        let t0 = Instant::now();
        let mut r = Recent::default();
        r.record("a.b.c", 1, [1.0, 2.0, 3.0], t0);
        r.record("d.e.f", 2, [0.0; 3], at(t0, 1));
        r.record("a.b.c", 1, [4.0, 5.0, 6.0], at(t0, 2));
        let list = r.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "a.b.c");
        assert_eq!(list[0].count, 2);
        assert_eq!(list[0].position, [4.0, 5.0, 6.0]);
        assert_eq!(list[1].id, "d.e.f");
    }

    #[test]
    fn recent_forgets_after_window_and_caps_size() {
        let t0 = Instant::now();
        let mut r = Recent::default();
        r.record("old.sound.x", 0, [0.0; 3], t0);
        r.record("new.sound.x", 0, [0.0; 3], t0 + RECENT_WINDOW + Duration::from_secs(1));
        assert_eq!(r.list().iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["new.sound.x"]);

        let mut r = Recent::default();
        for i in 0..(RECENT_MAX + 5) {
            r.record(&format!("s.n.{i}"), 0, [0.0; 3], t0 + Duration::from_millis(i as u64));
        }
        assert_eq!(r.len(), RECENT_MAX);
        assert!(r.list().iter().all(|h| h.id != "s.n.0"));
        assert_eq!(r.list()[0].id, format!("s.n.{}", RECENT_MAX + 4));
        // The index stays in step after evictions.
        for h in r.list() {
            assert_eq!(r.get(&h.id).unwrap().id, h.id);
        }
        assert!(r.get("s.n.0").is_none());
    }

    #[test]
    fn volume_decision_follows_settings_changes() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        let id = "block.portal.ambient";
        assert_eq!(e.decide(id, 4, [0.0; 3], false, true, t0).factor, 1.0);
        e.settings = Settings::parse("group block.portal 20\n");
        assert!((e.decide(id, 4, [0.0; 3], false, false, at(t0, 1)).factor - 0.2).abs() < 1e-6);
        e.settings = Settings::parse("group block.portal 20\nsound block.portal.ambient 0\n");
        assert_eq!(e.decide(id, 4, [0.0; 3], false, false, at(t0, 2)).factor, 0.0);
    }

    #[test]
    fn nudge_changes_every_category_but_master_by_an_inaudible_amount() {
        let original = [1.0f32, 1.0, 0.25, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        let n = nudged(original, false);
        let m = nudged(original, true);
        assert_eq!(n[0], original[0]);
        for i in 1..11 {
            assert_ne!(n[i], original[i]);
            // A second nudge differs from the first, so the game sees a change again.
            assert_ne!(m[i], n[i]);
            assert!((n[i] - original[i]).abs() < 1e-3 && (m[i] - original[i]).abs() < 1e-3);
            assert!((0.0..=1.0).contains(&n[i]) && (0.0..=1.0).contains(&m[i]));
        }
    }
}
