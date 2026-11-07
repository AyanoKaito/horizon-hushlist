// Hushlist core: per-sound volume, per-group volume, cooldowns and the recent-sound log.
// Shared by the standalone library (unit tests) and the module injected next to Horizon's
// client sound hook (`horizon_client_lib::sound_playback`).
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime};

pub const MOD_ID: &str = "hushlist";
pub const MOD_VERSION: &str = "0.1.0";
pub const SETTINGS_FILE: &str = "settings.txt";
/// Sounds stay in the recent list this long after they last played.
pub const RECENT_WINDOW: Duration = Duration::from_secs(300);
/// At most this many distinct sounds are kept in the recent list.
pub const RECENT_MAX: usize = 200;
/// Longest cooldown accepted from settings.
pub const COOLDOWN_MAX_SECS: u32 = 600;
/// How often the settings file is checked for changes.
const RELOAD_INTERVAL: Duration = Duration::from_secs(1);
/// Families whose sounds form one group regardless of the second ID segment.
const WHOLE_GROUPS: [&str; 6] = ["music", "music_disc", "weather", "ui", "event", "ambient"];

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
            let (Some(kind), Some(key), Some(value), None) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(number) = value.parse::<u32>() else {
                continue;
            };
            let key = normalize(key).to_owned();
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
        let mut groups: Vec<_> = self.groups.iter().collect();
        groups.sort();
        lines.extend(groups.into_iter().map(|(k, v)| format!("group {k} {v}")));
        let mut sounds: Vec<_> = self.sounds.iter().collect();
        sounds.sort();
        lines.extend(sounds.into_iter().map(|(k, v)| format!("sound {k} {v}")));
        let mut cooldowns: Vec<_> = self.cooldowns.iter().collect();
        cooldowns.sort();
        lines.extend(cooldowns.into_iter().map(|(k, v)| format!("cooldown {k} {v}")));
        lines.join("\n") + "\n"
    }

    /// Effective volume in percent: the sound's own value, else its group's, else 100.
    pub fn volume_percent(&self, id: &str) -> u8 {
        let id = normalize(id);
        if let Some(v) = self.sounds.get(id) {
            return *v;
        }
        group_key(id)
            .and_then(|g| self.groups.get(&g).copied())
            .unwrap_or(100)
    }

    /// Effective volume as a multiplier of the game's volume.
    pub fn volume_factor(&self, id: &str) -> f32 {
        f32::from(self.volume_percent(id)) / 100.0
    }

    pub fn cooldown(&self, id: &str) -> Option<Duration> {
        self.cooldowns
            .get(normalize(id))
            .map(|s| Duration::from_secs(u64::from(*s)))
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
    pub last: Instant,
    pub count: u32,
}

/// Sounds the player actually heard recently, one entry per sound event.
#[derive(Clone, Debug, Default)]
pub struct Recent {
    entries: Vec<Heard>,
}

impl Recent {
    pub fn record(&mut self, id: &str, source: usize, position: [f64; 3], now: Instant) {
        self.prune(now);
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.source = source;
            entry.position = position;
            entry.last = now;
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
        }
        self.entries.push(Heard {
            id: id.to_owned(),
            source,
            position,
            last: now,
            count: 1,
        });
    }

    /// Drops sounds not heard within [`RECENT_WINDOW`].
    pub fn prune(&mut self, now: Instant) {
        self.entries
            .retain(|e| now.saturating_duration_since(e.last) <= RECENT_WINDOW);
    }

    /// Most recently heard first.
    pub fn list(&self) -> Vec<&Heard> {
        let mut list: Vec<_> = self.entries.iter().collect();
        list.sort_by(|a, b| b.last.cmp(&a.last).then_with(|| a.id.cmp(&b.id)));
        list
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

/// The mod's live state.
#[derive(Debug, Default)]
pub struct Engine {
    pub settings: Settings,
    pub recent: Recent,
    last_start: HashMap<String, Instant>,
}

impl Engine {
    /// Decides one playback operation. `starting` is true for starts and loop
    /// replays, false for live updates of a sound that is already playing.
    pub fn decide(
        &mut self,
        id: &str,
        source: usize,
        position: [f64; 3],
        starting: bool,
        now: Instant,
    ) -> Decision {
        let id = normalize(id);
        if starting {
            if let Some(cooldown) = self.settings.cooldown(id)
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
        }
        Decision {
            factor: self.settings.volume_factor(id),
            cancel: false,
        }
    }

    /// Replaces the settings. Cooldown timers of sounds keep running.
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }
}

