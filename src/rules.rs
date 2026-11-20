//! `SpawnRule` and `RulesManager`: the rules of `config/incontrol/spawn.json`, each asked at one
//! moment of a mob's life (`when`), first match wins unless it says `continue`.
use serde_json::Value;

use crate::attributes::{self, Attr, AttributeMap};
use crate::conditions::{Asked, Context, Evaluator, Query, Registry};
use crate::json::{self, Error};

/// The rule file, under the game's `config/` folder.
pub const SPAWN_FILE: &str = "incontrol/spawn.json";

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
    }))
}

/// The error for a rule that asks for what this port doesn't do yet.
fn skipped(index: usize, map: &AttributeMap) -> String {
    format!(
        "Rule {index} in spawn.json uses what InControl for Horizon can't do yet, so it is skipped: {}",
        map.keys().join(" ")
    )
}

/// `RulesManager`'s spawn rules.
#[derive(Debug, Default)]
pub struct Rules {
    rules: Vec<SpawnRule>,
}

impl Rules {
    /// `readRules` of `spawn.json`'s `text`, None when the file is missing. The rules before a
    /// broken one are kept, as in the original.
    pub fn read(text: Option<&str>, registry: &Registry<'_>, errors: &mut Vec<String>) -> Rules {
        let mut rules = Rules::default();
        let Some(text) = text else {
            return rules;
        };
        if let Err(e) = rules.read_all(text, registry, errors) {
            errors.push(format!(
                "JSON error in 'spawn.json': check log for details ({e})"
            ));
        }
        rules
    }

    fn read_all(
        &mut self,
        text: &str,
        registry: &Registry<'_>,
        errors: &mut Vec<String>,
    ) -> Result<(), Error> {
        let root = json::parse_file(text)?;
        let Value::Array(entries) = root else {
            return Err(format!("Not a JSON Array: {root}"));
        };
        for (i, entry) in entries.iter().enumerate() {
            if let Some(rule) = parse_rule(entry, i, registry, errors)? {
                self.rules.push(rule);
            }
        }
        Ok(())
    }

    /// `getFilteredRules`: the rules asked at `when`, in file order.
    pub fn at(&self, when: When) -> impl Iterator<Item = &SpawnRule> {
        self.rules.iter().filter(move |r| r.when == Some(when))
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
        let rules = Rules::read(Some(text), &registry, &mut errors);
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
        assert_eq!(rules.at(When::Position).count(), 2);
        assert!(
            rules
                .at(When::Position)
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
    fn an_unknown_when_drops_the_rule_from_every_moment() {
        let (rules, errors) = read(r#"[{"when": "later", "result": "deny"}]"#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules.at(When::Position).count(), 0);
        assert_eq!(errors.len(), 1);
    }
}
