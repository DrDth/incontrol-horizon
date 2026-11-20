//! `GenericRuleEvaluator`: a rule's conditions, each a check on the spawn, join or despawn it is
//! asked about. A rule matches when every check passes, in the original's order.
use mod_api::{EntityRef, Host, LevelRef, LightLayer, SpawnReason, UnspawnedMobRef};

use crate::attributes::{Attr, AttributeMap};
use crate::counts::{CountInfo, RuleCache, namespace};
use crate::expression::{self, Expression};
use crate::json;

/// The mob a rule looks at.
#[derive(Clone, Copy)]
pub enum Subject {
    /// The mob of a spawn check or finalization, in no level yet.
    Unspawned(UnspawnedMobRef),
    /// A living entity that joins a level or a mob that may despawn.
    Entity(EntityRef),
}

/// `IEventQuery`: what the checks read from the event.
pub struct Query<'a> {
    pub level: LevelRef,
    pub dimension: &'a str,
    /// `getPos`.
    pub pos: [i32; 3],
    pub kind: &'a str,
    pub subject: Subject,
    /// The spawn's reason, for spawn checks and finalization.
    pub reason: Option<SpawnReason>,
    /// Whether the event's world is a `Level`: false for chunk generation, which asks with its
    /// `WorldGenRegion`.
    pub in_level: bool,
}

/// What the checks read besides the event.
pub struct Context<'a> {
    pub host: &'a dyn Host,
    pub cache: &'a RuleCache,
    /// `Config.PERPLAYER_RADIUS`.
    pub radius: i32,
    pub random: &'a mut Random,
}

/// `java.util.Random`'s place in `addRandomCheck`: any fair draw.
#[derive(Debug)]
pub struct Random(u64);

impl Default for Random {
    fn default() -> Self {
        Random(0x2545_f491_4f6c_dd1d)
    }
}

impl Random {
    /// A float in `[0, 1)`.
    pub fn next_float(&mut self) -> f32 {
        // xorshift64*.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let x = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (x >> 40) as f32 / (1u32 << 24) as f32
    }
}

/// When a rule is asked, for the checks that read what only some events have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// A spawn check or finalization: the mob isn't in a level yet, and the event says why it
    /// spawns.
    Spawn,
    /// A join or despawn: the mob is in a level.
    InLevel,
}

/// One check.
#[derive(Clone, Debug, PartialEq)]
enum Check {
    Random(f32),
    Dimension(Vec<String>),
    DimensionMod(Vec<String>),
    Time(Expression),
    MinTime(i32),
    MaxTime(i32),
    Height(Expression),
    MinHeight(i32),
    MaxHeight(i32),
    Mods(Vec<String>),
    Never,
    Raining,
    Thundering,
    BiomeTags(Vec<String>),
    Difficulty(String),
    MinSpawnDist(f32),
    MaxSpawnDist(f32),
    MinDist(f32),
    MaxDist(f32),
    BlockLight(Expression),
    MinBlockLight(i32),
    MaxBlockLight(i32),
    MinFullLight(i32),
    MaxFullLight(i32),
    MinSkyLight(i32),
    MaxSkyLight(i32),
    MinDifficulty(f32),
    MaxDifficulty(f32),
    Slime(bool),
    Biomes(Vec<String>),
    AllScoreboardTags(Vec<String>),
    AnyScoreboardTags(Vec<String>),
    Baby(bool),
    Spawner(bool),
    SpawnType(Vec<SpawnReason>),
    Mobs { ids: Vec<String>, tags: Vec<String> },
    MinCount(CountInfo),
    MaxCount(CountInfo),
}

/// `MobSpawnType.valueOf`, by the 1.21.1 names the rule files use.
fn spawn_reason(name: &str) -> Option<SpawnReason> {
    Some(match name.to_uppercase().as_str() {
        "NATURAL" => SpawnReason::Natural,
        "CHUNK_GENERATION" => SpawnReason::ChunkGeneration,
        "SPAWNER" => SpawnReason::Spawner,
        "STRUCTURE" => SpawnReason::Structure,
        "BREEDING" => SpawnReason::Breeding,
        "MOB_SUMMONED" => SpawnReason::MobSummoned,
        "JOCKEY" => SpawnReason::Jockey,
        "EVENT" => SpawnReason::Event,
        "CONVERSION" => SpawnReason::Conversion,
        "REINFORCEMENT" => SpawnReason::Reinforcement,
        "TRIGGERED" => SpawnReason::Triggered,
        "BUCKET" => SpawnReason::Bucket,
        "SPAWN_EGG" => SpawnReason::SpawnItemUse,
        "COMMAND" => SpawnReason::Command,
        "DISPENSER" => SpawnReason::Dispenser,
        "PATROL" => SpawnReason::Patrol,
        "TRIAL_SPAWNER" => SpawnReason::TrialSpawner,
        _ => return None,
    })
}

