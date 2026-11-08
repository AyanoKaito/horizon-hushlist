# Decisions

Decisions are listed with the date they were made. Current documents are rewritten in place; Git history keeps earlier versions.

- **2026-11-06 — Name.** The project is called Hushlist. The repository is `horizon-hushlist`; the internal mod ID is `hushlist` (Horizon package IDs allow lowercase letters, digits and underscores only).
- **2026-11-07 — Native trait mod.** Hushlist is a native Horizon trait mod: a Rust package declared with `[api]` in its bundle and linked into the game by the launcher. It uses Horizon's client sound hook and mod screen hooks, so it needs no source injections. Earlier (2026-11-06) the plan was `horizon-mixins` injections; a mixin module cannot host a player screen, because injected modules are private to the crate they land in. Each release is pinned to one Horizon revision and checked against it.
- **2026-11-06 — Quieter only.** Per-sound volume ranges from 0 to 100% of the game's normal level, plus mute and reset. Making sounds louder than normal is out of scope.
- **2026-11-06 — Live list first.** The main way to find a sound is a live list of sounds playing now and recently. A search field sits at the top of the list; it filters the list, and filtered rows still highlight when their sound plays.
- **2026-11-06 — Local settings.** Settings apply only to the player's own client. Other players and servers do not need the mod.
- **2026-11-06 — Original work.** Hushlist is designed and written from scratch. It does not use code, assets or interface designs from other mods. The project's code and documentation use the MIT License.
- **2026-11-06 — Validation.** Support is claimed only for environments that pass actual in-game checks. Compilation alone is not treated as proof that a feature works.
- **2026-11-08 — Translations.** All of the mod's own text (screen labels, names of sound families and of groups the game has no name for) lives in `mixins/hushlist/api/data/lang/<code>.lang`, one `key=value` per line. The mod follows the game's language setting and switches with it; a missing key or language falls back to English. Sound names come from the game's own translations. Translation files are the only tracked files in a language other than English.
