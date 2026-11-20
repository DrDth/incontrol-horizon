//! `ModCommands`: `/incontrol` and its redirect `/ctrl`.
//!
//! Horizon takes a command as usages, not as Brigadier nodes with redirects, so `ctrl` gets the
//! same usages under its own word.
use mod_api::{
    Command, CommandArgumentValue, CommandInvocation, CommandSource, LevelRef, PlayerRef,
    ServerHost, Text,
};

/// The command and its redirect.
const ROOTS: [&str; 2] = ["incontrol", "ctrl"];

/// Every usage, with the permission level each subcommand requires.
pub fn usages() -> Vec<Command> {
    let rest: [(&str, u8); 12] = [
        ("kill <type:word>", 2),
        ("reload", 1),
        ("showmobs", 0),
        ("showstats", 0),
        ("clearstats", 0),
        ("list", 2),
        ("days [number:integer]", 2),
        ("phases", 2),
        ("numbers", 2),
        ("setphase <phase:word>", 2),
        ("setnumber <number:word> <value:integer>", 2),
        ("clearphase <phase:word>", 2),
    ];
    ROOTS
        .iter()
        .flat_map(|root| {
            rest.iter().map(move |(usage, level)| {
                let usage = format!("{root} {usage}");
                if *level == 0 {
                    Command::new(&usage)
                } else {
                    Command::op(&usage, *level)
                }
            })
        })
        .collect()
}

/// What a parsed `/incontrol` asks for.
pub enum Run {
    Kill(String),
    Reload,
    ShowMobs,
    ShowStats,
    ClearStats,
    List,
    /// `days`: shows the day counter, or sets it.
    Days(Option<i32>),
    Phases,
    Numbers,
    /// `setphase` (true) and `clearphase` (false).
    SetPhase(String, bool),
    SetNumber(String, i32),
}

/// What `command` asks for, None when it is no `/incontrol` usage.
pub fn parse(command: &CommandInvocation) -> Option<Run> {
    let literals: Vec<&str> = command.literals.iter().map(String::as_str).collect();
    let [root, rest @ ..] = literals.as_slice() else {
        return None;
    };
    if !ROOTS.contains(root) {
        return None;
    }
    Some(match rest {
        ["kill"] => Run::Kill(match command.argument("type")? {
            CommandArgumentValue::String(s) => s.clone(),
            _ => return None,
        }),
        ["reload"] => Run::Reload,
        ["showmobs"] => Run::ShowMobs,
        ["showstats"] => Run::ShowStats,
        ["clearstats"] => Run::ClearStats,
        ["list"] => Run::List,
        ["days"] => Run::Days(integer(command, "number")),
        ["phases"] => Run::Phases,
        ["numbers"] => Run::Numbers,
        ["setphase"] => Run::SetPhase(word(command, "phase")?, true),
        ["clearphase"] => Run::SetPhase(word(command, "phase")?, false),
        ["setnumber"] => Run::SetNumber(word(command, "number")?, integer(command, "value")?),
        _ => return None,
    })
}

/// The word argument `name`.
fn word(command: &CommandInvocation, name: &str) -> Option<String> {
    match command.argument(name)? {
        CommandArgumentValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// The integer argument `name`.
fn integer(command: &CommandInvocation, name: &str) -> Option<i32> {
    match command.argument(name)? {
        CommandArgumentValue::Integer(i) => Some(*i),
        _ => None,
    }
}

/// A line to the command's source.
pub fn send(host: &mut dyn ServerHost, line: Text) {
    let _ = host.commands().feedback(line);
}

/// A line in a legacy colour (`ChatFormatting` in front of a literal, as the original writes it).
pub fn colored(code: char, text: &str) -> Text {
    Text::literal(format!("\u{a7}{code}{text}"))
}

/// `getPlayerOrException`: the player who ran the command, or the error Brigadier answers.
pub fn player(host: &mut dyn ServerHost, source: &CommandSource) -> Option<PlayerRef> {
    if let CommandSource::Player { player, .. } = source {
        return Some(*player);
    }
    let line = Text::literal("")
        .append(Text::translate("permissions.requires.player", Vec::new()))
        .with_style({
            let mut style = mod_api::text::TextStyle::default();
            style.color = Some("red".to_owned());
            style
        });
    send(host, line);
    None
}

/// The player's level.
pub fn level_of(host: &dyn ServerHost, player: PlayerRef) -> Option<LevelRef> {
    player.entity().level(host)
}

/// `CmdList`: how many mobs of each type the player's level has.
pub fn list(host: &mut dyn ServerHost, level: LevelRef) {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for entity in level.all_entities(host).unwrap_or_default() {
        if host.mob(entity).is_none() {
            continue;
        }
        let Some(kind) = entity.kind(host).map(|k| k.id(host)) else {
            continue;
        };
        match counts.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => counts.push((kind, 1)),
        }
    }
    for (kind, n) in counts {
        send(host, colored('e', &format!("Mob {kind}: {n}")));
    }
}

/// `CmdKillMobs`: removes the entities of `kind` (an entity type id, or `all`) from the player's
/// level.
pub fn kill(host: &mut dyn ServerHost, level: LevelRef, kind: &str) {
    if matches!(kind, "passive" | "hostile" | "entity") {
        // These need `instanceof Animal` and `Enemy`, which Horizon's mod API doesn't answer yet.
        send(
            host,
            colored(
                'c',
                &format!("InControl for Horizon can't tell '{kind}' mobs apart yet"),
            ),
        );
        return;
    }
    let all = kind == "all";
    let mut removed = 0;
    for entity in level.all_entities(host).unwrap_or_default() {
        let id = entity.kind(host).map(|k| k.id(host)).unwrap_or_default();
        let take = if all {
            id != "minecraft:player"
        } else {
            id == kind
        };
        if take {
            entity.discard(host);
            removed += 1;
        }
    }
    send(host, colored('e', &format!("Removed {removed} entities!")));
}
