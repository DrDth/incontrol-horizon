# InControl for Horizon

A port of [InControl](https://github.com/McJtyMods/InControl) by McJty to
[Horizon](https://github.com/horizon-rust/horizon), the Rust rewrite of Minecraft: Java Edition 26.3. Built as
a wasm mod from the original's 1.21 sources (10.3.1), the version All the Mods 10 ships.

InControl decides spawns by rules in `config/incontrol/spawn.json`. Each rule names conditions and a `result`
(`allow`, `deny`, `default`); the first rule that matches decides, unless it says `continue`. A rule is asked at
one moment of a mob's life, set with `when`:

- `position` (the default): the spawn check, before the mob is placed. `deny` stops the spawn, `allow` lets it
  spawn where vanilla's own rules wouldn't.
- `finalize`: before the mob gets its equipment and attributes. `deny` cancels the spawn.
- `onjoin`: any living entity joining a level, `/summon` and loaded chunks included. `deny` keeps it out.
- `despawn`: every tick a mob may despawn. `allow` despawns it now, `deny` keeps it.

This part carries these conditions:

- What and where: `mob` (ids and `#tags`), `mod`, `dimension`, `dimensionmod`, `biome`, `biometags`, `height`,
  `minheight`, `maxheight`, `slime`.
- When and how: `time`, `mintime`, `maxtime`, `weather`, `difficulty`, `mindifficulty`, `maxdifficulty`, `random`,
  `spawner` and `spawntype` (for `position` and `finalize`), `baby` (for `onjoin` and `despawn`).
- Light: `light`, `minlight`, `maxlight`, `minlight_full`, `maxlight_full`, `minlight_sky`, `maxlight_sky`.
- Distance: `mindist` and `maxdist` to the nearest player, `minspawndist` and `maxspawndist` to the world spawn.
- Counts: `mincount` and `maxcount`, by the mob's own type, by `mob`, by `mod` or `all`, scaled `perplayer` or
  `perchunk`. Every level's mobs are counted again every `cacheRetentionTicks` and followed as they join and leave
  in between, so a cap holds exactly, also when a spawner places several mobs at once.
- `scoreboardtags_all`, `scoreboardtags_any`.

`/incontrol` (or `/ctrl`) has `reload`, `list` (the mobs of your level by type), `kill <type>` (`all` or a type id),
`showstats` and `clearstats` (how often each rule fired), and `showmobs`.

The settings `cacheRetentionTicks` and `perPlayerRadius` live in `config/incontrol-server.toml`.

## Not yet ported

A rule that uses one of these is skipped, and the error says which keywords it lacks. A `deny` rule keeps working
with actions in it, since it never runs them.

- `hostile` and `passive`, also in `mincount`/`maxcount` and `/incontrol kill`: they ask whether a mob is an
  `Enemy` or an `Animal`, which Horizon's mod API doesn't answer yet.
- `seesky` and `cave` (`canSeeSkyFromBelowWater`), `baby` for `position` and `finalize`, `canspawnhere` and
  `notcolliding`: likewise.
- `block`, `blocktest`, `blockoffset`, `structure`, `structuretags`, `hasstructure`, the player's items (`helmet`,
  `playerhelditem` and the rest), `area`, `phase`, `number`, `daycount`, `incontrol` and `eventspawn`.
- Every action: changing health, speed, damage, armor, equipment, potions, names or NBT, `message`, `command`,
  phases and numbers.
- The other rule files: `loot.json`, `experience.json`, `effects.json`, `breakevents.json`, `placeevents.json`,
  `rightclicks.json`, `leftclicks.json`, `phases.json`, `spawner.json`, `areas.json`, `events.json`.
- `/incontrol debug`, `info`, `days`, `phases`, `numbers`, `area`, `setphase`, `clearphase`, `setnumber`.

## Differences from the original

- The rule files are JSON with comments (`//`, `/* */`, `#`). Gson's other lenient forms (unquoted names, single
  quotes) aren't read.
- `showstats` and `showmobs` answer in chat; the original writes them to the server log, which a wasm mod has
  none of.
- `random` draws from the mod's own generator, not Java's.
- Conditions on mods Horizon doesn't have (Serene Seasons, Game Stages, The Lost Cities, Baubles, EnigmaScript)
  add no check, as the original does when those mods are missing. `kubejs` reports that KubeJS isn't loaded and
  never matches, and `npc` never matches.
- All the Mods 10 keeps a `spawn.json` in `defaultconfigs/incontrol/`. Neither the original nor this port reads
  that folder, so it takes effect only once copied to `config/incontrol/`.

## Building

The mod needs Horizon's mod API from a checkout of [horizon-rust/horizon](https://github.com/horizon-rust/horizon).
Build it inside one:

```sh
mkdir -p <horizon>/.local/dev-mods/incontrol
cp -R Cargo.toml horizon-mod.toml src LICENSE <horizon>/.local/dev-mods/incontrol/
cd <horizon>/.local/dev-mods/incontrol
cargo run --manifest-path ../../../Cargo.toml -p cargo-horizon-mod -- horizon-mod build --wasm
```

## License

MIT, as the original, whose license text [LICENSE](LICENSE) keeps. The Horizon port by DrDth.