/// What reading a rule's conditions needs from the game.
pub struct Registry<'a> {
    /// Whether an entity type id is registered.
    pub entity_type: &'a dyn Fn(&str) -> bool,
}

/// A rule's checks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evaluator {
    checks: Vec<Check>,
}

fn strings(list: Vec<Attr>) -> Vec<String> {
    list.into_iter().map(|a| a.str().to_owned()).collect()
}

impl Evaluator {
    /// `GenericRuleEvaluator(map)`: the checks of the conditions in `map`, consumed from it.
    /// Conditions this port can't check yet stay in `map`. Errors the original reports go to
    /// `errors`; Err is where it throws, which ends the file.
    pub fn new(
        map: &mut AttributeMap,
        asked: Asked,
        registry: &Registry<'_>,
        errors: &mut Vec<String>,
    ) -> Result<Evaluator, String> {
        let mut checks = Vec::new();
        let expression =
            |s: &str, errors: &mut Vec<String>| -> Result<Option<Expression>, String> {
                let e = expression::parse(s)?;
                if e.is_none() {
                    errors.push(format!("Unknown expression '{s}'!"));
                }
                Ok(e)
            };
        if let Some(r) = map.consume("random") {
            checks.push(Check::Random(r.float()));
        }
        if let Some(list) = map.consume_list("dimension") {
            checks.push(Check::Dimension(strings(list)));
        }
        if let Some(list) = map.consume_list("dimensionmod") {
            checks.push(Check::DimensionMod(strings(list)));
        }
        if let Some(t) = map.consume("time")
            && let Some(e) = expression(t.str(), errors)?
        {
            checks.push(Check::Time(e));
        }
        if let Some(t) = map.consume("mintime") {
            checks.push(Check::MinTime(t.int()));
        }
        if let Some(t) = map.consume("maxtime") {
            checks.push(Check::MaxTime(t.int()));
        }
        if let Some(h) = map.consume("height")
            && let Some(e) = expression(h.str(), errors)?
        {
            checks.push(Check::Height(e));
        }
        if let Some(h) = map.consume("minheight") {
            checks.push(Check::MinHeight(h.int()));
        }
        if let Some(h) = map.consume("maxheight") {
            checks.push(Check::MaxHeight(h.int()));
        }
        if let Some(list) = map.consume_list("mod") {
            checks.push(Check::Mods(strings(list)));
        }
        if map.consume_list("kubejs").is_some() {
            // `KubeJSSupport.parse` without KubeJS: an error, and a check that never passes.
            errors.push("KubeJS condition specified but KubeJS is not loaded!".to_owned());
            checks.push(Check::Never);
        }
        if let Some(w) = map.consume("weather") {
            let w = w.str().to_lowercase();
            if w.starts_with("rain") {
                checks.push(Check::Raining);
            } else if w.starts_with("thunder") {
                checks.push(Check::Thundering);
            } else {
                errors.push(format!("Unknown weather '{w}'! Use 'rain' or 'thunder'"));
            }
        }
        if let Some(list) = map.consume_list("biometags") {
            checks.push(Check::BiomeTags(
                strings(list).iter().map(|s| json::id(s)).collect(),
            ));
        }
        if let Some(d) = map.consume("difficulty") {
            let d = d.str().to_lowercase();
            if ["peaceful", "easy", "normal", "hard"].contains(&d.as_str()) {
                checks.push(Check::Difficulty(d));
            } else {
                errors.push(format!(
                    "Unknown difficulty '{d}'! Use one of 'easy', 'normal', 'hard',  or 'peaceful'"
                ));
            }
        }
        if let Some(v) = map.consume("minspawndist") {
            checks.push(Check::MinSpawnDist(v.float()));
        }
        if let Some(v) = map.consume("maxspawndist") {
            checks.push(Check::MaxSpawnDist(v.float()));
        }
        if let Some(v) = map.consume("mindist") {
            checks.push(Check::MinDist(v.float()));
        }
        if let Some(v) = map.consume("maxdist") {
            checks.push(Check::MaxDist(v.float()));
        }
        if let Some(l) = map.consume("light")
            && let Some(e) = expression(l.str(), errors)?
        {
            checks.push(Check::BlockLight(e));
        }
        for (key, check) in [
            ("minlight", Check::MinBlockLight as fn(i32) -> Check),
            ("maxlight", Check::MaxBlockLight),
            ("minlight_full", Check::MinFullLight),
            ("maxlight_full", Check::MaxFullLight),
            ("minlight_sky", Check::MinSkyLight),
            ("maxlight_sky", Check::MaxSkyLight),
        ] {
            if let Some(v) = map.consume(key) {
                checks.push(check(v.int()));
            }
        }
        if let Some(v) = map.consume("mindifficulty") {
            checks.push(Check::MinDifficulty(v.float()));
        }
        if let Some(v) = map.consume("maxdifficulty") {
            checks.push(Check::MaxDifficulty(v.float()));
        }
        if let Some(v) = map.consume("slime") {
            checks.push(Check::Slime(v.bool()));
        }
        if let Some(list) = map.consume_list("biome") {
            checks.push(Check::Biomes(strings(list)));
        }
        // Conditions on mods Horizon doesn't have (Serene Seasons, Game Stages, The Lost Cities,
        // Baubles, EnigmaScript): the original warns in its log and adds no check.
        for key in [
            "state",
            "summer",
            "winter",
            "spring",
            "autumn",
            "gamestage",
            "incity",
            "instreet",
            "insphere",
            "inbuilding",
            "inmultibuilding",
            "building",
            "multibuilding",
            "amulet",
            "ring",
            "belt",
            "trinket",
            "head",
            "body",
            "charm",
        ] {
            map.consume_list(key);
        }
        if let Some(list) = map.consume_list("scoreboardtags_all") {
            checks.push(Check::AllScoreboardTags(strings(list)));
        }
        if let Some(list) = map.consume_list("scoreboardtags_any") {
            checks.push(Check::AnyScoreboardTags(strings(list)));
        }
        map.consume("when");
        if asked == Asked::InLevel
            && let Some(b) = map.consume("baby")
        {
            checks.push(Check::Baby(b.bool()));
        }
        if asked == Asked::Spawn {
            if let Some(s) = map.consume("spawner") {
                checks.push(Check::Spawner(s.bool()));
            }
            if let Some(list) = map.consume_list("spawntype") {
                let mut reasons = Vec::new();
                for name in strings(list) {
                    // `MobSpawnType.valueOf` throws for a name it doesn't know.
                    reasons.push(spawn_reason(&name).ok_or_else(|| {
                        format!(
                            "No enum constant net.minecraft.world.entity.MobSpawnType.{}",
                            name.to_uppercase()
                        )
                    })?);
                }
                checks.push(Check::SpawnType(reasons));
            }
        }
        if let Some(list) = map.consume_list("mob") {
            let ids = strings(list);
            let mut types = Vec::new();
            let mut tags = Vec::new();
            for id in &ids {
                if let Some(tag) = id.strip_prefix('#') {
                    tags.push(json::id(tag));
                    continue;
                }
                let full = json::id(id);
                if (registry.entity_type)(&full) {
                    types.push(full);
                } else {
                    // The defaulted registry's pig; only a single mob is reported.
                    if ids.len() == 1 {
                        errors.push(format!("Unknown mob '{id}'!"));
                    }
                    types.push("minecraft:pig".to_owned());
                }
            }
            checks.push(Check::Mobs { ids: types, tags });
        }
        if map.consume_list("npc").is_some() {
            // Only Custom NPCs marks a mob as one of its NPCs.
            checks.push(Check::Never);
        }
        for (key, min) in [("mincount", true), ("maxcount", false)] {
            let Some(text) = map.consume(key) else {
                continue;
            };
            let known = |id: &str| (registry.entity_type)(id);
            let Some(info) = CountInfo::parse(text.str(), &known, errors)? else {
                continue;
            };
            if info.hostile || info.passive {
                // Telling hostile and passive mobs apart needs `instanceof Enemy` and `Animal`.
                map.unconsumed(key, text);
                continue;
            }
            checks.push(if min {
                Check::MinCount(info)
            } else {
                Check::MaxCount(info)
            });
        }
        Ok(Evaluator { checks })
    }

