//! InControl for Horizon: rules in `config/incontrol/spawn.json` that deny, allow or keep mob
//! spawns by mob, place, time, light, weather and how many of a mob a level already has.
//!
//! A port of McJty's InControl (MIT), from its 1.21 sources (10.3.1, the version All the Mods 10
//! ships). This part holds the spawn rules with the conditions Horizon's mod API can check, the
//! mob counts behind `mincount` and `maxcount`, and the `/incontrol` commands that go with them.
mod attributes;
mod commands;
mod conditions;
mod config;
mod counts;
mod expression;
mod json;
mod rules;

use std::collections::HashSet;
use std::sync::Mutex;

use mod_api::{
    ConfigPath, Despawn, EntityJoinLevelEvent, EntityLeaveLevelEvent, FinalizeSpawnEvent, Flow,
    Hooks, Host, HostSide, LevelRef, LevelTickPreEvent, MobDespawnEvent,
    MobSpawnPositionCheckEvent, PlayerLoggedInEvent, RegisterCommandsEvent, RunModCommandEvent,
    ServerHost, ServerStartedEvent, SpawnCheck, SpawnReason, Text,
};

use crate::commands::Run;
use crate::conditions::{Context, Query, Random, Registry, Subject};
use crate::config::Config;
use crate::counts::{Census, RuleCache};
use crate::rules::{Outcome, Rules, When};

/// `Statistics`: how often each rule fired, by its place among the rules asked.
#[derive(Debug, Default)]
struct Statistics {
    spawns: Vec<(u32, bool)>,
}

impl Statistics {
    fn add(&mut self, rule: usize, deny: bool) {
        if self.spawns.len() <= rule {
            self.spawns.resize(rule + 1, (0, false));
        }
        self.spawns[rule].0 += 1;
        self.spawns[rule].1 = deny;
    }
}

#[derive(Debug, Default)]
struct State {
    config: Config,
    rules: Rules,
    cache: RuleCache,
    stats: Statistics,
    random: Random,
    /// `ErrorHandler.errors`: shown to every player who joins until the rules are reloaded.
    errors: Vec<String>,
}

#[derive(Default)]
pub struct InControl {
    state: Mutex<State>,
}

/// `(int)` of a coordinate: towards zero, as Java casts.
fn block(pos: [f64; 3]) -> [i32; 3] {
    pos.map(|c| c as i32)
}

/// The legacy-coloured error line the original sends (`ErrorHandler.error`).
fn error_line(message: &str) -> Text {
    Text::literal(format!("\u{a7}cInControl Error: \u{a7}6{message}"))
}

/// The entity type ids the game has.
fn entity_types(host: &dyn Host) -> HashSet<String> {
    host.entity_type_keys()
        .into_iter()
        .map(|k| k.id(host))
        .collect()
}

impl InControl {
    /// `ErrorHandler.error` for each of `errors`: kept, and told to every player online.
    fn report(&self, host: &mut dyn ServerHost, errors: Vec<String>) {
        let mut fresh = Vec::new();
        {
            let mut state = self.state.lock().unwrap();
            for e in errors {
                if !state.errors.contains(&e) {
                    state.errors.push(e.clone());
                }
                fresh.push(e);
            }
        }
        let players = host.players().server_players();
        for e in fresh {
            for &player in &players {
                let _ = player.tell(host, error_line(&e));
            }
        }
    }

