# Sound in Horizon

What we know about how Horizon plays sounds, and where Hushlist can hook in.

Findings are based on Horizon commit [`c5bf899`](https://github.com/horizon-rust/horizon/tree/c5bf8999ae5cf015b05a2af13248d6bb75b6fb7e). This is the revision that was studied, not a chosen release target. Paths and names may change in later Horizon versions.

## Where sound lives

The client's sound code is in `crates/horizon-client-render/src/`:

| File | What it covers |
| --- | --- |
| `sound_engine.rs` | Playing sounds and tracking active sound instances. |
| `audio.rs` | Higher-level audio state, including the list of known sound event names and subtitle events. |
| `sound_events.rs` | Sound event definitions. |

Relevant pieces:

- **`SoundInstance`** describes one playing sound: its ID, category, volume, position and whether it repeats.
- **`SoundEngine::play`** starts a sound instance.
- **`SoundInstance::get_volume`** returns the instance's volume.
- **`Audio::event_names`** lists known sound event names. It is a candidate source for the "All" view.
- Active instances keep receiving volume updates while they play.

## What this means for Hushlist

- **No named per-sound hook.** Horizon's native mod API has no hook for intercepting or adjusting individual sounds. Hushlist will inject into game functions with `horizon-mixins`. A request for an official hook is open as [horizon-rust/horizon#10444](https://github.com/horizon-rust/horizon/issues/10444).
- **Changing volume only at start is not enough.** Long sounds, such as ambience and loops, get volume updates while playing. A change must apply for the whole life of a sound, and must be visible immediately when the player moves a slider.
- **`get_volume` has more than one use.** Besides loudness, its result also feeds the calculation of how far a sound can be heard. Adjusting it directly may change hearing range as a side effect. This must be checked before choosing it as the injection point.
- **Subtitle events are not a full sound log.** The subtitle feed skips sounds without subtitles and does not expose sound IDs. The live list needs its own record of played sounds, most likely captured where sounds start.
- **Starting a sound is not the same as intercepting it.** The native API can play a local sound (useful for preview), but that does not give control over other sounds.

## Interface building blocks

The native mod hooks in `crates/horizon-client/src/game/native_hooks.rs` provide:

- a mod-owned screen: open, close, and render after the screen;
- mouse, scroll, keyboard, character input and drag events;
- drawing: filled rectangles, quads, sprites and text;
- access to stored settings.

Known limits of a mod-owned screen at this revision:

- It opens over the world and does not pause a single-player game. For Hushlist this is useful: sounds keep playing while the list is open.
- It does not open over the main menu through the normal path.
- Text uses the game's font atlas. Other fonts have not been tested.
- Full gamepad support is not provided automatically.

## Open questions

1. Where readable sound names come from: subtitle keys and language files are the likely source. What to show for sounds without subtitles.
2. Whether music and continuous rain pass through `SoundEngine::play` or another path.
3. Whether a change reaches sounds that are already playing.
4. Which injection point changes loudness without changing hearing range.
5. Whether the mod works on a LAN guest's client and when joining a server.
6. How long to keep the recent-sounds record and how to bound its memory.
