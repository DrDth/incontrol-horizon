//! `CountInfo` and `RuleCache`: how many mobs a level has, by type and by mod, counted again
//! every `cacheRetentionTicks` and kept up to date between as mobs join and leave.
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::json::{self, Error};

/// `CountInfo`: what `mincount` and `maxcount` count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CountInfo {
    pub amount: i32,
    /// Entity type ids; none counts the type of the mob the rule looks at.
    pub types: Vec<String>,
    pub per_player: bool,
    pub per_chunk: bool,
    pub passive: bool,
    pub hostile: bool,
    pub all: bool,
    pub module: Option<String>,
}

impl CountInfo {
    /// `CountInfo.validate`.
    fn validate(&self) -> Option<&'static str> {
        if self.per_player && self.per_chunk {
            return Some("You cannot combine 'perchunk' and 'perplayer'!");
        }
        if self.module.is_some() && !self.types.is_empty() {
            return Some("You cannot combine 'mod' with 'mob'!");
        }
        if (self.passive && self.hostile)
            || (self.all && self.passive)
            || (self.all && self.hostile)
        {
            return Some("Don't use all, passive, and hostile at the same time!");
        }
        if (self.passive || self.hostile || self.all) && !self.types.is_empty() {
            return Some("You cannot combine 'all', 'passive', or 'hostile' with 'mob'!");
        }
        None
    }

    /// `CountInfo.parseCountInfo(json)`: Ok(None) where the original reports an error and makes
    /// no check, Err where it throws. `known` says whether an entity type id is registered; the
    /// original's registry gives the pig for one that isn't.
    pub fn parse(
        text: &str,
        known: &dyn Fn(&str) -> bool,
        errors: &mut Vec<String>,
    ) -> Result<Option<CountInfo>, Error> {
        // `BuiltInRegistries.ENTITY_TYPE.get` is a defaulted registry: an unknown id is the pig,
        // never null, so the original's error after it never comes.
        let find = |id: &str| -> String {
            let id = json::id(id);
            if known(&id) {
                id
            } else {
                "minecraft:pig".to_owned()
            }
        };
        match json::parse_value(text)? {
            Value::String(s) => {
                let split: Vec<&str> = s.split(',').filter(|p| !p.is_empty()).collect();
                let Some(first) = split.first() else {
                    return Err("Index 0 out of bounds for length 0".to_owned());
                };
                let Ok(amount) = first.parse::<i32>() else {
                    errors.push(format!("Bad amount for mincount '{first}'!"));
                    return Ok(None);
                };
                let mut info = CountInfo {
                    amount,
                    ..Default::default()
                };
                if let Some(mob) = split.get(1) {
                    info.types.push(find(mob));
                }
                Ok(Some(info))
            }
            Value::Object(o) => {
                let amount = json::as_int(o.get("amount").ok_or("No amount")?)?;
                let mut info = CountInfo {
                    amount,
                    ..Default::default()
                };
                if let Some(mob) = o.get("mob") {
                    match mob {
                        Value::Array(list) => {
                            for el in list {
                                info.types.push(find(&json::as_string(el)?));
                            }
                        }
                        Value::Object(_) | Value::Null => {
                            errors.push("Bad entity tag in count description!".to_owned());
                            return Ok(None);
                        }
                        v => info.types.push(find(&json::as_string(v)?)),
                    }
                }
                if let Some(m) = o.get("mod") {
                    info.module = Some(json::as_string(m)?);
                }
                for (key, flag) in [
                    ("perplayer", &mut info.per_player),
                    ("perchunk", &mut info.per_chunk),
                    ("passive", &mut info.passive),
                    ("all", &mut info.all),
                    ("hostile", &mut info.hostile),
                ] {
                    if let Some(v) = o.get(key) {
                        *flag = json::as_bool(v)?;
                    }
                }
                if let Some(error) = info.validate() {
                    errors.push(error.to_owned());
                    return Ok(None);
                }
                Ok(Some(info))
            }
            Value::Array(_) | Value::Null => {
                errors.push(format!("Count description '{text}' is not valid!"));
                Ok(None)
            }
            v => Ok(Some(CountInfo {
                amount: json::as_int(&v)?,
                ..Default::default()
            })),
        }
    }

    /// `getCounter`: the count this info compares, in `level`, for a mob of type `kind`.
    pub fn count(&self, cache: &RuleCache, dimension: &str, kind: &str) -> i32 {
        let level = cache.level(dimension);
        let Some(level) = level else {
            // `getOrCreateCache` on a level not counted yet: empty counters, -1 totals.
            return if self.all { -3 } else { 0 };
        };
        if let Some(module) = &self.module {
            // `hostile` and `passive` are refused when the rule is read.
            return level.per_mod.get(module).copied().unwrap_or(0);
        }
        if self.all {
            return level.all;
        }
        if self.types.is_empty() {
            return level.count(kind);
        }
        self.types.iter().map(|t| level.count(t)).sum()
    }

    /// `getAmountAdjuster`: the amount, scaled by the level's chunks or players.
    pub fn amount(&self, cache: &RuleCache, dimension: &str) -> i32 {
        let level = cache.level(dimension);
        if self.per_chunk {
            let chunks = level.map_or(-1, |l| l.valid_spawn_chunks);
            self.amount.wrapping_mul(chunks) / 289
        } else if self.per_player {
            let players = level.map_or(-1, |l| l.valid_players);
            self.amount.wrapping_mul(players)
        } else {
            self.amount
        }
    }
}

