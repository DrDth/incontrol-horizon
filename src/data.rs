//! `DataStorage`: the world's day counter, its phases and its numbers, kept in the save's
//! `horizon-mod-data/incontrol/` (the original's `data/InControlData.dat`).
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The file in the world's data folder for the mod.
pub const FILE: &str = "data.json";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataStorage {
    is_day: Option<bool>,
    day_counter: i32,
    phases: BTreeSet<String>,
    numbers: BTreeMap<String, i32>,
    /// Ticks until the phase rules are asked again.
    check_counter: i32,
    /// Changed since it was last written.
    pub dirty: bool,
}

impl DataStorage {
    /// The data as `save` wrote it; empty data for a missing or broken file.
    pub fn read(text: Option<&str>) -> DataStorage {
        let mut data = DataStorage::default();
        let Some(Ok(Value::Object(o))) = text.map(serde_json::from_str::<Value>) else {
            return data;
        };
        data.day_counter = o.get("daycounter").and_then(Value::as_i64).unwrap_or(0) as i32;
        data.is_day = o.get("isday").and_then(Value::as_bool);
        if let Some(Value::Array(phases)) = o.get("phases") {
            data.phases = phases
                .iter()
                .filter_map(|p| p.as_str().map(str::to_owned))
                .collect();
        }
        if let Some(Value::Object(numbers)) = o.get("numbers") {
            for (k, v) in numbers {
                data.numbers
                    .insert(k.clone(), v.as_i64().unwrap_or(0) as i32);
            }
        }
        data
    }

    /// `save`.
    pub fn write(&self) -> String {
        let mut o = Map::new();
        o.insert("daycounter".into(), json!(self.day_counter));
        if let Some(day) = self.is_day {
            o.insert("isday".into(), json!(day));
        }
        o.insert("phases".into(), json!(self.phases));
        o.insert("numbers".into(), json!(self.numbers));
        Value::Object(o).to_string()
    }

    pub fn day_counter(&self) -> i32 {
        self.day_counter
    }

    pub fn set_day_counter(&mut self, days: i32) {
        self.day_counter = days;
        self.dirty = true;
    }

    pub fn phases(&self) -> &BTreeSet<String> {
        &self.phases
    }

    pub fn numbers(&self) -> &BTreeMap<String, i32> {
        &self.numbers
    }

    pub fn number(&self, name: &str) -> i32 {
        self.numbers.get(name).copied().unwrap_or(0)
    }

    pub fn set_number(&mut self, name: &str, value: i32) {
        self.numbers.insert(name.to_owned(), value);
        self.dirty = true;
    }

    /// `setPhase`.
    pub fn set_phase(&mut self, phase: &str, on: bool) {
        let changed = if on {
            self.phases.insert(phase.to_owned())
        } else {
            self.phases.remove(phase)
        };
        self.dirty |= changed;
    }

    /// `tickTime`: a new day starts when the time of day passes 0 again.
    pub fn tick_time(&mut self, day_time: i64) {
        let time = day_time % 24000;
        let day = (0..12000).contains(&time);
        match self.is_day {
            None => {
                self.is_day = Some(day);
                self.dirty = true;
            }
            Some(was) if was != day => {
                if day {
                    self.day_counter += 1;
                }
                self.is_day = Some(day);
                self.dirty = true;
            }
            _ => {}
        }
    }

    /// `tick`'s check counter: whether the phase rules are due this tick.
    pub fn phases_due(&mut self) -> bool {
        self.check_counter -= 1;
        if self.check_counter <= 0 {
            self.check_counter = 10;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_count_each_new_morning_and_the_data_reads_back() {
        let mut data = DataStorage::default();
        data.tick_time(13000);
        assert_eq!(data.day_counter(), 0);
        data.tick_time(24000 + 5);
        assert_eq!(data.day_counter(), 1);
        data.tick_time(24000 + 100);
        assert_eq!(data.day_counter(), 1);
        data.set_phase("night", true);
        data.set_number("waves", 3);
        let back = DataStorage::read(Some(&data.write()));
        assert_eq!(back.day_counter(), 1);
        assert!(back.phases().contains("night"));
        assert_eq!(back.number("waves"), 3);
        assert_eq!(back.number("other"), 0);
    }
}
