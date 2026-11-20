//! `GenericAttributeMapFactory` and `AttributeMap`: a rule's JSON object read into typed values by
//! the keywords its file knows (`RuleKeys`), each consumed once by the check or action it makes.
use serde_json::{Map, Value};

use crate::json::{self, Error};

/// `Type`: how an attribute's JSON is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Integer,
    Float,
    Boolean,
    String,
    /// Kept as JSON text and parsed by the check that takes it.
    Json,
    /// A number, boolean or string, or an object as JSON text.
    Object,
    /// A dimension id.
    DimensionType,
    /// A Custom NPCs mob.
    Npc,
}

/// One attribute's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Attr {
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
}

impl Attr {
    pub fn int(&self) -> i32 {
        match self {
            Attr::Int(i) => *i,
            Attr::Float(f) => *f as i32,
            _ => 0,
        }
    }

    pub fn float(&self) -> f32 {
        match self {
            Attr::Float(f) => *f,
            Attr::Int(i) => *i as f32,
            _ => 0.0,
        }
    }

    pub fn bool(&self) -> bool {
        matches!(self, Attr::Bool(true))
    }

    pub fn str(&self) -> &str {
        match self {
            Attr::Str(s) => s,
            _ => "",
        }
    }
}

/// `SpawnRule`'s factory: every keyword `spawn.json` knows, its type and whether it takes a list.
pub const SPAWN: &[(&str, Kind, bool)] = &[
    ("when", Kind::String, false),
    ("phase", Kind::String, true),
    ("number", Kind::Json, false),
    ("kubejs", Kind::Json, true),
    ("time", Kind::String, false),
    ("mintime", Kind::Integer, false),
    ("maxtime", Kind::Integer, false),
    ("daycount", Kind::Object, false),
    ("mindaycount", Kind::Integer, false),
    ("maxdaycount", Kind::Integer, false),
    ("mincount", Kind::Json, false),
    ("maxcount", Kind::Json, false),
    ("light", Kind::String, false),
    ("minlight", Kind::Integer, false),
    ("maxlight", Kind::Integer, false),
    ("minlight_full", Kind::Integer, false),
    ("maxlight_full", Kind::Integer, false),
    ("minlight_sky", Kind::Integer, false),
    ("maxlight_sky", Kind::Integer, false),
    ("height", Kind::String, false),
    ("minheight", Kind::Integer, false),
    ("maxheight", Kind::Integer, false),
    ("mindifficulty", Kind::Float, false),
    ("maxdifficulty", Kind::Float, false),
    ("mindist", Kind::Float, false),
    ("maxdist", Kind::Float, false),
    ("minspawndist", Kind::Float, false),
    ("maxspawndist", Kind::Float, false),
    ("random", Kind::Float, false),
    ("canspawnhere", Kind::Boolean, false),
    ("notcolliding", Kind::Boolean, false),
    ("spawner", Kind::Boolean, false),
    ("spawntype", Kind::String, true),
    ("incontrol", Kind::Boolean, false),
    ("eventspawn", Kind::Boolean, false),
    ("inbuilding", Kind::Boolean, false),
    ("inmultibuilding", Kind::Boolean, false),
    ("building", Kind::String, true),
    ("multibuilding", Kind::String, true),
    ("incity", Kind::Boolean, false),
    ("instreet", Kind::Boolean, false),
    ("insphere", Kind::Boolean, false),
    ("gamestage", Kind::String, false),
    ("passive", Kind::Boolean, false),
    ("hostile", Kind::Boolean, false),
    ("baby", Kind::Boolean, false),
    ("cave", Kind::Boolean, false),
    ("seesky", Kind::Boolean, false),
    ("slime", Kind::Boolean, false),
    ("weather", Kind::String, false),
    ("biometags", Kind::String, true),
    ("difficulty", Kind::String, false),
    ("hasstructure", Kind::Boolean, false),
    ("structure", Kind::String, true),
    ("structuretags", Kind::String, true),
    ("winter", Kind::Boolean, false),
    ("summer", Kind::Boolean, false),
    ("spring", Kind::Boolean, false),
    ("autumn", Kind::Boolean, false),
    ("mob", Kind::String, true),
    ("npc", Kind::Npc, true),
    ("mod", Kind::String, true),
    ("area", Kind::String, false),
    ("block", Kind::Json, true),
    ("blockoffset", Kind::Json, false),
    ("blocktest", Kind::Json, true),
    ("biome", Kind::String, true),
    ("dimension", Kind::DimensionType, true),
    ("dimensionmod", Kind::String, true),
    ("state", Kind::String, false),
    ("scoreboardtags_all", Kind::String, true),
    ("scoreboardtags_any", Kind::String, true),
    ("helmet", Kind::Json, true),
    ("chestplate", Kind::Json, true),
    ("leggings", Kind::Json, true),
    ("boots", Kind::Json, true),
    ("amulet", Kind::Json, true),
    ("ring", Kind::Json, true),
    ("belt", Kind::Json, true),
    ("trinket", Kind::Json, true),
    ("head", Kind::Json, true),
    ("body", Kind::Json, true),
    ("charm", Kind::Json, true),
    ("playerhelditem", Kind::Json, true),
    ("offhanditem", Kind::Json, true),
    ("bothhandsitem", Kind::Json, true),
    ("customevent", Kind::String, false),
    ("continue", Kind::Boolean, false),
    ("result", Kind::String, false),
    ("message", Kind::String, false),
    ("ai", Kind::Json, false),
    ("addscoreboardtags", Kind::Json, false),
    ("healthset", Kind::Float, false),
    ("healthmultiply", Kind::Float, false),
    ("healthadd", Kind::Float, false),
    ("speedset", Kind::Float, false),
    ("speedmultiply", Kind::Float, false),
    ("speedadd", Kind::Float, false),
    ("damageset", Kind::Float, false),
    ("damagemultiply", Kind::Float, false),
    ("damageadd", Kind::Float, false),
    ("armorset", Kind::Float, false),
    ("armormultiply", Kind::Float, false),
    ("armoradd", Kind::Float, false),
    ("armortoughnessset", Kind::Float, false),
    ("armortoughnessmultiply", Kind::Float, false),
    ("armortoughnessadd", Kind::Float, false),
    ("attackspeedset", Kind::Float, false),
    ("attackspeedmultiply", Kind::Float, false),
    ("attackspeedadd", Kind::Float, false),
    ("followrangeset", Kind::Float, false),
    ("followrangemultiply", Kind::Float, false),
    ("followrangeadd", Kind::Float, false),
    ("knockbackset", Kind::Float, false),
    ("knockbackmultiply", Kind::Float, false),
    ("knockbackadd", Kind::Float, false),
    ("knockbackresistanceset", Kind::Float, false),
    ("knockbackresistancemultiply", Kind::Float, false),
    ("knockbackresistanceadd", Kind::Float, false),
    ("sizemultiply", Kind::Float, false),
    ("sizeadd", Kind::Float, false),
    ("angry", Kind::Boolean, false),
    ("nbt", Kind::Json, false),
    ("customname", Kind::String, false),
    ("setphase", Kind::String, false),
    ("clearphase", Kind::String, false),
    ("togglephase", Kind::String, false),
    ("changenumber", Kind::String, false),
    ("helditem", Kind::Json, true),
    ("armorboots", Kind::Json, true),
    ("armorlegs", Kind::Json, true),
    ("armorchest", Kind::Json, true),
    ("armorhelmet", Kind::Json, true),
    ("potion", Kind::String, true),
    ("potionnoparticles", Kind::String, true),
    ("nodespawn", Kind::Boolean, false),
];