/// `RuleCache.CachePerWorld`.
#[derive(Clone, Debug)]
pub struct LevelCount {
    counters: HashMap<String, i32>,
    per_mod: HashMap<String, i32>,
    /// `countHostile + countPassive + countNeutral`, each -1 until the first count.
    all: i32,
    valid_spawn_chunks: i32,
    valid_players: i32,
    dirty_counter: i32,
}

impl Default for LevelCount {
    fn default() -> Self {
        LevelCount {
            counters: HashMap::new(),
            per_mod: HashMap::new(),
            all: -3,
            valid_spawn_chunks: -1,
            valid_players: -1,
            dirty_counter: 0,
        }
    }
}

impl LevelCount {
    fn count(&self, kind: &str) -> i32 {
        self.counters.get(kind).copied().unwrap_or(0)
    }

    /// `addCountedMob` for a mob of type `kind`.
    fn add(&mut self, kind: &str) {
        *self.counters.entry(kind.to_owned()).or_insert(0) += 1;
        *self.per_mod.entry(namespace(kind).to_owned()).or_insert(0) += 1;
        self.all += 1;
    }

    /// `removeCountedMob`: false when the counts are probably wrong.
    fn remove(&mut self, kind: &str) -> bool {
        let count = self.count(kind);
        if count <= 0 {
            return false;
        }
        self.counters.insert(kind.to_owned(), count - 1);
        if let Some(n) = self.per_mod.get_mut(namespace(kind)) {
            *n -= 1;
            if self.all <= 0 {
                return false;
            }
            self.all -= 1;
        }
        true
    }
}

/// The namespace of an id (`ResourceLocation.getNamespace`).
pub fn namespace(id: &str) -> &str {
    id.split_once(':').map_or("minecraft", |(ns, _)| ns)
}

/// What a level count reads from the game, gathered by the caller.
pub struct Census {
    /// The type of every mob in the level.
    pub mobs: Vec<String>,
    /// `chunkMap.size()`.
    pub chunks: i32,
    /// The players in the level who aren't spectators.
    pub players: i32,
}

/// `RuleCache`: one count per level, by dimension id.
#[derive(Debug, Default)]
pub struct RuleCache {
    levels: HashMap<String, LevelCount>,
    /// The entity types seen as mobs (`instanceof Mob`), so a type that leaves is known by its id.
    mob_kinds: HashSet<String>,
}

impl RuleCache {
    fn level(&self, dimension: &str) -> Option<&LevelCount> {
        self.levels.get(dimension)
    }

    /// `performCount`'s first step: whether the level's counts are due again.
    pub fn due(&mut self, dimension: &str) -> bool {
        let level = self.levels.entry(dimension.to_owned()).or_default();
        level.dirty_counter -= 1;
        level.dirty_counter <= 0
    }

