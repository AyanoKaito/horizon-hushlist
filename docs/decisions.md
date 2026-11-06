# Decisions

Decisions are listed with the date they were made. Current documents are rewritten in place; Git history keeps earlier versions.

- **2026-11-06 — Name.** The project is called Hushlist. The repository is `horizon-hushlist`; the internal mod ID is `hushlist` (Horizon package IDs allow lowercase letters, digits and underscores only).
- **2026-11-06 — Native mod.** Hushlist is a native Horizon mod built with `horizon-mixins`. Where Horizon has no named hook, the mod injects into game functions directly. Each injection must be checked against every Horizon revision the mod supports. If Horizon adds a named sound playback hook, switching to it will be evaluated.
- **2026-11-06 — Quieter only.** Per-sound volume ranges from 0 to 100% of the game's normal level, plus mute and reset. Making sounds louder than normal is out of scope.
- **2026-11-06 — Live list first.** The main way to find a sound is a live list of sounds playing now and recently. A search field sits at the top of the list; it filters the list, and filtered rows still highlight when their sound plays.
- **2026-11-06 — Local settings.** Settings apply only to the player's own client. Other players and servers do not need the mod.
- **2026-11-06 — Original work.** Hushlist is designed and written from scratch. It does not use code, assets or interface designs from other mods. The project's code and documentation use the MIT License.
- **2026-11-06 — Validation.** Support is claimed only for environments that pass actual in-game checks. Compilation alone is not treated as proof that a feature works.