    /// `GenericRuleEvaluator.match`: whether every check passes.
    pub fn matches(&self, query: &Query<'_>, ctx: &mut Context<'_>) -> bool {
        self.checks.iter().all(|c| test(c, query, ctx))
    }
}

/// `TestingTools.isChunkInvalid`: the chunk at `pos` isn't loaded in full.
fn chunk_invalid(query: &Query<'_>, ctx: &Context<'_>, pos: [i32; 3]) -> bool {
    !query.in_level || !query.level.is_loaded(ctx.host, pos)
}

/// `getClosestPlayer`: the nearest player who isn't a spectator within the configured radius of
/// the event's position, as their block position.
fn closest_player(query: &Query<'_>, ctx: &Context<'_>) -> Option<[i32; 3]> {
    let host = ctx.host;
    let at = query.pos.map(f64::from);
    let range = f64::from(ctx.radius) * f64::from(ctx.radius);
    let mut best: Option<(f64, [i32; 3])> = None;
    for player in query.level.online_players(host).unwrap_or_default() {
        if player.is_spectator(host) == Some(true) {
            continue;
        }
        let entity = player.entity();
        let (Some(p), Some(block)) = (entity.position(host), entity.block_position(host)) else {
            continue;
        };
        let d = (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2) + (p[2] - at[2]).powi(2);
        if d < range && best.is_none_or(|(b, _)| d < b) {
            best = Some((d, block));
        }
    }
    best.map(|(_, block)| block)
}