    /// `performCount`'s count, with `retention` ticks until the next.
    pub fn recount(&mut self, dimension: &str, census: Census, retention: i32) {
        let level = self.levels.entry(dimension.to_owned()).or_default();
        level.dirty_counter = retention;
        level.valid_spawn_chunks = census.chunks;
        level.valid_players = census.players;
        level.counters.clear();
        level.per_mod.clear();
        level.all = 0;
        for kind in census.mobs {
            level.add(&kind);
            self.mob_kinds.insert(kind);
        }
    }

    /// `addMob`: a mob of type `kind` joined the level.
    pub fn join(&mut self, dimension: &str, kind: &str) {
        self.mob_kinds.insert(kind.to_owned());
        self.levels
            .entry(dimension.to_owned())
            .or_default()
            .add(kind);
    }

    /// `removeMob`: an entity of type `kind` left the level.
    pub fn leave(&mut self, dimension: &str, kind: &str) {
        if !self.mob_kinds.contains(kind) {
            return;
        }
        if let Some(level) = self.levels.get_mut(dimension)
            && !level.remove(kind)
        {
            // The cache is probably invalid, so it's counted again next tick.
            level.dirty_counter = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(id: &str) -> bool {
        id == "minecraft:zombie" || id == "minecraft:skeleton"
    }

    #[test]
    fn count_descriptions_read_as_the_original() {
        let mut errors = Vec::new();
        let plain = CountInfo::parse("15", &known, &mut errors)
            .unwrap()
            .unwrap();
        assert_eq!(plain.amount, 15);
        assert!(plain.types.is_empty());
        let object = CountInfo::parse(
            r#"{"amount": 4, "mob": ["minecraft:zombie", "creeper"], "perplayer": true}"#,
            &known,
            &mut errors,
        )
        .unwrap()
        .unwrap();
        assert_eq!(object.types, vec!["minecraft:zombie", "minecraft:pig"]);
        assert!(object.per_player);
        assert!(errors.is_empty());
        let bad = CountInfo::parse(
            r#"{"amount": 4, "mod": "x", "mob": "minecraft:zombie"}"#,
            &known,
            &mut errors,
        );
        assert_eq!(bad, Ok(None));
        assert_eq!(errors, vec!["You cannot combine 'mod' with 'mob'!"]);
        assert!(CountInfo::parse("15,minecraft:zombie", &known, &mut errors).is_err());
    }

    #[test]
    fn counts_follow_joins_and_leaves_until_the_next_count() {
        let mut cache = RuleCache::default();
        assert!(cache.due("minecraft:overworld"));
        cache.recount(
            "minecraft:overworld",
            Census {
                mobs: vec![
                    "minecraft:zombie".into(),
                    "minecraft:zombie".into(),
                    "a:b".into(),
                ],
                chunks: 578,
                players: 2,
            },
            10,
        );
        let zombies = CountInfo {
            amount: 3,
            types: vec!["minecraft:zombie".into()],
            ..Default::default()
        };
        assert_eq!(zombies.count(&cache, "minecraft:overworld", "x:y"), 2);
        cache.join("minecraft:overworld", "minecraft:zombie");
        assert_eq!(zombies.count(&cache, "minecraft:overworld", "x:y"), 3);
        cache.leave("minecraft:overworld", "minecraft:item");
        cache.leave("minecraft:overworld", "minecraft:zombie");
        assert_eq!(zombies.count(&cache, "minecraft:overworld", "x:y"), 2);
        let all = CountInfo {
            all: true,
            ..Default::default()
        };
        assert_eq!(all.count(&cache, "minecraft:overworld", "x:y"), 3);
        let module = CountInfo {
            module: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(module.count(&cache, "minecraft:overworld", "x:y"), 1);
        let per_chunk = CountInfo {
            amount: 10,
            per_chunk: true,
            ..Default::default()
        };
        assert_eq!(per_chunk.amount(&cache, "minecraft:overworld"), 20);
        for _ in 0..9 {
            assert!(!cache.due("minecraft:overworld"));
        }
        assert!(cache.due("minecraft:overworld"));
    }
}