// ---------------------------------------------------------------------------
// Runtime glue: process-wide state, settings file, live refresh, test trace.
// Only the game calls these; unit tests use `Engine` directly.

/// Process-wide state behind the game's sound hook (game thread only).
struct Runtime {
    engine: Engine,
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    next_check: Option<Instant>,
    /// Pending live refresh: 1 = nudge category volumes, 2 = restore them.
    refresh: u8,
    saved_volumes: Option<[f32; 11]>,
    nudged_at: Option<Instant>,
    traced: HashMap<String, Instant>,
    trace: Option<std::fs::File>,
    started: Option<Instant>,
}

fn runtime() -> MutexGuard<'static, Runtime> {
    static RUNTIME: OnceLock<Mutex<Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Mutex::new(Runtime {
                engine: Engine::default(),
                path: None,
                modified: None,
                next_check: None,
                refresh: 0,
                saved_volumes: None,
                nudged_at: None,
                traced: HashMap::new(),
                trace: None,
                started: None,
            })
        })
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// `config/hushlist/settings.txt` in the game folder (the working directory).
pub fn settings_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("config")
        .join(MOD_ID)
        .join(SETTINGS_FILE)
}

impl Runtime {
    fn ensure_started(&mut self, now: Instant) {
        if self.started.is_some() {
            return;
        }
        self.started = Some(now);
        self.path = Some(settings_path());
        if let Some(path) = std::env::var_os("HUSHLIST_TRACE") {
            self.trace = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok();
        }
        self.reload(now);
    }

    fn reload(&mut self, now: Instant) {
        self.next_check = Some(now + RELOAD_INTERVAL);
        let Some(path) = self.path.clone() else {
            return;
        };
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if modified == self.modified {
            return;
        }
        self.modified = modified;
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let settings = Settings::parse(&text);
        if settings != self.engine.settings {
            self.engine.set_settings(settings);
            self.refresh = 1;
            self.trace_line(now, &format!("\"event\":\"reload\",\"entries\":{}", self.entry_count()));
        }
    }

    fn entry_count(&self) -> usize {
        let s = &self.engine.settings;
        s.sounds.len() + s.groups.len() + s.cooldowns.len()
    }

    fn trace_line(&mut self, now: Instant, body: &str) {
        let Some(file) = self.trace.as_mut() else {
            return;
        };
        let ms = self
            .started
            .map_or(0, |s| now.saturating_duration_since(s).as_millis());
        use std::io::Write;
        let _ = writeln!(file, "{{\"ms\":{ms},{body}}}");
    }
}

/// Called by the game's sound hook for every start and live update.
pub fn on_play(
    identifier: &str,
    source: usize,
    position: [f64; 3],
    starting: bool,
    volume: &mut f32,
    cancel: &mut bool,
) {
    if *cancel {
        return;
    }
    let now = Instant::now();
    let mut rt = runtime();
    rt.ensure_started(now);
    if rt.next_check.is_none_or(|t| now >= t) {
        rt.reload(now);
    }
    let decision = rt.engine.decide(identifier, source, position, starting, now);
    if decision.cancel {
        *cancel = true;
    } else if decision.factor != 1.0 {
        *volume *= decision.factor;
    }
    if starting || decision.factor != 1.0 {
        let id = normalize(identifier).to_owned();
        let body = format!(
            "\"id\":\"{id}\",\"source\":{source},\"pos\":[{:.1},{:.1},{:.1}],\"starting\":{starting},\"factor\":{},\"cancel\":{},\"volume\":{}",
            position[0], position[1], position[2], decision.factor, decision.cancel, *volume
        );
        // Live updates of an edited sound repeat every tick: trace them once a second.
        let due = starting
            || rt
                .traced
                .get(&id)
                .is_none_or(|t| now.saturating_duration_since(*t) >= Duration::from_secs(1));
        if due {
            rt.traced.insert(id, now);
            rt.trace_line(now, &body);
        }
    }
}

/// Time the nudged category volumes stay, so the audio tick sees them.
pub const NUDGE_HOLD: Duration = Duration::from_millis(150);