/// `BlockPos.distSqr`.
fn dist_sqr(a: [i32; 3], b: [i32; 3]) -> f64 {
    (0..3)
        .map(|i| (f64::from(a[i]) - f64::from(b[i])).powi(2))
        .sum()
}

/// The world spawn (`ServerLevel.getSharedSpawnPos`).
fn world_spawn(ctx: &Context<'_>) -> Option<[i32; 3]> {
    let overworld = ctx.host.level("minecraft:overworld")?;
    Some(overworld.respawn_data(ctx.host)?.pos)
}

fn time_of_day(query: &Query<'_>, ctx: &Context<'_>) -> Option<i32> {
    if !query.in_level {
        return None;
    }
    let t = query.level.day_time(ctx.host)?;
    Some((t % 24000) as i32)
}

fn light(query: &Query<'_>, ctx: &Context<'_>, layer: LightLayer) -> Option<i32> {
    if chunk_invalid(query, ctx, query.pos) {
        return None;
    }
    query.level.brightness(ctx.host, layer, query.pos)
}

/// `LevelReader.getMaxLocalRawBrightness`: block light or sky light less the sky's darkening,
/// whichever is brighter.
fn full_light(query: &Query<'_>, ctx: &Context<'_>) -> Option<i32> {
    let block = light(query, ctx, LightLayer::Block)?;
    let sky = light(query, ctx, LightLayer::Sky)?;
    let darken = query.level.get_sky_darken(ctx.host).unwrap_or(0);
    Some(block.max(sky - darken))
}

/// The scoreboard tags of a living subject.
fn tags(query: &Query<'_>, ctx: &Context<'_>) -> Option<Vec<String>> {
    match query.subject {
        Subject::Unspawned(mob) => mob.tags(ctx.host).ok(),
        Subject::Entity(e) => {
            ctx.host.living(e)?;
            e.tags(ctx.host)
        }
    }
}