/// A rule's attributes, in the factory's order, each until a check or action consumes it.
#[derive(Debug, Default)]
pub struct AttributeMap {
    values: Vec<(&'static str, Vec<Attr>)>,
}

impl AttributeMap {
    /// The single value of `key`, taken out of the map.
    pub fn consume(&mut self, key: &str) -> Option<Attr> {
        self.consume_list(key).and_then(|v| v.into_iter().next())
    }

    /// Every value of `key` (`consumeAsList`), taken out of the map.
    pub fn consume_list(&mut self, key: &str) -> Option<Vec<Attr>> {
        let i = self.values.iter().position(|(k, _)| *k == key)?;
        Some(self.values.remove(i).1)
    }

    /// Puts a consumed value back, for a condition this port can't check yet.
    pub fn unconsumed(&mut self, key: &'static str, value: Attr) {
        self.set(key, value);
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The keywords no one consumed.
    pub fn keys(&self) -> Vec<&'static str> {
        self.values.iter().map(|(k, _)| *k).collect()
    }

    fn set(&mut self, key: &'static str, value: Attr) {
        self.values.push((key, vec![value]));
    }

    fn add(&mut self, key: &'static str, value: Attr) {
        match self.values.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(value),
            None => self.values.push((key, vec![value])),
        }
    }
}

