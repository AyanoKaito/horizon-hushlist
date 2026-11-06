# Hushlist

*Hear it. Find it. Hush it.*

Hushlist is a native mod for [Horizon](https://playhorizon.dev), a reimplementation of Minecraft Java Edition in Rust. It lets you turn down individual game sounds instead of whole sound categories: quiet the villagers next to your base or the portal hum in your storage room, and keep everything else as it was.

## Status

**Early research and design. There is nothing to install yet.**

The project started on 6 November 2026. Current work:

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

Hushlist is a native Horizon mod built with [`horizon-mixins`](https://github.com/horizon-rust/horizon-mixins). It is an original project, designed and written from scratch. It does not contain code, assets or interface designs from other mods.

The project is developed with AI assistance under human direction.

## License

[MIT](LICENSE).
