# Research log

Dated record of what was checked and what it proved. Each entry separates what was confirmed from what was not.

## 2026-11-08 — Zones, part 2 (points)

**Pin:** Horizon Nightly `6560641ba51a452f6bc2407baa4eb1b3be517ca8`.

**Change:** the zone editor has a third shape, Points. Each click on the map places a corner on the block grid (a click on a corner removes it), up to 10; "Clear points" removes them all. A block is in the zone when its middle is inside the polygon. The zone can be saved from 3 corners.

**Confirmed** (isolated offscreen runs, English and Russian, standard test meadow): four corners clicked around the player made an 8 by 8 block zone (64 blocks); two corners alone could not be saved; a second click on a corner removed it; the zone file kept the four corners; chickens inside the zone played at volume 0 after muting, chickens 11 to 13 blocks away at full volume. The circle checks still pass.

**Not confirmed:** self-crossing polygons (they follow the even-odd rule); corners outside the map's view after switching the scale.

## 2026-11-08 — Zones, part 1 (circle and square)

**Pin:** Horizon Nightly `6560641ba51a452f6bc2407baa4eb1b3be517ca8`.

**Change:** the Zones tab lists the zones of the world played now and makes new ones. The editor shows a map of the terrain around the player (the game's map colours, 64 or 128 blocks across) with the zone painted block by block, a circle or a square, a radius from 2 to 64, and a name. Inside a zone, the list has "Recent here", All and Changed; settings there apply only to sounds that start in the zone. Edit shape and Delete (with a question) are in the zone's header.

**Findings:** the client can read loaded blocks around the player and each block's map colour, which is enough for the map. The world played now is known by its save folder or server address. The recent list keeps only where each sound last played, so the zone's "Recent here" needs a list of its own: the same chicken sound played inside and outside the zone in turn.

**Confirmed** (isolated offscreen runs, English and Russian): chickens without AI stood 2 to 3 blocks from the player and 11 to 13 blocks away; a radius-6 circle zone was made around the player; muting the chicken group inside the zone wrote only to the zone file; over 25 seconds every inside chicken call played at volume 0 and every outside call at full volume. The map loaded at both scales; renaming, edit and cancel, and delete with its question worked. The screen (21), profiles (19) and Mods page (11) checks still pass (Russian).

**Not confirmed:** frame time with the editor open; overlapping zones in game (unit tests only); zones on servers; the map for zones far from the player (unloaded terrain shows dark).

## 2026-11-08 — Profiles

**Pin:** Horizon Nightly `6560641ba51a452f6bc2407baa4eb1b3be517ca8`.

**Change:** the header has a "Profile: ..." button that opens the profiles page: a list of profiles with the number of changed settings and an "Active" tag, and New profile, Use, Rename, Duplicate and Delete... buttons. A new profile is named right away (Enter saves, Escape cancels). A double click on a row uses that profile. Escape on the page goes back to the list.

**Confirmed** (isolated offscreen runs, English and Russian):
- an existing `settings.txt` became the first profile, with its setting;
- New and typing named a profile, and the screen stayed open while typing;
- Duplicate placed a copy under the selected profile with its settings file;
- a double click switched profiles and saved the active one;
- after switching, muting the weather group wrote only to the active profile's file;
- the active profile could not be deleted; deleting another asked first and removed its file;
- renaming saved the new name;
- the earlier screen checks (21) and Mods page checks still pass with the profile files.

**Not confirmed:** Mods page reset with more than one profile (the check ran with one profile); many profiles (the page shows as many rows as fit, without scrolling).

## 2026-11-08 — Mods page

**Pin:** Horizon Nightly `6560641ba51a452f6bc2407baa4eb1b3be517ca8`.

**Change:** Hushlist's page in Options > Mods has a key row for the open key ("Hotkey to open") and a "Reset all sound settings..." button. The mod's name, description and key caption follow the game's language.

**Findings:** a page button cannot open the mod's screen directly: the screen opens only while the world is played with no menu over it, and a mod cannot close the options menu. Page buttons have no disabled state. An "Open Hushlist" button that opened the screen after the player returned to the game was tried and dropped as confusing. Reset swaps the buttons for "Yes, reset all sound settings" and "Cancel".

**Confirmed** (isolated offscreen runs, English and Russian, title screen and a world):
- the page has no open button, with or without a world;
- Cancel keeps the settings file; Yes empties it;
- rebinding the key from H to J: H no longer opens the screen, J does.

**Not confirmed:** whether the mod's key also appears in the game's Controls settings.

## 2026-11-08 — Player screen 0.2.0

**Pin:** Horizon Nightly `6560641ba51a452f6bc2407baa4eb1b3be517ca8`.

**Change:** Hushlist is now a trait mod: a Rust package the launcher links into the game, using Horizon's mod hooks instead of source injections. Injected modules are private to the crate they land in, so they could not host a player screen. The sound logic is unchanged.

**Screen:** opened with H. The Recent tab lists sounds heard in the last 5 minutes, grouped by sound family, with a name from the game's subtitles, the sound ID, time since the last play, play count, a speaker button (mute), a volume slider, the percentage and a reset button. Clicking a row plays the sound at the player. Groups of long-playing sounds (weather, music, discs, ambience) start collapsed. A Freeze button stops the list from reordering; while frozen, a bar counts new sounds and shows them when clicked. Icons are the mod's own, drawn into the mod image slot; panels have rounded corners and rows animate to their new places.

**Confirmed:**
- 26 standalone unit tests passed (settings, groups, recent list, list building and ordering, layout and hit testing).
- Native build of the game with the mod linked the way the launcher links trait mods.
- Isolated offscreen run at 1280x720, GUI scale 2, with rain, mobs and a music disc:
  - H opened the screen and the close button closed it;
  - the speaker button muted rain and wrote `sound weather.rain 0` to the settings file;
  - dragging a slider to 30 % wrote that value;
  - with the list frozen, three new mobs produced a "5 new sounds" bar, and clicking it showed them;
  - screenshots showed the icons, real sound names, own and inherited volume shades, and the frozen state.
- The screen keeps groups collapsed while a long sound plays and does not count live volume refreshes as new plays.

**Not confirmed:**
- The All, Changed and Zones tabs, search and cooldown controls exist in the design only.
- The reset button was clicked in the run but on a different row than intended; reset was not verified.
- Frame time with the screen open was not measured.
- LAN, dedicated servers, other GUI scales and window sizes, gamepad input.
- The Mods list entry.

## 2026-11-07 — Technical prototype 0.1.0

**Pin:** Horizon Nightly `6560641ba51a452f6bc2407baa4eb1b3be517ca8` (2026-11-07).

**Finding:** Horizon now has a client sound hook, `horizon_client_lib::sound_playback::native_sound_play` (added 2026-11-06, not yet assigned a hooks API number). For every sound start and live update it passes the sound event ID, sound category, position and whether the sound is starting, and lets a mod change the volume or cancel the operation. Hushlist injects into it with `horizon-mixins` instead of patching the sound engine.

**Bundle:** one module (settings, effective volume, cooldowns, recent list) and two injections (sound hook, Mods list entry). Settings come from `config/hushlist/settings.txt` and are reloaded while the game runs. No player interface yet.

**Confirmed:**
- 10 standalone unit tests passed (groups, volume resolution, parsing, cooldowns, recent list, live refresh).
- `horizon-mixins apply` (tool `4342a0bf`): 2 injections, 1 module.
- Native Dev build of the transformed source: passed (25 min, cold cache).
- Isolated single-player run with audio enabled and master volume 1 %, trace from the mod:
  - without settings, 18 villager ambient starts played at the normal volume;
  - with that sound set to 0 %, all 14 starts were silenced, and the settings file was reloaded without restarting the game;
  - with a 10 s cooldown, 4 starts played and 11 were skipped; gaps between played starts were 10.4–12.9 s;
  - a music disc started at the normal volume; after its group was set to 20 %, the playing disc was updated to 20 % without restarting.
- No errors in the game log.

**Not confirmed:**
- The Mods list entry was not checked in the run.
- LAN, dedicated servers, and sounds other than those above.
- Whether lowering volume changes hearing range in practice. From the source: the range uses `max(volume, 1)`, so it changes only for sounds whose base volume is above 1.
- The hook has no API number yet; its signature may change when it gets one.

## 2026-11-06 — Project start

- Chose the name Hushlist and created this repository.
- Checked the Horizon mod catalog: no existing mod offers per-sound volume control.
- Found [horizon-rust/horizon#10444](https://github.com/horizon-rust/horizon/issues/10444), opened the same day, asking for a native hook to intercept and attenuate played sounds. Hushlist does not depend on it, but could use it if it lands.
- Recorded first decisions in [decisions.md](../decisions.md) and draft scenarios in [concept.md](../concept.md).

## 2026-11-02 — Injection feasibility check

**Goal:** a quick, cheap check that `horizon-mixins` can find and transform the sound functions Hushlist needs.

**Setup:** a minimal Cargo project containing Horizon's `sound_engine.rs` from commit `c5bf8999ae5cf015b05a2af13248d6bb75b6fb7e`, and a test bundle with two injections:

1. `HEAD` of `SoundEngine::play` — read the started sound instance by its ID.
2. `RETURN` of `SoundInstance::get_volume` — multiply the volume of `minecraft:entity.villager.ambient` by `0.5` as a test.

**Result:** `horizon-mixins apply` finished with `status: ok` and 2 injections.

**Confirmed:**
- Both injection targets can be found in the real source file.
- The tool transforms that file without errors.

**Not confirmed:**
- Transforming the full Horizon workspace.
- Compiling the transformed code.
- Running the game, hearing the change, long-playing sounds, or changing volume while a sound plays.
- Side effects of changing `get_volume` on how far sounds can be heard.

The `get_volume` change was a test only, not the chosen design.
