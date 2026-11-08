# Hushlist

*Hear it. Find it. Hush it.*

Hushlist is a native mod for [Horizon](https://playhorizon.dev), a reimplementation of Minecraft Java Edition in Rust. It lets you turn down individual game sounds instead of whole sound categories: quiet the villagers next to your base or the portal hum in your storage room, and keep everything else as it was.

## Status

**Early development. There is nothing to install yet.**

The project started on 6 November 2026. Version 0.2.0 runs in a local build of the game: press H to open a screen that lists the sounds you heard recently, with a volume slider, mute and reset for each sound and each group, in English and Russian. It is not packaged for installation yet; see the [research log](docs/research/log.md). Current work:

- player scenarios and the scope of the first version: [docs/concept.md](docs/concept.md);
- decisions made so far: [docs/decisions.md](docs/decisions.md);
- how sound works in Horizon and where the mod can hook in: [docs/research/horizon-audio.md](docs/research/horizon-audio.md);
- dated research log: [docs/research/log.md](docs/research/log.md).

## Planned idea

- A live list of the sounds you are hearing right now and heard recently, so you can find an annoying sound without knowing its technical ID.
- A search field that filters the list while sounds keep highlighting as they play.
- A volume slider from 0 to 100% for each sound, plus mute and reset.
- Settings saved between sessions.

Details are not final and are discussed in [docs/concept.md](docs/concept.md).

## Approach

Hushlist is a native Horizon trait mod: a Rust package that the Horizon launcher links into the game, using Horizon's client hooks for sound playback and mod screens. It is an original project, designed and written from scratch. It does not contain code, assets or interface designs from other mods.

The project is developed with AI assistance under human direction.

## License

[MIT](LICENSE).
