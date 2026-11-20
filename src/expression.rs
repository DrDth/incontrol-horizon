//! `Tools.parseExpression`: `gt(5)`, `range(2,8)`, `repeat(24000,0,1000)` and the rest, a test of
//! one integer.

/// A parsed expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expression {
    Greater(i32),
    GreaterOrEqual(i32),
    Smaller(i32),
    SmallerOrEqual(i32),
    Equal(i32),
    NotEqual(i32),
    Range(i32, i32),
    OutsideRange(i32, i32),
    Repeat(i32, i32, i32),
}

impl Expression {
    pub fn test(self, value: i32) -> bool {
        match self {
            Expression::Greater(n) => value > n,
            Expression::GreaterOrEqual(n) => value >= n,
            Expression::Smaller(n) => value < n,
            Expression::SmallerOrEqual(n) => value <= n,
            Expression::Equal(n) => value == n,
            Expression::NotEqual(n) => value != n,
            Expression::Range(min, max) => value >= min && value <= max,
            Expression::OutsideRange(min, max) => value < min || value > max,
            Expression::Repeat(cycle, min, max) => {
                // Java's `%` keeps the dividend's sign, as Rust's does.
                let v = value.wrapping_rem(cycle);
                v >= min && v <= max
            }
        }
    }
}

/// `Integer.parseInt` of `s`, without the trimming Java doesn't do either.
fn int(s: &str) -> Result<i32, String> {
    s.parse::<i32>()
        .map_err(|_| format!("For input string: \"{s}\""))
}

/// The text between `from` and `to` (exclusive), as `substring` with `indexOf`s; Java throws when
/// they're out of order or missing.
fn between(s: &str, from: Option<usize>, to: Option<usize>) -> Result<&str, String> {
    match (from, to) {
        (Some(a), Some(b)) if a <= b => Ok(&s[a..b]),
        _ => Err(format!("Bad expression: {s}")),
    }
}

/// `Tools.parseExpression(input)`: Ok(None) for an expression it doesn't know (the original
/// reports it and adds no check), Err where Java throws while reading the numbers.
pub fn parse(input: &str) -> Result<Option<Expression>, String> {
    let i = input.to_lowercase();
    let open = i.find('(').map(|p| p + 1);
    let close = i.find(')');
    let comma = i.find(',');
    let single = || -> Result<i32, String> { int(between(&i, open, close)?.trim()) };
    let starts = |a: &str, b: &str| i.starts_with(a) || i.starts_with(b);
    Ok(Some(if starts("greater(", "gt(") {
        Expression::Greater(single()?)
    } else if starts("greaterorequal(", "ge(") {
        Expression::GreaterOrEqual(single()?)
    } else if starts("smaller(", "lt(") {
        Expression::Smaller(single()?)
    } else if starts("smallerorequal(", "le(") {
        Expression::SmallerOrEqual(single()?)
    } else if starts("equal(", "eq(") {
        Expression::Equal(single()?)
    } else if starts("notequal(", "ne(") {
        Expression::NotEqual(single()?)
    } else if i.starts_with("range(") || i.starts_with("outsiderange(") {
        let min = int(between(&i, open, comma)?)?;
        let max = int(between(&i, comma.map(|c| c + 1), close)?)?;
        if i.starts_with("range(") {
            Expression::Range(min, max)
        } else {
            Expression::OutsideRange(min, max)
        }
    } else if i.starts_with("repeat(") {
        let last = i.rfind(',');
        let cycle = int(between(&i, open, comma)?)?;
        let min = int(between(&i, comma.map(|c| c + 1), last)?)?;
        let max = int(between(&i, last.map(|c| c + 1), close)?)?;
        Expression::Repeat(cycle, min, max)
    } else {
        return Ok(None);
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions_test_as_the_original() {
        assert!(parse("gt(5)").unwrap().unwrap().test(6));
        assert!(!parse("GT( 5 )").unwrap().unwrap().test(5));
        assert!(parse("range(2,8)").unwrap().unwrap().test(8));
        assert!(parse("outsiderange(2,8)").unwrap().unwrap().test(1));
        let repeat = parse("repeat(10,2,3)").unwrap().unwrap();
        assert!(repeat.test(13) && !repeat.test(14));
        assert_eq!(parse("between(1,2)"), Ok(None));
        // `range( 2,8)`: `Integer.parseInt(" 2")` throws.
        assert!(parse("range( 2,8)").is_err());
    }
}
