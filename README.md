# Hushlist

*Hear it. Find it. Hush it.*

Hushlist is a native mod for [Horizon](https://playhorizon.dev), a reimplementation of Minecraft Java Edition in Rust. It lets you turn down individual game sounds instead of whole sound categories: quiet the villagers next to your base or the portal hum in your storage room, and keep everything else as it was.

## Status

**In development. There is nothing to install yet.**

The project started on 6 November 2026. Version 0.3.0 runs in a local build of the game, in English and Russian. Press H to open a screen with the sounds you heard recently, every sound of the game, and the ones you changed: each sound and each group has a volume slider, mute, reset and an optional pause between plays. Search, category filters and a flat or grouped view help find a sound. Settings live in profiles you can switch, and zones give a place in the world its own volumes. Multiplayer, servers and performance are not checked yet, and it is not packaged for installation; see the [research log](docs/research/log.md). Current work:

- player scenarios and the scope of the first version: [docs/concept.md](docs/concept.md);
- decisions made so far: [docs/decisions.md](docs/decisions.md);
- how sound works in Horizon and where the mod can hook in: [docs/research/horizon-audio.md](docs/research/horizon-audio.md);
- dated research log: [docs/research/log.md](docs/research/log.md).

## Features

- A live list of the sounds you are hearing right now and heard recently, so you can find an annoying sound without knowing its technical ID.
- Every sound of the game, and a list of the ones you changed.
- A volume from 0 to 100% for each sound and each group, plus mute, reset and a pause between plays.
- Search, category filters and a flat or grouped view.
- Profiles: named sets of settings, one active at a time.
- Zones: a place in the world (a circle, a square or a shape you draw on a map) whose sounds get their own volumes, with an optional see-through border in the world.
- Settings saved between sessions; the text follows the game's language.

Details are discussed in [docs/concept.md](docs/concept.md).

## Approach

Hushlist is a native Horizon trait mod: a Rust package that the Horizon launcher links into the game, using Horizon's client hooks for sound playback and mod screens. It is an original project, designed and written from scratch. It does not contain code, assets or interface designs from other mods.

The project is developed with AI assistance under human direction.

## License

[MIT](LICENSE).
