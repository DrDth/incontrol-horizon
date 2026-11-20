//! `SpawnRule` and `RulesManager`: the rules of `config/incontrol/spawn.json`, each asked at one
//! moment of a mob's life (`when`), first match wins unless it says `continue`.
use std::collections::BTreeSet;

use serde_json::Value;

use crate::attributes::{self, Attr, AttributeMap};
use crate::conditions::{Asked, Context, Evaluator, Query, Registry};
use crate::json::{self, Error};

/// The rule files, under the game's `config/` folder.
pub const SPAWN_FILE: &str = "incontrol/spawn.json";
pub const PHASES_FILE: &str = "incontrol/phases.json";

/// `SpawnWhen`: when a rule is asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// `MobSpawnEvent.PositionCheck`, the default.
    Position,
    /// `EntityJoinLevelEvent`.
    OnJoin,
    /// `FinalizeSpawnEvent`.
    Finalize,
    /// `MobDespawnEvent`.
    Despawn,
}

impl When {
    fn by_name(name: &str) -> Option<When> {
        Some(match name.to_uppercase().as_str() {
            "POSITION" => When::Position,
            "ONJOIN" => When::OnJoin,
            "FINALIZE" => When::Finalize,
            "DESPAWN" => When::Despawn,
            _ => return None,
        })
    }
}

/// `ICResult`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Allow,
    Default,
    Deny,
    DenyWithActions,
}

/// One rule of `spawn.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnRule {
    when: Option<When>,
    evaluator: Evaluator,
    pub result: Outcome,
    pub keep_going: bool,
    /// `PhaseTools.getPhases`: the phases that must all be on for the rule to be asked.
    phases: Vec<String>,
}

impl SpawnRule {
    pub fn matches(&self, query: &Query<'_>, ctx: &mut Context<'_>) -> bool {
        self.evaluator.matches(query, ctx)
    }
}

/// The keywords that are actions rather than conditions: what a rule does to the mob it
/// matches (`RuleBase.addActions`).
fn is_action(key: &str) -> bool {
    let first = attributes::SPAWN
        .iter()
        .position(|(k, _, _)| *k == "customevent")
        .unwrap_or(usize::MAX);
    attributes::SPAWN
        .iter()
        .position(|(k, _, _)| *k == key)
        .is_some_and(|i| i >= first)
        && key != "result"
        && key != "continue"
}

/// `SpawnRule.parse(element, index)`: Ok(None) for a rule this port skips, Err where the original
/// throws, which ends the file.
fn parse_rule(
    element: &Value,
    index: usize,
    registry: &Registry<'_>,
    errors: &mut Vec<String>,
) -> Result<Option<SpawnRule>, Error> {
    let Value::Object(object) = element else {
        return Err(format!("Not a JSON Object: {element}"));
    };
    let mut map = attributes::parse(attributes::SPAWN, object, "spawn.json", errors)?;
    let when = match object.get("when") {
        Some(w) => {
            let name = json::as_string(w)?;
            let when = When::by_name(&name);
            if when.is_none() {
                errors.push(format!(
                    "Invalid spawn rule 'when' value '{name}'!. Should be one of [POSITION, ONJOIN, FINALIZE, DESPAWN]"
                ));
            }
            when
        }
        None => Some(When::Position),
    };
    let phases = match object.get("phase") {
        Some(Value::Array(list)) => list.iter().map(json::as_string).collect::<Result<_, _>>()?,
        Some(p) => vec![json::as_string(p)?],
        None => Vec::new(),
    };
    let asked = match when {
        Some(When::Position | When::Finalize) => Asked::Spawn,
        _ => Asked::InLevel,
    };
    let evaluator = Evaluator::new(&mut map, asked, registry, errors)?;
    let result = match map.consume("result") {
        Some(Attr::Str(r)) => {
            if r == "default" || r.starts_with("def") {
                Outcome::Default
            } else if r == "allow" || r == "true" {
                Outcome::Allow
            } else if r == "deny_with_actions" {
                Outcome::DenyWithActions
            } else {
                Outcome::Deny
            }
        }
        _ => Outcome::Default,
    };
    let keep_going = map.consume("continue").is_some_and(|c| c.bool());
    if result == Outcome::Deny {
        // A plain deny never runs its actions.
        for key in map.keys() {
            if is_action(key) {
                map.consume_list(key);
            }
        }
    }
    if !map.is_empty() {
        errors.push(skipped(index, &map));
        return Ok(None);
    }
    Ok(Some(SpawnRule {
        when,
        evaluator,
        result,
        keep_going,
        phases,
    }))
}

