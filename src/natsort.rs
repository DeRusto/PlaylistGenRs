//! Natural sort keys, matching Python's `_natural_key`: lowercase, split on digit runs,
//! digit runs compare numerically.

use std::cmp::Ordering;
use std::path::{Component, Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Str(String),
    /// Digit run with leading zeros stripped, so "01" == "1" like Python's `int()`.
    Num(String),
}

impl Ord for Token {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Token::Str(a), Token::Str(b)) => a.cmp(b),
            // No overflow: compare by length first, then lexically.
            (Token::Num(a), Token::Num(b)) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
            // Unreachable for keys built here: tokens always alternate Str, Num, Str, ...
            (Token::Str(_), Token::Num(_)) => Ordering::Greater,
            (Token::Num(_), Token::Str(_)) => Ordering::Less,
        }
    }
}

impl PartialOrd for Token {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Python `re.split(r"(\d+)", s.lower())` with digit runs as numbers.
/// Always starts and ends with a `Str` (possibly empty), alternating with `Num`.
///
/// shortcut: only ASCII digits count as digits (Python's `\d` also matches other Unicode
/// decimal digits); upgrade if media names use non-ASCII numerals.
pub fn natural_key(s: &str) -> Vec<Token> {
    let lower = s.to_lowercase();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_num = false;
    for c in lower.chars() {
        let is_digit = c.is_ascii_digit();
        if is_digit != in_num {
            out.push(token(std::mem::take(&mut cur), in_num));
            in_num = is_digit;
        }
        cur.push(c);
    }
    out.push(token(cur, in_num));
    if in_num {
        out.push(Token::Str(String::new()));
    }
    out
}

fn token(s: String, num: bool) -> Token {
    if num {
        let t = s.trim_start_matches('0');
        Token::Num(if t.is_empty() { "0".into() } else { t.into() })
    } else {
        Token::Str(s)
    }
}

/// Natural key of a path's file name (Python `natural_sort_key`).
pub fn name_key(path: &Path) -> Vec<Token> {
    natural_key(&path.file_name().unwrap_or_default().to_string_lossy())
}

/// Sort key for `f` relative to `base`.
///
/// Deliberate deviation from Python's `_path_sort_key`: Python flattens every component's
/// tokens into one list, which can misalign str/int positions (and raise `TypeError`).
/// Here components are compared one by one, each by natural key. For normal layouts the
/// resulting order is identical.
pub fn path_key(f: &Path, base: &Path) -> Vec<Vec<Token>> {
    f.strip_prefix(base)
        .unwrap_or(f)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(natural_key(&s.to_string_lossy())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        v.sort_by_key(|s| natural_key(s));
        v
    }

    #[test]
    fn numeric_order() {
        assert_eq!(
            sorted(&["ep10.mkv", "ep2.mkv", "ep1.mkv"]),
            ["ep1.mkv", "ep2.mkv", "ep10.mkv"]
        );
    }

    #[test]
    fn seasons_and_case() {
        assert_eq!(
            sorted(&["Season 10", "season 2", "Season 1"]),
            ["Season 1", "season 2", "Season 10"]
        );
    }

    #[test]
    fn leading_digits_and_zeros() {
        assert_eq!(
            natural_key("10 Foo"),
            vec![
                Token::Str("".into()),
                Token::Num("10".into()),
                Token::Str(" foo".into())
            ]
        );
        assert_eq!(natural_key("ep01"), natural_key("ep1"));
        assert_eq!(natural_key("0"), natural_key("000"));
    }

    #[test]
    fn huge_numbers_dont_overflow() {
        let big = "9".repeat(60);
        assert_eq!(
            sorted(&[&format!("a{big}"), "a10"]),
            ["a10".to_string(), format!("a{big}")]
        );
    }

    #[test]
    fn path_key_component_wise() {
        let base = Path::new("/m/Show");
        let mut v = vec![
            Path::new("/m/Show/Season 10/e1.mkv"),
            Path::new("/m/Show/Season 2/e10.mkv"),
            Path::new("/m/Show/Season 2/e9.mkv"),
        ];
        v.sort_by_key(|p| path_key(p, base));
        assert_eq!(
            v,
            [
                Path::new("/m/Show/Season 2/e9.mkv"),
                Path::new("/m/Show/Season 2/e10.mkv"),
                Path::new("/m/Show/Season 10/e1.mkv"),
            ]
        );
    }
}