    /// `RulesManager.reloadRules` (and the config with it), creating an empty rule file where
    /// there is none.
    fn load(&self, host: &mut dyn ServerHost) {
        let files = host.config_files();
        let config = ConfigPath::new(config::FILE)
            .ok()
            .map(|path| match files.read(&path) {
                Ok(bytes) => Config::parse(&String::from_utf8_lossy(&bytes)),
                Err(_) => {
                    let _ = files.write(&path, config::DEFAULT.as_bytes());
                    Config::default()
                }
            });
        let text =
            ConfigPath::new(rules::SPAWN_FILE)
                .ok()
                .and_then(|path| match files.read(&path) {
                    Ok(bytes) => Some(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(_) => {
                        // `JSonTools.makeEmptyRuleFile`.
                        let _ = files.write(&path, b"[]");
                        None
                    }
                });
        let known = entity_types(host);
        let is_known = |id: &str| known.contains(id);
        let registry = Registry {
            entity_type: &is_known,
        };
        let mut errors = Vec::new();
        let rules = Rules::read(text.as_deref(), &registry, &mut errors);
        {
            let mut state = self.state.lock().unwrap();
            state.config = config.unwrap_or_default();
            state.rules = rules;
        }
        self.report(host, errors);
    }

    /// The rules asked at `when` for `query`, in order, until one matches without `continue`;
    /// the outcomes of those that matched, with each one's place for the statistics.
    fn ask(&self, host: &dyn Host, when: When, query: &Query<'_>) -> Vec<(usize, Outcome)> {
        let mut guard = self.state.lock().unwrap();
        let state = &mut *guard;
        let mut ctx = Context {
            host,
            cache: &state.cache,
            radius: state.config.per_player_radius,
            random: &mut state.random,
        };
        let mut matched = Vec::new();
        for (i, rule) in state.rules.at(when).enumerate() {
            if rule.matches(query, &mut ctx) {
                matched.push((i, rule.result));
                if !rule.keep_going {
                    break;
                }
            }
        }
        matched
    }

    fn count_stats(&self, matched: &[(usize, Outcome)]) {
        let mut state = self.state.lock().unwrap();
        for &(i, outcome) in matched {
            let deny = matches!(outcome, Outcome::Deny | Outcome::DenyWithActions);
            state.stats.add(i, deny);
        }
    }

    /// `performCount` for `level`, when it is due.
    fn count(&self, host: &mut dyn ServerHost, level: LevelRef) {
        let Some(dimension) = level.dimension().id(host) else {
            return;
        };
        let retention = {
            let mut state = self.state.lock().unwrap();
            if !state.cache.due(&dimension) {
                return;
            }
            state.config.cache_retention_ticks
        };
        let mut mobs = Vec::new();
        for entity in level.all_entities(host).unwrap_or_default() {
            if host.mob(entity).is_some()
                && let Some(kind) = entity.kind(host)
            {
                mobs.push(kind.id(host));
            }
        }
        let chunks = level.loaded_chunks(host).map_or(0, |c| c.len() as i32);
        let players = level
            .online_players(host)
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.is_spectator(host) != Some(true))
            .count() as i32;
        let census = Census {
            mobs,
            chunks,
            players,
        };
        self.state
            .lock()
            .unwrap()
            .cache
            .recount(&dimension, census, retention);
    }

    fn show_stats(&self, host: &mut dyn ServerHost) {
        let lines: Vec<String> = {
            let state = self.state.lock().unwrap();
            let mut lines = vec!["### Spawner ###".to_owned(), "### Spawn ###".to_owned()];
            for (i, (count, deny)) in state.stats.spawns.iter().enumerate() {
                let what = if *deny { "deny)" } else { "allow)" };
                lines.push(format!("Rule {i} fired {count} times ({what}"));
            }
            lines
        };
        for line in lines {
            commands::send(host, Text::literal(line));
        }
    }

    fn run(&self, host: &mut dyn ServerHost, source: &mod_api::CommandSource, what: Run) {
        match what {
            Run::ShowStats => self.show_stats(host),
            Run::ClearStats => self.state.lock().unwrap().stats = Statistics::default(),
            Run::ShowMobs => {
                if commands::player(host, source).is_none() {
                    return;
                }
                let mut ids: Vec<String> = entity_types(host).into_iter().collect();
                ids.sort();
                for id in ids {
                    commands::send(host, Text::literal(format!("Mob:{id}")));
                }
            }
            Run::Reload => {
                let Some(player) = commands::player(host, source) else {
                    return;
                };
                self.state.lock().unwrap().errors.clear();
                let _ = player.tell(host, Text::literal("Reloaded InControl rules"));
                self.load(host);
            }
            Run::List => {
                if let Some(player) = commands::player(host, source)
                    && let Some(level) = commands::level_of(host, player)
                {
                    commands::list(host, level);
                }
            }
            Run::Kill(kind) => {
                if let Some(player) = commands::player(host, source)
                    && let Some(level) = commands::level_of(host, player)
                {
                    commands::kill(host, level, &kind);
                }
            }
        }
    }
}

/// A living entity that isn't a player, as the original's join handlers take.
fn living_mob(host: &dyn Host, entity: mod_api::EntityRef, kind: &str) -> bool {
    host.living(entity).is_some() && kind != "minecraft:player"
}

