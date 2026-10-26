//! Minimal Java-regex matcher for resource pack `filter` patterns.
//!
//! Vanilla compiles the `namespace`/`path` strings with
//! `Pattern.compile(...)` and tests them through `Pattern::asPredicate`, i.e.
//! an unanchored `find()`. This matcher supports the subset packs use in
//! practice: literals, `.`, `\d`/`\w`/`\s`, escaped literals, `[...]` classes
//! with ranges and `^` negation, `*`/`+`/`?` quantifiers, and `^`/`$` anchors.
//! Alternation and groups are treated as literals.
//!
//! Patterns are matched against individual path components exactly as vanilla
//! does: `namespace` against the namespace and `path` against the resource
//! path such as `blockstates/stone.json`.

#[derive(Debug, Clone)]
enum Atom {
    Literal(char),
    Any,
    Class {
        ranges: Vec<(char, char)>,
        negated: bool,
    },
    Start,
    End,
}

impl Atom {
    fn matches(&self, c: char) -> bool {
        match self {
            Atom::Literal(literal) => *literal == c,
            Atom::Any => true,
            Atom::Class { ranges, negated } => {
                let hit = ranges.iter().any(|(low, high)| (*low..=*high).contains(&c));
                hit != *negated
            }
            Atom::Start | Atom::End => false,
        }
    }
}

#[derive(Debug, Clone)]
struct Piece {
    atom: Atom,
    min: usize,
    max: Option<usize>,
}

fn digit() -> Atom {
    Atom::Class {
        ranges: vec![('0', '9')],
        negated: false,
    }
}

fn word() -> Atom {
    Atom::Class {
        ranges: vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')],
        negated: false,
    }
}

fn whitespace() -> Atom {
    Atom::Class {
        ranges: vec![('\t', '\r'), (' ', ' ')],
        negated: false,
    }
}

fn parse_class(chars: &[char], start: usize) -> (Atom, usize) {
    let mut index = start + 1;
    let negated = chars.get(index) == Some(&'^');
    if negated {
        index += 1;
    }
    let mut ranges = Vec::new();
    while index < chars.len() && chars[index] != ']' {
        let low = if chars[index] == '\\' && index + 1 < chars.len() {
            index += 1;
            chars[index]
        } else {
            chars[index]
        };
        index += 1;
        if chars.get(index) == Some(&'-') && chars.get(index + 1).is_some_and(|c| *c != ']') {
            index += 1;
            let high = if chars[index] == '\\' && index + 1 < chars.len() {
                index += 1;
                chars[index]
            } else {
                chars[index]
            };
            index += 1;
            ranges.push((low, high));
        } else {
            ranges.push((low, low));
        }
    }
    if index < chars.len() {
        index += 1;
    }
    (Atom::Class { ranges, negated }, index)
}

fn parse(pattern: &str) -> Vec<Piece> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut pieces = Vec::new();
    let mut index = 0;
    while let Some(&current) = chars.get(index) {
        let atom = match current {
            '^' => {
                index += 1;
                pieces.push(Piece {
                    atom: Atom::Start,
                    min: 1,
                    max: Some(1),
                });
                continue;
            }
            '$' => {
                index += 1;
                pieces.push(Piece {
                    atom: Atom::End,
                    min: 1,
                    max: Some(1),
                });
                continue;
            }
            '.' => {
                index += 1;
                Atom::Any
            }
            '\\' => {
                index += 1;
                let escaped = chars.get(index).copied().unwrap_or('\\');
                index += 1;
                match escaped {
                    'd' => digit(),
                    'w' => word(),
                    's' => whitespace(),
                    other => Atom::Literal(other),
                }
            }
            '[' => {
                let (atom, next) = parse_class(&chars, index);
                index = next;
                atom
            }
            other => {
                index += 1;
                Atom::Literal(other)
            }
        };
        let (min, max) = match chars.get(index) {
            Some('*') => {
                index += 1;
                (0, None)
            }
            Some('+') => {
                index += 1;
                (1, None)
            }
            Some('?') => {
                index += 1;
                (0, Some(1))
            }
            _ => (1, Some(1)),
        };
        pieces.push(Piece { atom, min, max });
    }
    pieces
}

fn matches_at(pieces: &[Piece], piece_index: usize, text: &[char], text_index: usize) -> bool {
    let Some(piece) = pieces.get(piece_index) else {
        return true;
    };
    match piece.atom {
        Atom::Start => {
            return text_index == 0 && matches_at(pieces, piece_index + 1, text, text_index);
        }
        Atom::End => {
            return text_index == text.len()
                && matches_at(pieces, piece_index + 1, text, text_index);
        }
        _ => {}
    }

    let mut count = 0;
    while text_index + count < text.len()
        && piece.max.is_none_or(|max| count < max)
        && piece.atom.matches(text[text_index + count])
    {
        count += 1;
    }
    if count < piece.min {
        return false;
    }
    loop {
        if matches_at(pieces, piece_index + 1, text, text_index + count) {
            return true;
        }
        if count == piece.min {
            return false;
        }
        count -= 1;
    }
}

/// Java `Pattern.matcher(input).find()` for the supported subset.
pub(crate) fn find(pattern: &str, input: &str) -> bool {
    let pieces = parse(pattern);
    let text: Vec<char> = input.chars().collect();
    let anchored = matches!(pieces.first().map(|piece| &piece.atom), Some(Atom::Start));
    if anchored {
        matches_at(&pieces, 0, &text, 0)
    } else {
        (0..=text.len()).any(|start| matches_at(&pieces, 0, &text, start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_filters_match_the_full_resource_path() {
        assert!(find(r".*_wall\.json", "blockstates/oak_wall.json"));
        assert!(!find(r".*_wall\.json", "blockstates/oak_fence.json"));
        assert!(find(r"models/block/.*", "models/block/oak_wall.json"));
    }

    #[test]
    fn find_is_unanchored() {
        assert!(find("stone", "blockstates/stone.json"));
        assert!(find("wall", "blockstates/oak_wall.json"));
        assert!(!find("wall", "blockstates/stone.json"));
    }

    #[test]
    fn dot_and_quantifiers() {
        assert!(find("a.c", "abc"));
        assert!(find("ab*c", "ac"));
        assert!(find("ab*c", "abbbc"));
        assert!(find("ab+c", "abbbc"));
        assert!(!find("ab+c", "ac"));
        assert!(find("ab?c", "ac"));
        assert!(!find("ab?c", "abbc"));
    }

    #[test]
    fn character_classes() {
        assert!(find(r"[a-c]+\.json", "abc.json"));
        assert!(find(r"[^a]", "aab"));
        assert!(!find(r"[^a]", "aaa"));
        assert!(find(r"\d+", "chunk12"));
        assert!(!find(r"\d", "chunk"));
        assert!(find(r"\w+", "oak_wall"));
        assert!(find(r"\s", "a b"));
    }

    #[test]
    fn anchors_and_escapes() {
        assert!(find("^minecraft$", "minecraft"));
        assert!(!find("^craft$", "minecraft"));
        assert!(find(r"\.json$", "blockstates/stone.json"));
        assert!(find(r"a\.b", "a.b"));
        assert!(!find(r"a\.b", "axb"));
    }

    #[test]
    fn unsupported_constructs_are_literals() {
        assert!(!find("stone|dirt", "stone"));
        assert!(find("stone|dirt", "a stone|dirt b"));
    }
}