fn test(check: &Check, query: &Query<'_>, ctx: &mut Context<'_>) -> bool {
    let host = ctx.host;
    let level = query.level;
    match check {
        Check::Random(r) => ctx.random.next_float() < *r,
        Check::Dimension(dims) => dims.iter().any(|d| d == query.dimension),
        Check::DimensionMod(mods) => mods.iter().any(|m| m == namespace(query.dimension)),
        Check::Time(e) => time_of_day(query, ctx).is_some_and(|t| e.test(t)),
        Check::MinTime(min) => time_of_day(query, ctx).is_some_and(|t| t >= *min),
        Check::MaxTime(max) => time_of_day(query, ctx).is_some_and(|t| t <= *max),
        Check::Height(e) => e.test(query.pos[1]),
        Check::MinHeight(min) => query.pos[1] >= *min,
        Check::MaxHeight(max) => query.pos[1] <= *max,
        Check::Mods(mods) => mods.iter().any(|m| m == namespace(query.kind)),
        Check::Never => false,
        Check::Raining => query.in_level && level.is_raining(host) == Some(true),
        Check::Thundering => query.in_level && level.is_thundering(host) == Some(true),
        Check::BiomeTags(tags) => level.biome_at(host, query.pos).is_some_and(|biome| {
            tags.iter()
                .any(|t| level.biome_in_tag(host, &biome, t) == Some(true))
        }),
        Check::Difficulty(d) => level.difficulty(host) == Some(d.as_str()),
        Check::MinSpawnDist(v) => {
            world_spawn(ctx).is_some_and(|s| dist_sqr(query.pos, s) >= f64::from(v * v))
        }
        Check::MaxSpawnDist(v) => {
            world_spawn(ctx).is_some_and(|s| dist_sqr(query.pos, s) <= f64::from(v * v))
        }
        Check::MinDist(v) => match closest_player(query, ctx) {
            None => true,
            Some(p) => dist_sqr(query.pos, p) >= f64::from(v * v),
        },
        Check::MaxDist(v) => match closest_player(query, ctx) {
            None => false,
            Some(p) => dist_sqr(query.pos, p) <= f64::from(v * v),
        },
        Check::BlockLight(e) => light(query, ctx, LightLayer::Block).is_some_and(|l| e.test(l)),
        Check::MinBlockLight(v) => light(query, ctx, LightLayer::Block).is_some_and(|l| l >= *v),
        Check::MaxBlockLight(v) => light(query, ctx, LightLayer::Block).is_some_and(|l| l <= *v),
        Check::MinSkyLight(v) => light(query, ctx, LightLayer::Sky).is_some_and(|l| l >= *v),
        Check::MaxSkyLight(v) => light(query, ctx, LightLayer::Sky).is_some_and(|l| l <= *v),
        Check::MinFullLight(v) => full_light(query, ctx).is_some_and(|l| l >= *v),
        Check::MaxFullLight(v) => full_light(query, ctx).is_some_and(|l| l <= *v),
        Check::MinDifficulty(v) => level
            .local_difficulty(host, query.pos)
            .is_some_and(|d| d.effective >= *v),
        Check::MaxDifficulty(v) => level
            .local_difficulty(host, query.pos)
            .is_some_and(|d| d.effective <= *v),
        Check::Slime(want) => {
            let chunk = [query.pos[0] >> 4, query.pos[2] >> 4];
            let slime = level
                .chunk_ref(host, chunk)
                .and_then(|c| c.is_slime_chunk(host))
                .unwrap_or(false);
            slime == *want
        }
        Check::Biomes(biomes) => level
            .biome_at(host, query.pos)
            .is_some_and(|b| biomes.contains(&b)),
        Check::AllScoreboardTags(want) => {
            tags(query, ctx).is_some_and(|have| want.iter().all(|t| have.contains(t)))
        }
        Check::AnyScoreboardTags(want) => {
            tags(query, ctx).is_some_and(|have| have.iter().any(|t| want.contains(t)))
        }
        Check::Baby(want) => match query.subject {
            Subject::Entity(e) => host
                .living(e)
                .and_then(|l| l.baby(host))
                .is_some_and(|b| b == *want),
            Subject::Unspawned(_) => false,
        },
        Check::Spawner(want) => (query.reason == Some(SpawnReason::Spawner)) == *want,
        Check::SpawnType(reasons) => query.reason.is_some_and(|r| reasons.contains(&r)),
        Check::Mobs { ids, tags } => {
            ids.iter().any(|id| id == query.kind)
                || tags.iter().any(|t| host.entity_type_in_tag(query.kind, t))
        }
        Check::MinCount(info) => {
            let count = info.count(ctx.cache, query.dimension, query.kind);
            count >= info.amount(ctx.cache, query.dimension)
        }
        Check::MaxCount(info) => {
            let count = info.count(ctx.cache, query.dimension, query.kind);
            count < info.amount(ctx.cache, query.dimension)
        }
    }
}