/// One element of a list attribute (`GenericAttributeMapFactory.parse`'s transformers).
fn element(kind: Kind, value: &Value) -> Result<Attr, Error> {
    Ok(match kind {
        Kind::Integer => Attr::Int(json::as_int(value)?),
        Kind::Float => Attr::Float(json::as_float(value)?),
        Kind::Boolean => Attr::Bool(json::as_bool(value)?),
        Kind::String => Attr::Str(json::as_string(value)?),
        Kind::Json => Attr::Str(value.to_string()),
        Kind::DimensionType => Attr::Str(json::id(&json::as_string(value)?)),
        // The rule object is never a primitive, so the original's OBJECT list errors.
        Kind::Object => return Err("Expected a primitive".to_owned()),
        Kind::Npc => {
            let tab = value
                .get("cloneTab")
                .ok_or("No cloneTab")
                .and_then(|v| json::as_int(v).map_err(|_| "Bad cloneTab"))?;
            let name = value
                .get("cloneName")
                .ok_or("No cloneName")
                .and_then(|v| json::as_string(v).map_err(|_| "Bad cloneName"))?;
            Attr::Str(format!("{tab}:{name}"))
        }
    })
}

/// A primitive or object as JSON text, for `JSON` and `OBJECT` attributes.
fn json_text(key: &str, value: &Value) -> Result<Option<Attr>, Error> {
    Ok(match value {
        Value::Object(o) => Some(Attr::Str(Value::Object(o.clone()).to_string())),
        Value::String(s) => Some(Attr::Str(s.clone())),
        Value::Number(_) => Some(Attr::Str(json::as_int(value)?.to_string())),
        Value::Bool(_) => return Err(format!("Bad type for key '{key}'!")),
        _ => None,
    })
}

/// The rule `object`'s attributes. Keywords the file doesn't know are reported, and the rule then
/// has no attributes at all, as in the original.
pub fn parse(
    attributes: &[(&'static str, Kind, bool)],
    object: &Map<String, Value>,
    file: &str,
    errors: &mut Vec<String>,
) -> Result<AttributeMap, Error> {
    let mut map = AttributeMap::default();
    let unknown: Vec<&str> = object
        .keys()
        .map(String::as_str)
        .filter(|k| !attributes.iter().any(|(name, _, _)| name == k))
        .collect();
    if !unknown.is_empty() {
        errors.push(format!(
            "Invalid keywords for {file}: {}",
            unknown.join(" ")
        ));
        return Ok(map);
    }
    for &(key, kind, multi) in attributes {
        let Some(value) = object.get(key) else {
            continue;
        };
        if multi {
            for v in json::array_or_single(value) {
                map.add(key, element(kind, v)?);
            }
            continue;
        }
        let attr = match kind {
            Kind::Integer => Some(Attr::Int(json::as_int(value)?)),
            Kind::Float => Some(Attr::Float(json::as_float(value)?)),
            Kind::Boolean => Some(Attr::Bool(json::as_bool(value)?)),
            Kind::String => Some(Attr::Str(json::as_string(value)?)),
            Kind::DimensionType => Some(Attr::Str(json::id(&json::as_string(value)?))),
            Kind::Npc => Some(element(kind, value)?),
            Kind::Json | Kind::Object => json_text(key, value)?,
        };
        if let Some(attr) = attr {
            map.set(key, attr);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(text: &str) -> (AttributeMap, Vec<String>) {
        let value: Value = serde_json::from_str(text).unwrap();
        let mut errors = Vec::new();
        let map = parse(SPAWN, value.as_object().unwrap(), "spawn.json", &mut errors).unwrap();
        (map, errors)
    }

    #[test]
    fn values_are_read_by_their_keyword_type() {
        let (mut map, errors) = rule(
            r#"{"mob": "minecraft:zombie", "mincount": 15, "result": "deny", "dimension": "overworld"}"#,
        );
        assert!(errors.is_empty());
        assert_eq!(
            map.consume_list("mob"),
            Some(vec![Attr::Str("minecraft:zombie".into())])
        );
        assert_eq!(map.consume("mincount"), Some(Attr::Str("15".into())));
        assert_eq!(
            map.consume("dimension"),
            Some(Attr::Str("minecraft:overworld".into()))
        );
        assert_eq!(map.keys(), vec!["result"]);
    }

    #[test]
    fn an_unknown_keyword_leaves_the_rule_empty() {
        let (map, errors) = rule(r#"{"mob": "minecraft:zombie", "colour": "red"}"#);
        assert!(map.is_empty());
        assert_eq!(errors, vec!["Invalid keywords for spawn.json: colour"]);
    }
}