/// `PhaseRule`: a phase that is on while its conditions hold.
#[derive(Clone, Debug, PartialEq)]
pub struct PhaseRule {
    pub name: String,
    evaluator: Evaluator,
}

impl PhaseRule {
    pub fn matches(&self, query: &Query<'_>, ctx: &mut Context<'_>) -> bool {
        self.evaluator.matches(query, ctx)
    }
}

/// `PhaseRule.parse`: `{"name": ..., "conditions": {...}}`.
fn parse_phase(
    element: &Value,
    _index: usize,
    registry: &Registry<'_>,
    errors: &mut Vec<String>,
) -> Result<Option<PhaseRule>, Error> {
    let Value::Object(object) = element else {
        return Err(format!("Not a JSON Object: {element}"));
    };
    let conditions = match object.get("conditions") {
        Some(Value::Object(c)) => c,
        other => {
            let shown = other.map_or("null".to_owned(), Value::to_string);
            return Err(format!("Not a JSON Object: {shown}"));
        }
    };
    let mut map = attributes::parse(attributes::PHASES, conditions, "phases.json", errors)?;
    let evaluator = Evaluator::new(&mut map, Asked::InLevel, registry, errors)?;
    let name = object.get("name").ok_or("No name")?;
    Ok(Some(PhaseRule {
        name: json::as_string(name)?,
        evaluator,
    }))
}