#[mod_api::horizon_mod]
impl Hooks for InControl {
    fn server_started(&self, host: &mut dyn ServerHost, _: &mut ServerStartedEvent) {
        self.load(host);
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        // `ErrorHandler.onPlayerJoinWorld`.
        let errors = self.state.lock().unwrap().errors.clone();
        let Some(player) = host.player(event.player) else {
            return;
        };
        for e in errors {
            let _ = player.tell(host, error_line(&e));
        }
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        event.commands.extend(commands::usages());
    }

    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        let Some(what) = commands::parse(event.command) else {
            return Flow::Continue;
        };
        self.run(host, event.source, what);
        Flow::Cancel
    }

    fn level_tick_pre(&self, host: HostSide, event: &mut LevelTickPreEvent) {
        // `onWorldTick`: the counts, on the server.
        if let HostSide::Server(host) = host {
            self.count(host, event.level);
        }
    }

    fn entity_join_level(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EntityJoinLevelEvent<'_>,
    ) -> Flow {
        let Some(entity) = host.entity(event.entity) else {
            return Flow::Continue;
        };
        if !living_mob(host, entity, event.kind) {
            return Flow::Continue;
        }
        // `onEntityJoinWorldLast`, at the highest priority: the mob counts.
        if host.mob(entity).is_some() {
            self.state
                .lock()
                .unwrap()
                .cache
                .join(event.dimension, event.kind);
        }
        // `onEntityJoinWorld`, at the lowest: the rules asked on joining.
        let Some(level) = host.level(event.dimension) else {
            return Flow::Continue;
        };
        let Some(pos) = entity.block_position(host) else {
            return Flow::Continue;
        };
        let query = Query {
            level,
            dimension: event.dimension,
            pos,
            kind: event.kind,
            subject: Subject::Entity(entity),
            reason: None,
            in_level: true,
        };
        let matched = self.ask(host, When::OnJoin, &query);
        self.count_stats(&matched);
        if matched
            .iter()
            .any(|(_, o)| matches!(o, Outcome::Deny | Outcome::DenyWithActions))
        {
            return Flow::Cancel;
        }
        Flow::Continue
    }

    fn entity_leave_level(&self, _: &mut dyn ServerHost, event: &mut EntityLeaveLevelEvent<'_>) {
        // `onEntityLeavesWorld`.
        self.state
            .lock()
            .unwrap()
            .cache
            .leave(event.dimension, event.kind);
    }

    fn mob_spawn_position_check(
        &self,
        host: &dyn Host,
        event: &mut MobSpawnPositionCheckEvent<'_>,
    ) {
        let pos = block(event.pos);
        let query = Query {
            level: event.level,
            dimension: event.dimension,
            pos,
            kind: event.kind,
            subject: Subject::Unspawned(event.mob),
            reason: Some(event.reason),
            in_level: event.reason != SpawnReason::ChunkGeneration,
        };
        let matched = self.ask(host, When::Position, &query);
        self.count_stats(&matched);
        for (_, outcome) in matched {
            *event.result = match outcome {
                Outcome::Allow => SpawnCheck::Succeed,
                Outcome::Default => SpawnCheck::Default,
                Outcome::Deny | Outcome::DenyWithActions => SpawnCheck::Fail,
            };
        }
    }

    fn finalize_spawn(&self, host: &dyn Host, event: &mut FinalizeSpawnEvent<'_>) -> Flow {
        let Some(level) = host.level(event.dimension) else {
            return Flow::Continue;
        };
        let pos = block(event.pos);
        let query = Query {
            level,
            dimension: event.dimension,
            pos,
            kind: event.kind,
            subject: Subject::Unspawned(event.mob),
            reason: Some(event.reason),
            in_level: event.reason != SpawnReason::ChunkGeneration,
        };
        let mut flow = Flow::Continue;
        for (_, outcome) in self.ask(host, When::Finalize, &query) {
            match outcome {
                // The actions run instead of the mob's own finalization.
                Outcome::Allow => flow = Flow::Cancel,
                Outcome::Default => {}
                Outcome::Deny | Outcome::DenyWithActions => {
                    event.spawn_cancelled = true;
                    flow = Flow::Cancel;
                }
            }
        }
        flow
    }

    fn mob_despawn(&self, host: &dyn Host, event: &mut MobDespawnEvent<'_>) {
        let Some(level) = host.level(event.dimension) else {
            return;
        };
        let pos = block(event.pos);
        let query = Query {
            level,
            dimension: event.dimension,
            pos,
            kind: event.kind,
            subject: Subject::Entity(event.mob.entity()),
            reason: None,
            in_level: true,
        };
        for (_, outcome) in self.ask(host, When::Despawn, &query) {
            *event.result = match outcome {
                Outcome::Allow => Despawn::Allow,
                Outcome::Default => Despawn::Default,
                Outcome::Deny | Outcome::DenyWithActions => Despawn::Deny,
            };
        }
    }
}