/// Live refresh after a settings change. Horizon re-applies the sound hook to
/// every playing sound of a category whose option volume changed, so the
/// category volumes are moved by an inaudible amount for at least
/// [`NUDGE_HOLD`] and then restored. `read` and `write` are the game's option
/// volume accessors. Call it outside [`on_play`]'s lock.
pub fn refresh_step(now: Instant, read: impl Fn(usize) -> f32, write: impl Fn([f32; 11])) {
    let mut rt = runtime();
    match rt.refresh {
        1 => {
            let mut volumes = [0.0f32; 11];
            for (i, v) in volumes.iter_mut().enumerate() {
                *v = read(i);
            }
            rt.saved_volumes = Some(volumes);
            let mut nudged = volumes;
            for v in nudged.iter_mut().skip(1) {
                *v = if *v > 0.5 { *v - 1.0e-4 } else { *v + 1.0e-4 };
            }
            write(nudged);
            rt.nudged_at = Some(now);
            rt.refresh = 2;
        }
        2 if rt.nudged_at.is_some_and(|t| now.saturating_duration_since(t) >= NUDGE_HOLD) => {
            if let Some(volumes) = rt.saved_volumes.take() {
                write(volumes);
            }
            rt.refresh = 0;
        }
        _ => {}
    }
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
        let s = Settings::parse(
            "group entity.chicken 30\nsound minecraft:entity.chicken.egg 50\n",
        );
        assert_eq!(s.volume_percent("entity.chicken.egg"), 50);
        assert_eq!(s.volume_percent("minecraft:entity.chicken.ambient"), 30);
        assert_eq!(s.volume_percent("entity.cow.ambient"), 100);
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
    fn cooldown_skips_starts_inside_the_pause_only() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        e.set_settings(Settings::parse("cooldown entity.villager.ambient 10\n"));
        let id = "minecraft:entity.villager.ambient";
        assert!(!e.decide(id, 6, [0.0; 3], true, t0).cancel);
        assert!(e.decide(id, 6, [0.0; 3], true, at(t0, 4)).cancel);
        assert!(e.decide(id, 6, [0.0; 3], true, at(t0, 9)).cancel);
        assert!(!e.decide(id, 6, [0.0; 3], true, at(t0, 10)).cancel);
        // The pause restarts from the last start that actually played.
        assert!(e.decide(id, 6, [0.0; 3], true, at(t0, 15)).cancel);
        assert!(!e.decide(id, 6, [0.0; 3], true, at(t0, 20)).cancel);
        // Live updates of a playing sound are never cancelled.
        assert!(!e.decide(id, 6, [0.0; 3], false, at(t0, 21)).cancel);
        // Other sounds are not affected.
        assert!(!e.decide("entity.villager.trade", 6, [0.0; 3], true, at(t0, 21)).cancel);
    }

    #[test]
    fn skipped_starts_are_not_heard() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        e.set_settings(Settings::parse("cooldown a.b.c 10\n"));
        e.decide("a.b.c", 0, [0.0; 3], true, t0);
        e.decide("a.b.c", 0, [0.0; 3], true, at(t0, 1));
        assert_eq!(e.recent.list()[0].count, 1);
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
    }

    #[test]
    fn volume_decision_follows_settings_changes() {
        let t0 = Instant::now();
        let mut e = Engine::default();
        let id = "block.portal.ambient";
        assert_eq!(e.decide(id, 4, [0.0; 3], true, t0).factor, 1.0);
        e.set_settings(Settings::parse("group block.portal 20\n"));
        assert!((e.decide(id, 4, [0.0; 3], false, at(t0, 1)).factor - 0.2).abs() < 1e-6);
        e.set_settings(Settings::parse("group block.portal 20\nsound block.portal.ambient 0\n"));
        assert_eq!(e.decide(id, 4, [0.0; 3], false, at(t0, 2)).factor, 0.0);
    }

    #[test]
    fn refresh_nudges_then_restores_category_volumes() {
        use std::cell::RefCell;
        let stored = RefCell::new([1.0f32, 1.0, 0.25, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
        let original = *stored.borrow();
        let t0 = Instant::now();
        runtime().refresh = 1;
        let step = |t: Instant| refresh_step(t, |i| stored.borrow()[i], |v| *stored.borrow_mut() = v);
        step(t0);
        let nudged = *stored.borrow();
        assert_eq!(nudged[0], original[0]);
        for i in 1..11 {
            assert_ne!(nudged[i], original[i]);
            assert!((nudged[i] - original[i]).abs() < 1e-3);
            assert!((0.0..=1.0).contains(&nudged[i]));
        }
        // Held long enough for the next audio tick to notice.
        step(t0 + Duration::from_millis(50));
        assert_eq!(*stored.borrow(), nudged);
        step(t0 + NUDGE_HOLD);
        assert_eq!(*stored.borrow(), original);
        step(t0 + NUDGE_HOLD * 2);
        assert_eq!(*stored.borrow(), original);
    }
}