type Parse<T> = fn(&Value, usize, &Registry<'_>, &mut Vec<String>) -> Result<Option<T>, Error>;

/// `RulesManager.readRules` of one file: its rules up to a broken one, which is reported.
fn read_file<T>(
    text: Option<&str>,
    file: &str,
    parse: Parse<T>,
    registry: &Registry<'_>,
    errors: &mut Vec<String>,
) -> Vec<T> {
    let mut rules = Vec::new();
    let Some(text) = text else {
        return rules;
    };
    let mut read = || -> Result<(), Error> {
        let root = json::parse_file(text)?;
        let Value::Array(entries) = root else {
            return Err(format!("Not a JSON Array: {root}"));
        };
        for (i, entry) in entries.iter().enumerate() {
            if let Some(rule) = parse(entry, i, registry, errors)? {
                rules.push(rule);
            }
        }
        Ok(())
    };
    if let Err(e) = read() {
        errors.push(format!(
            "JSON error in '{file}': check log for details ({e})"
        ));
    }
    rules
}

/// The error for a rule that asks for what this port doesn't do yet.
fn skipped(index: usize, map: &AttributeMap) -> String {
    format!(
        "Rule {index} in spawn.json uses what InControl for Horizon can't do yet, so it is skipped: {}",
        map.keys().join(" ")
    )
}

/// `RulesManager`'s spawn and phase rules.
#[derive(Debug, Default)]
pub struct Rules {
    rules: Vec<SpawnRule>,
    pub phases: Vec<PhaseRule>,
}

impl Rules {
    /// The rules of `spawn.json` and `phases.json`, None for a missing file. The rules before a
    /// broken one are kept, as in the original.
    pub fn read(
        spawn: Option<&str>,
        phases: Option<&str>,
        registry: &Registry<'_>,
        errors: &mut Vec<String>,
    ) -> Rules {
        Rules {
            rules: read_file(spawn, "spawn.json", parse_rule, registry, errors),
            phases: read_file(phases, "phases.json", parse_phase, registry, errors),
        }
    }

    /// `getFilteredRules`: the rules asked at `when` while all their phases are on, in file
    /// order.
    pub fn at<'a>(
        &'a self,
        when: When,
        on: &'a BTreeSet<String>,
    ) -> impl Iterator<Item = &'a SpawnRule> {
        self.rules
            .iter()
            .filter(move |r| r.when == Some(when) && r.phases.iter().all(|p| on.contains(p)))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> (Rules, Vec<String>) {
        let known = |id: &str| id.starts_with("minecraft:") || id.starts_with("irons_spellbooks:");
        let registry = Registry {
            entity_type: &known,
        };
        let mut errors = Vec::new();
        let rules = Rules::read(Some(text), None, &registry, &mut errors);
        (rules, errors)
    }

    #[test]
    fn atm10s_mob_caps_read_as_position_denials() {
        let (rules, errors) = read(
            r#"[
                {"mob": "minecraft:zombie", "mincount": 15, "result": "deny"},
                {"mob": "irons_spellbooks:necromancer", "mincount": 15, "result": "deny"}
            ]"#,
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(rules.at(When::Position, &BTreeSet::new()).count(), 2);
        assert!(
            rules
                .at(When::Position, &BTreeSet::new())
                .all(|r| r.result == Outcome::Deny && !r.keep_going)
        );
    }

    #[test]
    fn a_mob_from_a_missing_mod_is_reported_and_read_as_the_pig() {
        let (rules, errors) =
            read(r#"[{"mob": "creeperoverhaul:cave_creeper", "mincount": 15, "result": "deny"}]"#);
        assert_eq!(errors, vec!["Unknown mob 'creeperoverhaul:cave_creeper'!"]);
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn a_rule_with_what_the_port_lacks_is_skipped_and_reported() {
        let (rules, errors) = read(
            r#"[{"mob": "minecraft:cow", "result": "allow", "healthmultiply": 2},
                {"mob": "minecraft:cow", "result": "deny", "healthmultiply": 2},
                {"mob": "minecraft:cow", "hostile": true}]"#,
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(errors.len(), 2);
        assert!(errors[0].starts_with("Rule 0 in spawn.json"));
        assert!(errors[0].ends_with("healthmultiply"));
        assert!(errors[1].ends_with("hostile"));
    }

    #[test]
    fn a_broken_rule_ends_the_file_and_keeps_those_before() {
        let (rules, errors) = read(
            r#"[{"mob": "minecraft:cow", "result": "deny"},
                {"mob": "minecraft:cow", "spawntype": "sneezing"},
                {"mob": "minecraft:pig", "result": "deny"}]"#,
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("JSON error in 'spawn.json'"));
    }

    #[test]
    fn a_rule_with_phases_is_asked_only_while_they_are_on() {
        let (rules, errors) = read(
            r#"[{"phase": ["night", "blood"], "mob": "minecraft:zombie", "result": "deny"},
                {"phase": "night", "result": "deny"}]"#,
        );
        assert!(errors.is_empty(), "{errors:?}");
        let mut on = BTreeSet::new();
        assert_eq!(rules.at(When::Position, &on).count(), 0);
        on.insert("night".to_owned());
        assert_eq!(rules.at(When::Position, &on).count(), 1);
        on.insert("blood".to_owned());
        assert_eq!(rules.at(When::Position, &on).count(), 2);
    }

    #[test]
    fn phase_rules_read_their_conditions() {
        let known = |_: &str| true;
        let registry = Registry {
            entity_type: &known,
        };
        let mut errors = Vec::new();
        let rules = Rules::read(
            None,
            Some(
                r#"[{"name": "night", "conditions": {"mintime": 13000}},
                    {"name": "always", "conditions": {"mob": "minecraft:zombie"}}]"#,
            ),
            &registry,
            &mut errors,
        );
        // A keyword phases don't know empties the conditions, as in the original: always on.
        assert_eq!(rules.phases.len(), 2);
        assert_eq!(errors, vec!["Invalid keywords for phases.json: mob"]);
    }

    #[test]
    fn an_unknown_when_drops_the_rule_from_every_moment() {
        let (rules, errors) = read(r#"[{"when": "later", "result": "deny"}]"#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules.at(When::Position, &BTreeSet::new()).count(), 0);
        assert_eq!(errors.len(), 1);
    }
}
