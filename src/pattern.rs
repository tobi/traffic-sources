//! Linear-time regular expressions for the rule files.
//!
//! A Thompson-NFA simulation (Pike VM) with leftmost-first priority, so results agree with
//! backtracking engines while every search runs in `O(pattern × haystack)` and cannot be driven
//! into catastrophic backtracking by a hostile User-Agent. Most rules are literal alternations;
//! those are answered by substring tests without running the VM at all, and the rest are skipped
//! unless a literal that every match must contain is present.
//!
//! Supported syntax: literals, `.`, `[...]` / `[^...]` with ranges, `\d \D \w \W \s \S`, `\b \B`,
//! `^ $ \A \z`, `( )`, `(?: )`, `(?P<name> )`, `|`, `* + ? {n} {n,} {n,m}` with lazy `?`
//! variants, `\n \t \r \f \v \xHH \x{H…}`, escaped punctuation, and the flags `i` and `s` as
//! `(?flags)` or `(?flags: … )`. Anything else (lookaround, backreferences, Unicode classes,
//! nested class sets, multi-line mode) is a compile error rather than a silent misreading.
//!
//! `\w`, `\b` and `\s` treat non-ASCII letters, digits and whitespace as word and space
//! characters, so `\bGooglebot\b` does not match inside `éGooglebot`. `\d` is `[0-9]` only, and
//! `(?i)` folds only `A-Z`/`a-z`. Every rule is ASCII, and User-Agents and host names are ASCII on
//! the wire, so those two simplifications do not change any result in practice.

use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;

/// Counted repetitions are expanded, so bound them to keep programs small.
const MAX_REPEAT: u32 = 1000;
const MAX_INSTS: usize = 100_000;
const MAX_DEPTH: usize = 64;
/// Upper bound on literal sets derived for the fast paths.
const MAX_LITERALS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Error {
    pub(crate) message: String,
    pub(crate) pattern: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "regex {:?}: {}", self.pattern, self.message)
    }
}

impl std::error::Error for Error {}

// ---------------------------------------------------------------- syntax tree

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Start,
    End,
    WordBoundary,
    NotWordBoundary,
}

/// A set of characters: explicit ranges, optionally united with the word or whitespace
/// predicates, optionally negated as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Class {
    /// Sorted, non-overlapping, non-adjacent inclusive ranges of code points.
    ranges: Vec<(u32, u32)>,
    word: bool,
    space: bool,
    negated: bool,
}

/// `\w`: ASCII word characters, and letters and digits beyond ASCII.
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || (!c.is_ascii() && c.is_alphanumeric())
}

impl Class {
    fn new(mut r: Vec<(u32, u32)>) -> Self {
        r.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(r.len());
        for (a, b) in r {
            match out.last_mut() {
                Some(last) if a <= last.1.saturating_add(1) => last.1 = last.1.max(b),
                _ => out.push((a, b)),
            }
        }
        Class {
            ranges: out,
            word: false,
            space: false,
            negated: false,
        }
    }
    fn negate(mut self) -> Self {
        self.negated = !self.negated;
        self
    }
    /// Add the other ASCII case of every ASCII letter in the ranges.
    fn fold(mut self) -> Self {
        let mut r = self.ranges.clone();
        for &(a, b) in &self.ranges {
            for (lo, hi, shift) in [(0x61u32, 0x7Au32, -32i32), (0x41, 0x5A, 32)] {
                let (s, e) = (a.max(lo), b.min(hi));
                if s <= e {
                    r.push(((s as i32 + shift) as u32, (e as i32 + shift) as u32));
                }
            }
        }
        self.ranges = Class::new(r).ranges;
        self
    }
    fn contains(&self, c: char) -> bool {
        let n = c as u32;
        let in_ranges = self
            .ranges
            .binary_search_by(|&(a, b)| {
                if b < n {
                    std::cmp::Ordering::Less
                } else if a > n {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok();
        let hit = in_ranges || (self.word && is_word_char(c)) || (self.space && c.is_whitespace());
        hit != self.negated
    }
    /// The members, if there are only a few.
    fn members(&self, limit: usize) -> Option<Vec<char>> {
        if self.word || self.space || self.negated {
            return None;
        }
        let mut out = Vec::new();
        for &(a, b) in &self.ranges {
            if (b - a) as usize >= limit || out.len() + (b - a) as usize >= limit {
                return None;
            }
            out.extend((a..=b).filter_map(char::from_u32));
        }
        Some(out)
    }
    /// True if every ASCII letter in the class is present in both cases.
    fn case_closed(&self) -> bool {
        (b'a'..=b'z')
            .all(|l| self.contains(l as char) == self.contains(l.to_ascii_uppercase() as char))
    }
}

/// `\d`, `\w` or `\s`. Digits are ASCII only, so a captured version number is always ASCII;
/// word and whitespace characters follow Unicode, as `\b` must agree with `\w`.
fn named_class(name: char) -> Class {
    match name {
        'd' => Class::new(vec![(0x30, 0x39)]),
        'w' => Class {
            word: true,
            ..Class::new(vec![(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)])
        },
        's' => Class {
            space: true,
            ..Class::new(vec![(0x09, 0x0D), (0x20, 0x20)])
        },
        _ => unreachable!("only d, w and s are named classes"),
    }
}

#[derive(Debug, Clone)]
enum Node {
    Empty,
    /// A literal, with whether it was written under `(?i)`.
    Lit(char, bool),
    Class(Class),
    Any {
        newline: bool,
    },
    Look(Look),
    Group(Box<Node>, Option<usize>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
}

#[derive(Debug, Clone, Copy, Default)]
struct Flags {
    ci: bool,
    dot_nl: bool,
}

struct Parser<'a> {
    src: &'a str,
    c: Vec<char>,
    i: usize,
    caps: usize,
}

impl Parser<'_> {
    fn err<T>(&self, m: impl Into<String>) -> Result<T, Error> {
        Err(Error {
            message: m.into(),
            pattern: self.src.to_string(),
        })
    }
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn alternation(&mut self, mut flags: Flags, depth: usize) -> Result<Node, Error> {
        if depth > MAX_DEPTH {
            return self.err("nesting too deep");
        }
        let mut branches = vec![self.concat(&mut flags, depth)?];
        while self.eat('|') {
            branches.push(self.concat(&mut flags, depth)?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().expect("one branch")
        } else {
            Node::Alt(branches)
        })
    }

    fn concat(&mut self, flags: &mut Flags, depth: usize) -> Result<Node, Error> {
        let mut items = Vec::new();
        while let Some(ch) = self.peek() {
            if ch == '|' || ch == ')' {
                break;
            }
            let Some(atom) = self.atom(flags, depth)? else {
                continue;
            };
            let atom = self.quantified(atom)?;
            items.push(atom);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().expect("one item"),
            _ => Node::Concat(items),
        })
    }

    fn quantified(&mut self, atom: Node) -> Result<Node, Error> {
        let (min, max) = match self.peek() {
            Some('*') => (0, None),
            Some('+') => (1, None),
            Some('?') => (0, Some(1)),
            Some('{') => {
                self.i += 1;
                let (min, max) = self.counted()?;
                self.i -= 1;
                (min, max)
            }
            _ => return Ok(atom),
        };
        self.i += 1;
        let greedy = !self.eat('?');
        if matches!(self.peek(), Some('*' | '+' | '?' | '{')) {
            return self.err("repetition of a repetition");
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        })
    }

    /// Parses `n}`, `n,}` or `n,m}` after the `{`; leaves `i` just past the `}`.
    fn counted(&mut self) -> Result<(u32, Option<u32>), Error> {
        let num = |p: &mut Self| -> Result<Option<u32>, Error> {
            let s = p.i;
            while p.peek().is_some_and(|c| c.is_ascii_digit()) {
                p.i += 1;
            }
            if s == p.i {
                return Ok(None);
            }
            let n: String = p.c[s..p.i].iter().collect();
            match n.parse::<u32>() {
                Ok(n) if n <= MAX_REPEAT => Ok(Some(n)),
                _ => p.err(format!("repetition count above {MAX_REPEAT}")),
            }
        };
        let Some(min) = num(self)? else {
            return self.err("invalid counted repetition");
        };
        let max = if self.eat(',') { num(self)? } else { Some(min) };
        if !self.eat('}') {
            return self.err("unclosed counted repetition");
        }
        if max.is_some_and(|m| m < min) {
            return self.err("repetition range is backwards");
        }
        Ok((min, max))
    }

    /// One atom, or `None` for a bare flag group such as `(?i)`.
    fn atom(&mut self, flags: &mut Flags, depth: usize) -> Result<Option<Node>, Error> {
        let ch = self.peek().expect("caller checked");
        self.i += 1;
        Ok(Some(match ch {
            '(' => return self.group(flags, depth),
            '[' => Node::Class(self.class(*flags)?),
            '.' => Node::Any {
                newline: flags.dot_nl,
            },
            '^' => Node::Look(Look::Start),
            '$' => Node::Look(Look::End),
            '\\' => self.escape(*flags)?,
            '*' | '+' | '?' | '{' => return self.err("repetition operator missing expression"),
            c => Node::Lit(c, flags.ci),
        }))
    }

    fn group(&mut self, flags: &mut Flags, depth: usize) -> Result<Option<Node>, Error> {
        let mut inner = *flags;
        let mut cap = None;
        if self.eat('?') {
            if self.eat('P') || self.peek() == Some('<') {
                if !self.eat('<') {
                    return self.err("expected `<` after `(?P`");
                }
                let s = self.i;
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    self.i += 1;
                }
                if s == self.i || !self.eat('>') {
                    return self.err("invalid group name");
                }
                self.caps += 1;
                cap = Some(self.caps);
            } else {
                let mut on = true;
                let mut any = false;
                loop {
                    match self.peek() {
                        Some('i') => inner.ci = on,
                        Some('s') => inner.dot_nl = on,
                        Some('-') if on => on = false,
                        Some(':') => {
                            self.i += 1;
                            break;
                        }
                        Some(')') if any => {
                            self.i += 1;
                            *flags = inner;
                            return Ok(None);
                        }
                        Some(c) => {
                            return self.err(format!("unsupported group or flag `(?{c}`"));
                        }
                        None => return self.err("unclosed group"),
                    }
                    any = true;
                    self.i += 1;
                }
            }
        } else {
            self.caps += 1;
            cap = Some(self.caps);
        }
        let node = self.alternation(inner, depth + 1)?;
        if !self.eat(')') {
            return self.err("unclosed group");
        }
        Ok(Some(Node::Group(Box::new(node), cap)))
    }

    fn escape(&mut self, flags: Flags) -> Result<Node, Error> {
        let Some(c) = self.peek() else {
            return self.err("trailing backslash");
        };
        self.i += 1;
        Ok(match c {
            'd' | 'w' | 's' => Node::Class(named_class(c)),
            'D' | 'W' | 'S' => Node::Class(named_class(c.to_ascii_lowercase()).negate()),
            'b' => Node::Look(Look::WordBoundary),
            'B' => Node::Look(Look::NotWordBoundary),
            'A' => Node::Look(Look::Start),
            'z' => Node::Look(Look::End),
            _ => Node::Lit(self.escaped_char(c)?, flags.ci),
        })
    }

    /// The literal named by `\c` (the backslash and `c` already consumed).
    fn escaped_char(&mut self, c: char) -> Result<char, Error> {
        Ok(match c {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'f' => '\u{0c}',
            'v' => '\u{0b}',
            'x' => {
                let digits: String = if self.eat('{') {
                    let s = self.i;
                    while self.peek().is_some_and(|c| c != '}') {
                        self.i += 1;
                    }
                    let d = self.c[s..self.i].iter().collect();
                    if !self.eat('}') {
                        return self.err("unclosed \\x{");
                    }
                    d
                } else {
                    let d: String = self.c.iter().skip(self.i).take(2).collect();
                    self.i += d.chars().count();
                    if d.len() != 2 {
                        return self.err("\\x needs two hex digits");
                    }
                    d
                };
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(c) if !digits.is_empty() && digits.len() <= 8 => c,
                    _ => return self.err("invalid \\x escape"),
                }
            }
            // `\<` and `\>` are word-boundary assertions in some dialects; refuse rather than guess.
            '<' | '>' => return self.err(format!("unsupported escape \\{c}")),
            c if c.is_ascii_punctuation() => c,
            c => return self.err(format!("unsupported escape \\{c}")),
        })
    }

    fn class(&mut self, flags: Flags) -> Result<Class, Error> {
        let negated = self.eat('^');
        let mut r: Vec<(u32, u32)> = Vec::new();
        let (mut word, mut space) = (false, false);
        let mut first = true;
        loop {
            let Some(c) = self.peek() else {
                return self.err("unclosed character class");
            };
            self.i += 1;
            if c == ']' && !first {
                break;
            }
            first = false;
            if c == '[' {
                return self.err("nested character classes are not supported");
            }
            if matches!(c, '&' | '~' | '-') && self.peek() == Some(c) {
                return self.err("character class set operations are not supported");
            }
            let lo = if c == '\\' {
                let Some(e) = self.peek() else {
                    return self.err("trailing backslash");
                };
                self.i += 1;
                match e {
                    'd' | 'w' | 's' => {
                        let n = named_class(e);
                        r.extend(n.ranges);
                        word |= n.word;
                        space |= n.space;
                        continue;
                    }
                    'D' | 'W' | 'S' => {
                        return self.err(format!("\\{e} inside [...] is not supported"));
                    }
                    _ => self.escaped_char(e)?,
                }
            } else {
                c
            };
            // A range, unless the `-` is the last thing before `]`.
            if self.peek() == Some('-') && self.c.get(self.i + 1).is_some_and(|&n| n != ']') {
                self.i += 1;
                let hc = self.peek().expect("checked above");
                self.i += 1;
                let hi = if hc == '\\' {
                    let Some(e) = self.peek() else {
                        return self.err("trailing backslash");
                    };
                    self.i += 1;
                    if matches!(e, 'd' | 'w' | 's' | 'D' | 'W' | 'S') {
                        return self.err("invalid range endpoint");
                    }
                    self.escaped_char(e)?
                } else {
                    hc
                };
                if hi < lo {
                    return self.err("character class range is backwards");
                }
                r.push((lo as u32, hi as u32));
            } else {
                r.push((lo as u32, lo as u32));
            }
        }
        let mut class = Class {
            word,
            space,
            ..Class::new(r)
        };
        if flags.ci {
            class = class.fold();
        }
        Ok(if negated { class.negate() } else { class })
    }
}

// ---------------------------------------------------------------- literal analysis

/// How the strings of a literal set must be compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Case {
    /// No cased letters, or letters that are present in both cases.
    Neutral,
    Insensitive,
    Sensitive,
    Mixed,
}

impl Case {
    fn join(self, o: Case) -> Case {
        match (self, o) {
            (Case::Neutral, x) | (x, Case::Neutral) => x,
            (a, b) if a == b => a,
            _ => Case::Mixed,
        }
    }
}

/// The complete, finite set of strings a node can match, when it is small.
fn exact(n: &Node) -> Option<(Vec<String>, Case)> {
    match n {
        Node::Empty => Some((vec![String::new()], Case::Neutral)),
        Node::Lit(c, ci) => {
            let case = match (c.is_ascii_alphabetic(), ci) {
                (false, _) => Case::Neutral,
                (true, true) => Case::Insensitive,
                (true, false) => Case::Sensitive,
            };
            Some((vec![c.to_string()], case))
        }
        Node::Class(c) => {
            let m = c.members(8)?;
            let case = if c.case_closed() {
                Case::Neutral
            } else {
                Case::Sensitive
            };
            Some((m.into_iter().map(String::from).collect(), case))
        }
        Node::Any { .. } | Node::Look(_) => None,
        Node::Group(n, _) => exact(n),
        Node::Concat(items) => {
            let mut acc = (vec![String::new()], Case::Neutral);
            for it in items {
                acc = product(acc, exact(it)?)?;
            }
            Some(acc)
        }
        Node::Alt(bs) => {
            let mut set = Vec::new();
            let mut case = Case::Neutral;
            for b in bs {
                let (s, c) = exact(b)?;
                set.extend(s);
                case = case.join(c);
                if set.len() > MAX_LITERALS {
                    return None;
                }
            }
            Some((set, case))
        }
        Node::Repeat { node, min, max, .. } => {
            let max = (*max)?;
            if max > 4 {
                return None;
            }
            let one = exact(node)?;
            let mut out: Vec<String> = Vec::new();
            let mut case = one.1;
            let mut k = (vec![String::new()], Case::Neutral);
            for i in 0..=max {
                if i >= *min {
                    out.extend(k.0.iter().cloned());
                    case = case.join(k.1);
                }
                if i < max {
                    k = product(k, one.clone())?;
                }
            }
            (out.len() <= MAX_LITERALS).then_some((out, case))
        }
    }
}

fn product(a: (Vec<String>, Case), b: (Vec<String>, Case)) -> Option<(Vec<String>, Case)> {
    if a.0.len() * b.0.len() > MAX_LITERALS {
        return None;
    }
    let mut out = Vec::with_capacity(a.0.len() * b.0.len());
    for x in &a.0 {
        for y in &b.0 {
            out.push(format!("{x}{y}"));
        }
    }
    Some((out, a.1.join(b.1)))
}

/// Lowercased strings, one of which every match must contain. Comparing case-insensitively is a
/// sound over-approximation even for case-sensitive parts.
fn required(n: &Node) -> Option<Vec<String>> {
    let lower = |s: Vec<String>| -> Option<Vec<String>> {
        let mut v: Vec<String> = s.iter().map(|x| x.to_ascii_lowercase()).collect();
        v.sort();
        v.dedup();
        (!v.iter().any(String::is_empty)).then_some(v)
    };
    if let Some((s, _)) = exact(n) {
        return lower(s);
    }
    match n {
        Node::Group(n, _) => required(n),
        Node::Repeat { node, min, .. } if *min >= 1 => required(node),
        Node::Alt(bs) => {
            let mut set = Vec::new();
            for b in bs {
                set.extend(required(b)?);
            }
            set.sort();
            set.dedup();
            (set.len() <= MAX_LITERALS).then_some(set)
        }
        Node::Concat(items) => {
            let mut candidates: Vec<Vec<String>> = Vec::new();
            let mut run: Option<(Vec<String>, Case)> = None;
            for it in items {
                match exact(it) {
                    Some(e) => {
                        run = match run.take() {
                            None => Some(e),
                            Some(r) => match product(r.clone(), e.clone()) {
                                Some(p) => Some(p),
                                None => {
                                    candidates.extend(lower(r.0));
                                    Some(e)
                                }
                            },
                        }
                    }
                    None => {
                        if let Some(r) = run.take() {
                            candidates.extend(lower(r.0));
                        }
                        candidates.extend(required(it));
                    }
                }
            }
            if let Some(r) = run {
                candidates.extend(lower(r.0));
            }
            candidates.into_iter().max_by_key(|set| {
                let shortest = set.iter().map(String::len).min().unwrap_or(0);
                (shortest, usize::MAX - set.len())
            })
        }
        _ => None,
    }
}

/// The strings one of which every match starts with (`lead`) or ends with (`trail`). A set
/// containing `""` carries no information.
fn edge(n: &Node, from_end: bool) -> Option<Vec<String>> {
    let empty = || vec![String::new()];
    if let Some((s, _)) = exact(n) {
        return Some(s);
    }
    match n {
        Node::Look(_) => Some(empty()),
        Node::Group(n, _) => edge(n, from_end),
        Node::Repeat { node, min, .. } if *min >= 1 => edge(node, from_end),
        Node::Alt(bs) => {
            let mut set = Vec::new();
            for b in bs {
                set.extend(edge(b, from_end)?);
            }
            (set.len() <= MAX_LITERALS).then_some(set)
        }
        Node::Concat(items) => {
            let mut acc = empty();
            let ordered: Vec<&Node> = if from_end {
                items.iter().rev().collect()
            } else {
                items.iter().collect()
            };
            for it in ordered {
                let (next, last) = match exact(it) {
                    Some((e, _)) => (e, false),
                    None if matches!(it, Node::Look(_)) => (empty(), false),
                    None => match edge(it, from_end) {
                        Some(e) => (e, true),
                        None => return Some(acc),
                    },
                };
                let joined = if from_end {
                    product((next, Case::Neutral), (acc.clone(), Case::Neutral))
                } else {
                    product((acc.clone(), Case::Neutral), (next, Case::Neutral))
                };
                match joined {
                    Some((p, _)) => acc = p,
                    None => return Some(acc),
                }
                if last {
                    return Some(acc);
                }
            }
            Some(acc)
        }
        _ => None,
    }
}

/// True if every match is pinned to the start (`from_end = false`) or end of the haystack.
fn anchored(n: &Node, from_end: bool) -> bool {
    let want = if from_end { Look::End } else { Look::Start };
    match n {
        Node::Look(l) => *l == want,
        Node::Group(n, _) => anchored(n, from_end),
        Node::Alt(bs) => bs.iter().all(|b| anchored(b, from_end)),
        Node::Concat(items) => {
            let edge_item = if from_end {
                items.last()
            } else {
                items.first()
            };
            edge_item.is_some_and(|it| anchored(it, from_end))
        }
        _ => false,
    }
}

/// Lowercase, deduplicated, and `None` if the set says nothing.
fn useful(set: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut v: Vec<String> = set?.iter().map(|x| x.to_ascii_lowercase()).collect();
    v.sort();
    v.dedup();
    (!v.is_empty() && !v.iter().any(String::is_empty)).then_some(v)
}

/// Necessary conditions checked before running the VM. All strings are lowercase and compared
/// against the lowercased haystack, which over-approximates case-sensitive parts soundly.
#[derive(Debug, Clone, Default)]
struct Prefilter {
    required: Option<Vec<String>>,
    prefix: Option<Vec<String>>,
    suffix: Option<Vec<String>>,
}

impl Prefilter {
    fn new(ast: &Node) -> Self {
        Prefilter {
            required: required(ast),
            prefix: anchored(ast, false)
                .then(|| useful(edge(ast, false)))
                .flatten(),
            suffix: anchored(ast, true)
                .then(|| useful(edge(ast, true)))
                .flatten(),
        }
    }
    fn may_match(&self, h: &Haystack) -> bool {
        let l = h.lower.as_ref();
        self.prefix
            .as_ref()
            .is_none_or(|p| p.iter().any(|s| l.starts_with(s.as_str())))
            && self
                .suffix
                .as_ref()
                .is_none_or(|p| p.iter().any(|s| l.ends_with(s.as_str())))
            && self
                .required
                .as_ref()
                .is_none_or(|r| r.iter().any(|s| l.contains(s.as_str())))
    }
}

/// A haystack together with its ASCII-lowercased form, computed once and shared by every pattern
/// tested against it. Lowercasing ASCII keeps byte offsets unchanged.
pub(crate) struct Haystack<'h> {
    raw: &'h str,
    lower: Cow<'h, str>,
}

impl<'h> Haystack<'h> {
    pub(crate) fn new(raw: &'h str) -> Self {
        let lower = if raw.bytes().any(|b| b.is_ascii_uppercase()) {
            Cow::Owned(raw.to_ascii_lowercase())
        } else {
            Cow::Borrowed(raw)
        };
        Haystack { raw, lower }
    }
}

/// One pass over a haystack that tells which of many patterns could match it.
///
/// Every pattern with a required-literal prefilter is indexed by those literals in an
/// Aho–Corasick automaton over lowercase bytes; patterns without one are always candidates. A
/// pattern the screen rules out would also have been ruled out by its own prefilter, so testing
/// only the candidates, in index order, gives exactly the same answers as testing every pattern.
#[derive(Debug, Clone, Default)]
pub(crate) struct Screen {
    /// Byte to equivalence class; class 0 is every byte that occurs in no literal.
    classes: Vec<u16>,
    stride: usize,
    /// `delta[state * stride + class]` is the next state (a full DFA: no failure links at scan).
    delta: Vec<u32>,
    /// Patterns reported on entering each state: `hits[ends[s]..ends[s + 1]]`.
    ends: Vec<u32>,
    hits: Vec<u32>,
    always: Vec<u32>,
    len: usize,
}

/// The patterns that survived a [`Screen`], as a bitset in index order.
pub(crate) struct Candidates(Vec<u64>);

impl Candidates {
    pub(crate) fn into_indices(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(w, bits)| {
            let mut bits = bits;
            std::iter::from_fn(move || {
                (bits != 0).then(|| {
                    let b = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    w * 64 + b
                })
            })
        })
    }
}

impl Screen {
    pub(crate) fn new<'a>(patterns: impl IntoIterator<Item = &'a Regex>) -> Self {
        let mut lits: Vec<(&'a [u8], u32)> = Vec::new();
        let mut always = Vec::new();
        let mut len = 0;
        for (i, rx) in patterns.into_iter().enumerate() {
            len = i + 1;
            match &rx.pre.required {
                Some(req) if !req.iter().any(String::is_empty) => {
                    lits.extend(req.iter().map(|s| (s.as_bytes(), i as u32)));
                }
                _ => always.push(i as u32),
            }
        }
        let mut classes = vec![0u16; 256];
        let mut stride = 1;
        for &(l, _) in &lits {
            for &b in l {
                if classes[b as usize] == 0 {
                    classes[b as usize] = stride as u16;
                    stride += 1;
                }
            }
        }
        // A trie first (`NONE` marks a missing edge), then breadth-first failure links that turn
        // it into a DFA and merge each state's outputs with those of its longest proper suffix.
        const NONE: u32 = u32::MAX;
        let mut delta = vec![NONE; stride];
        let mut out: Vec<Vec<u32>> = vec![Vec::new()];
        for &(l, id) in &lits {
            let mut s = 0;
            for &b in l {
                let at = s * stride + classes[b as usize] as usize;
                if delta[at] == NONE {
                    delta[at] = out.len() as u32;
                    delta.extend(std::iter::repeat_n(NONE, stride));
                    out.push(Vec::new());
                }
                s = delta[at] as usize;
            }
            out[s].push(id);
        }
        let mut fail = vec![0u32; out.len()];
        let mut queue = std::collections::VecDeque::new();
        for edge in &mut delta[..stride] {
            match *edge {
                NONE => *edge = 0,
                t => queue.push_back(t),
            }
        }
        while let Some(s) = queue.pop_front() {
            let s = s as usize;
            let inherited = out[fail[s] as usize].clone();
            out[s].extend(inherited);
            for c in 0..stride {
                let via_fail = delta[fail[s] as usize * stride + c];
                match delta[s * stride + c] {
                    NONE => delta[s * stride + c] = via_fail,
                    t => {
                        fail[t as usize] = via_fail;
                        queue.push_back(t);
                    }
                }
            }
        }
        let mut ends = vec![0];
        let mut hits = Vec::new();
        for mut o in out {
            o.sort_unstable();
            o.dedup();
            hits.extend(o);
            ends.push(hits.len() as u32);
        }
        Screen {
            classes,
            stride,
            delta,
            ends,
            hits,
            always,
            len,
        }
    }

    pub(crate) fn candidates(&self, h: &Haystack) -> Candidates {
        let mut set = vec![0u64; self.len.div_ceil(64)];
        let mut mark = |i: u32| set[i as usize / 64] |= 1 << (i % 64);
        self.always.iter().for_each(|&i| mark(i));
        if self.stride > 1 {
            let mut s = 0;
            for &b in h.lower.as_bytes() {
                s = self.delta[s * self.stride + self.classes[b as usize] as usize] as usize;
                let (a, z) = (self.ends[s] as usize, self.ends[s + 1] as usize);
                self.hits[a..z].iter().for_each(|&i| mark(i));
            }
        }
        Candidates(set)
    }
}

/// Whole-pattern answer for a literal alternation, optionally anchored at either end.
#[derive(Debug, Clone)]
struct Literals {
    set: Vec<String>,
    ci: bool,
    start: bool,
    end: bool,
}

impl Literals {
    fn from_ast(n: &Node) -> Option<Literals> {
        let (mut start, mut end) = (false, false);
        let mid = match n {
            Node::Concat(items) => {
                let mut items = &items[..];
                if let [Node::Look(Look::Start), rest @ ..] = items {
                    start = true;
                    items = rest;
                }
                if let [rest @ .., Node::Look(Look::End)] = items {
                    end = true;
                    items = rest;
                }
                Node::Concat(items.to_vec())
            }
            Node::Look(Look::Start) => {
                start = true;
                Node::Empty
            }
            Node::Look(Look::End) => {
                end = true;
                Node::Empty
            }
            n => n.clone(),
        };
        let (set, case) = exact(&mid)?;
        let ci = match case {
            Case::Mixed => return None,
            Case::Insensitive => true,
            Case::Neutral | Case::Sensitive => false,
        };
        let set = if ci {
            set.into_iter().map(|s| s.to_ascii_lowercase()).collect()
        } else {
            set
        };
        Some(Literals {
            set,
            ci,
            start,
            end,
        })
    }

    fn is_match(&self, h: &Haystack) -> bool {
        let hay = if self.ci { h.lower.as_ref() } else { h.raw };
        self.set.iter().any(|s| {
            let s = s.as_str();
            match (self.start, self.end) {
                (true, true) => hay == s,
                (true, false) => hay.starts_with(s),
                (false, true) => hay.ends_with(s),
                (false, false) => hay.contains(s),
            }
        })
    }
}

// ---------------------------------------------------------------- program

#[derive(Debug, Clone)]
enum Inst {
    Char(char),
    Class(Class),
    Any,
    AnyNotNewline,
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Look(Look),
    Match,
}

struct Compiler<'a> {
    insts: Vec<Inst>,
    src: &'a str,
}

impl Compiler<'_> {
    fn push(&mut self, i: Inst) -> Result<usize, Error> {
        if self.insts.len() >= MAX_INSTS {
            return Err(Error {
                message: "pattern too large".into(),
                pattern: self.src.to_string(),
            });
        }
        self.insts.push(i);
        Ok(self.insts.len() - 1)
    }

    fn node(&mut self, n: &Node) -> Result<(), Error> {
        match n {
            Node::Empty => {}
            Node::Lit(c, ci) => {
                if *ci && c.is_ascii_alphabetic() {
                    let l = c.to_ascii_lowercase() as u32;
                    let u = c.to_ascii_uppercase() as u32;
                    self.push(Inst::Class(Class::new(vec![(l, l), (u, u)])))?;
                } else {
                    self.push(Inst::Char(*c))?;
                }
            }
            Node::Class(c) => {
                self.push(Inst::Class(c.clone()))?;
            }
            Node::Any { newline } => {
                self.push(if *newline {
                    Inst::Any
                } else {
                    Inst::AnyNotNewline
                })?;
            }
            Node::Look(l) => {
                self.push(Inst::Look(*l))?;
            }
            Node::Group(n, cap) => match cap {
                Some(i) => {
                    self.push(Inst::Save(2 * i))?;
                    self.node(n)?;
                    self.push(Inst::Save(2 * i + 1))?;
                }
                None => self.node(n)?,
            },
            Node::Concat(items) => {
                for it in items {
                    self.node(it)?;
                }
            }
            Node::Alt(bs) => {
                let mut jumps = Vec::new();
                for (k, b) in bs.iter().enumerate() {
                    if k + 1 < bs.len() {
                        let split = self.push(Inst::Split(0, 0))?;
                        self.node(b)?;
                        jumps.push(self.push(Inst::Jmp(0))?);
                        let next = self.insts.len();
                        self.insts[split] = Inst::Split(split + 1, next);
                    } else {
                        self.node(b)?;
                    }
                }
                let end = self.insts.len();
                for j in jumps {
                    self.insts[j] = Inst::Jmp(end);
                }
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => {
                for _ in 0..*min {
                    self.node(node)?;
                }
                let split = |at: usize, body: usize, out: usize| {
                    if *greedy {
                        (at, Inst::Split(body, out))
                    } else {
                        (at, Inst::Split(out, body))
                    }
                };
                match max {
                    None => {
                        let l = self.push(Inst::Split(0, 0))?;
                        self.node(node)?;
                        self.push(Inst::Jmp(l))?;
                        let out = self.insts.len();
                        let (at, i) = split(l, l + 1, out);
                        self.insts[at] = i;
                    }
                    Some(max) => {
                        let mut holes = Vec::new();
                        for _ in *min..*max {
                            holes.push(self.push(Inst::Split(0, 0))?);
                            self.node(node)?;
                        }
                        let out = self.insts.len();
                        for h in holes {
                            let (at, i) = split(h, h + 1, out);
                            self.insts[at] = i;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- matching

/// A compiled pattern. Immutable, so it is `Send + Sync` and can be shared freely.
#[derive(Debug, Clone)]
pub(crate) struct Regex {
    insts: Vec<Inst>,
    /// Number of capture slots, two per group including the implicit group 0.
    slots: usize,
    /// Every match starts at offset 0.
    anchored: bool,
    literals: Option<Literals>,
    pre: Prefilter,
    /// Bytes a match can start with, when that set is known and not every byte.
    first: Option<Box<[bool; 256]>>,
}

/// Capture groups of one match, as byte ranges into the haystack.
#[derive(Debug, Clone)]
pub(crate) struct Captures<'h> {
    hay: &'h str,
    slots: Vec<Option<usize>>,
}

impl<'h> Captures<'h> {
    pub(crate) fn get(&self, i: usize) -> Option<&'h str> {
        match (self.slots.get(2 * i)?, self.slots.get(2 * i + 1)?) {
            (Some(s), Some(e)) => Some(&self.hay[*s..*e]),
            _ => None,
        }
    }
}

/// `caps[i]` is group `i`; panics if the group did not take part in the match.
impl<'h> std::ops::Index<usize> for Captures<'h> {
    type Output = str;
    fn index(&self, i: usize) -> &str {
        self.get(i)
            .unwrap_or_else(|| panic!("capture group {i} did not participate"))
    }
}

struct Threads {
    dense: Vec<usize>,
    sparse: Vec<usize>,
    caps: Vec<Option<usize>>,
    slots: usize,
}

impl Threads {
    fn new() -> Self {
        Threads {
            dense: Vec::new(),
            sparse: Vec::new(),
            caps: Vec::new(),
            slots: 0,
        }
    }
    /// Size for a program; `sparse` needs no clearing, as membership is confirmed via `dense`.
    fn reset(&mut self, n: usize, slots: usize) {
        self.dense.clear();
        if self.sparse.len() < n {
            self.sparse.resize(n, 0);
        }
        self.caps.clear();
        self.caps.resize(n * slots, None);
        self.slots = slots;
    }
    fn contains(&self, pc: usize) -> bool {
        let i = self.sparse[pc];
        i < self.dense.len() && self.dense[i] == pc
    }
    fn insert(&mut self, pc: usize) {
        self.sparse[pc] = self.dense.len();
        self.dense.push(pc);
    }
    fn caps(&self, pc: usize) -> &[Option<usize>] {
        &self.caps[pc * self.slots..(pc + 1) * self.slots]
    }
}

enum Frame {
    Explore(usize),
    Restore(usize, Option<usize>),
}

/// Per-thread VM buffers, reused across searches so matching does not allocate.
struct Cache {
    clist: Threads,
    nlist: Threads,
    stack: Vec<Frame>,
    scratch: Vec<Option<usize>>,
    visited: Vec<u64>,
    jobs: Vec<Job>,
}

/// Backtracker work item.
enum Job {
    Step(usize, usize),
    Restore(usize, Option<usize>),
}

/// The backtracker's visited set holds one bit per (instruction, offset); above this many bits
/// the Pike VM, whose memory does not grow with the haystack, is used instead.
const MAX_VISITED_BITS: usize = 1 << 21;

impl Cache {
    fn new() -> Self {
        Cache {
            clist: Threads::new(),
            nlist: Threads::new(),
            stack: Vec::new(),
            scratch: Vec::new(),
            visited: Vec::new(),
            jobs: Vec::new(),
        }
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::new());
}

fn is_word(c: Option<char>) -> bool {
    c.is_some_and(is_word_char)
}

/// The bytes that the first character of any match can start with, found by following every
/// empty-width path from the start of the program to the first character-consuming instruction.
/// `None` when a match can be empty or start with any character, so nothing can be skipped.
fn first_bytes(insts: &[Inst]) -> Option<Box<[bool; 256]>> {
    let mut set = Box::new([false; 256]);
    let mut seen = vec![false; insts.len()];
    let mut stack = vec![0];
    while let Some(pc) = stack.pop() {
        if std::mem::replace(&mut seen[pc], true) {
            continue;
        }
        match &insts[pc] {
            Inst::Char(c) => {
                let mut buf = [0; 4];
                set[c.encode_utf8(&mut buf).as_bytes()[0] as usize] = true;
            }
            Inst::Class(cl) => {
                for b in 0..0x80u8 {
                    set[b as usize] |= cl.contains(b as char);
                }
                // Any multi-byte character may be in the class; mark every lead byte.
                set[0xc0..].fill(true);
            }
            Inst::Any | Inst::AnyNotNewline | Inst::Match => return None,
            Inst::Split(a, b) => {
                stack.push(*b);
                stack.push(*a);
            }
            Inst::Jmp(t) => stack.push(*t),
            // A look-around only narrows where a match can be; what follows decides the byte.
            Inst::Save(_) | Inst::Look(_) => stack.push(pc + 1),
        }
    }
    Some(set)
}

impl Regex {
    pub(crate) fn new(pattern: &str) -> Result<Regex, Error> {
        Self::build(pattern, false)
    }

    /// Compile with `(?i)` applied to the whole pattern.
    pub(crate) fn case_insensitive(pattern: &str) -> Result<Regex, Error> {
        Self::build(pattern, true)
    }

    pub(crate) fn build(pattern: &str, ci: bool) -> Result<Regex, Error> {
        let mut p = Parser {
            src: pattern,
            c: pattern.chars().collect(),
            i: 0,
            caps: 0,
        };
        let ast = p.alternation(
            Flags {
                ci,
                ..Flags::default()
            },
            0,
        )?;
        if p.i != p.c.len() {
            return p.err("unopened group");
        }
        let mut c = Compiler {
            insts: Vec::new(),
            src: pattern,
        };
        c.push(Inst::Save(0))?;
        c.node(&ast)?;
        c.push(Inst::Save(1))?;
        c.push(Inst::Match)?;
        Ok(Regex {
            first: first_bytes(&c.insts),
            insts: c.insts,
            slots: 2 * (p.caps + 1),
            anchored: anchored(&ast, false),
            literals: Literals::from_ast(&ast),
            pre: Prefilter::new(&ast),
        })
    }

    pub(crate) fn is_match(&self, hay: &str) -> bool {
        self.is_match_in(&Haystack::new(hay))
    }

    /// [`Self::is_match`] against a haystack prepared once for many patterns.
    pub(crate) fn is_match_in(&self, h: &Haystack) -> bool {
        if let Some(l) = &self.literals {
            return l.is_match(h);
        }
        self.pre.may_match(h) && self.run(h.raw, &mut []).is_some()
    }

    pub(crate) fn captures<'h>(&self, hay: &'h str) -> Option<Captures<'h>> {
        if !self.pre.may_match(&Haystack::new(hay)) {
            return None;
        }
        let mut slots = vec![None; self.slots];
        self.run(hay, &mut slots)?;
        Some(Captures { hay, slots })
    }

    /// Byte range of the leftmost-first match.
    pub(crate) fn find(&self, hay: &str) -> Option<(usize, usize)> {
        if !self.pre.may_match(&Haystack::new(hay)) {
            return None;
        }
        let mut slots = [None, None];
        self.run(hay, &mut slots)?;
        Some((slots[0]?, slots[1]?))
    }

    /// Replace the leftmost-first match, if any.
    pub(crate) fn replace_first(&self, hay: &str, with: &str) -> String {
        match self.find(hay) {
            Some((s, e)) => format!("{}{with}{}", &hay[..s], &hay[e..]),
            None => hay.to_string(),
        }
    }

    /// Pike VM. With an empty `out` it stops at the first match; otherwise it records the
    /// leftmost-first match's capture slots (as many as `out` holds).
    fn run(&self, hay: &str, out: &mut [Option<usize>]) -> Option<()> {
        let small = self.insts.len().saturating_mul(hay.len() + 1) <= MAX_VISITED_BITS;
        let mut go = |c: &mut Cache| {
            if small {
                self.backtrack(c, hay, out)
            } else {
                self.pike(c, hay, out)
            }
        };
        CACHE.with(|c| match c.try_borrow_mut() {
            Ok(mut c) => go(&mut c),
            // Only reachable if a search re-entered itself; never share the buffers.
            Err(_) => go(&mut Cache::new()),
        })
    }

    /// Bounded backtracking: depth-first in priority order, so the first match found is the
    /// leftmost-first one. Each (instruction, offset) is explored at most once — a state that
    /// failed once fails again, whatever the captures — which keeps the search linear.
    fn backtrack(&self, cache: &mut Cache, hay: &str, out: &mut [Option<usize>]) -> Option<()> {
        let n = self.insts.len();
        let width = hay.len() + 1;
        let visited = &mut cache.visited;
        visited.clear();
        visited.resize((n * width).div_ceil(64), 0);
        let jobs = &mut cache.jobs;
        let slots = &mut cache.scratch;
        slots.clear();
        slots.resize(out.len(), None);
        let b = hay.as_bytes();
        for start in 0..width {
            if start > 0 && self.anchored {
                break;
            }
            if let Some(first) = &self.first {
                if b.get(start).is_none_or(|&c| !first[c as usize]) {
                    continue;
                }
            } else if start < hay.len() && !hay.is_char_boundary(start) {
                continue;
            }
            jobs.clear();
            jobs.push(Job::Step(0, start));
            while let Some(job) = jobs.pop() {
                let (mut pc, mut at) = match job {
                    Job::Step(pc, at) => (pc, at),
                    Job::Restore(s, v) => {
                        slots[s] = v;
                        continue;
                    }
                };
                loop {
                    let bit = pc * width + at;
                    if visited[bit / 64] & (1 << (bit % 64)) != 0 {
                        break;
                    }
                    visited[bit / 64] |= 1 << (bit % 64);
                    let cur = || hay[at..].chars().next();
                    match &self.insts[pc] {
                        Inst::Match => {
                            out.copy_from_slice(slots);
                            return Some(());
                        }
                        Inst::Char(x) => match cur() {
                            Some(c) if c == *x => {
                                at += c.len_utf8();
                                pc += 1;
                            }
                            _ => break,
                        },
                        Inst::Class(cl) => {
                            // ASCII fast path: most rule text and haystacks are ASCII.
                            let c = match b.get(at) {
                                Some(&x) if x < 0x80 => Some(x as char),
                                Some(_) => cur(),
                                None => None,
                            };
                            match c {
                                Some(c) if cl.contains(c) => {
                                    at += c.len_utf8();
                                    pc += 1;
                                }
                                _ => break,
                            }
                        }
                        Inst::Any | Inst::AnyNotNewline => match cur() {
                            Some(c) if c != '\n' || matches!(self.insts[pc], Inst::Any) => {
                                at += c.len_utf8();
                                pc += 1;
                            }
                            _ => break,
                        },
                        Inst::Jmp(x) => pc = *x,
                        Inst::Split(x, y) => {
                            jobs.push(Job::Step(*y, at));
                            pc = *x;
                        }
                        Inst::Save(s) => {
                            if *s < slots.len() {
                                jobs.push(Job::Restore(*s, slots[*s]));
                                slots[*s] = Some(at);
                            }
                            pc += 1;
                        }
                        Inst::Look(l) => {
                            let prev = hay[..at].chars().next_back();
                            let next = cur();
                            let holds = match l {
                                Look::Start => at == 0,
                                Look::End => at == hay.len(),
                                Look::WordBoundary => is_word(prev) != is_word(next),
                                Look::NotWordBoundary => is_word(prev) == is_word(next),
                            };
                            if !holds {
                                break;
                            }
                            pc += 1;
                        }
                    }
                }
            }
        }
        None
    }

    fn pike(&self, cache: &mut Cache, hay: &str, out: &mut [Option<usize>]) -> Option<()> {
        let n = self.insts.len();
        let slots = out.len();
        let Cache {
            clist,
            nlist,
            stack,
            scratch,
            ..
        } = cache;
        clist.reset(n, slots);
        nlist.reset(n, slots);
        stack.clear();
        scratch.clear();
        scratch.resize(slots, None);
        let mut matched = false;
        let mut at = 0;
        let mut prev: Option<char> = None;
        loop {
            let cur = hay[at..].chars().next();
            if !matched && (at == 0 || !self.anchored) {
                scratch.fill(None);
                self.add(clist, stack, scratch, 0, at, prev, cur);
            }
            if clist.dense.is_empty() && (matched || self.anchored) {
                break;
            }
            let next_at = at + cur.map_or(0, char::len_utf8);
            let after = cur.and_then(|_| hay[next_at..].chars().next());
            for i in 0..clist.dense.len() {
                let pc = clist.dense[i];
                let ok = match (&self.insts[pc], cur) {
                    (Inst::Match, _) => {
                        if slots == 0 {
                            return Some(());
                        }
                        out.copy_from_slice(clist.caps(pc));
                        matched = true;
                        // Lower-priority threads can no longer win.
                        break;
                    }
                    (_, None) => false,
                    (Inst::Char(x), Some(c)) => *x == c,
                    (Inst::Class(cl), Some(c)) => cl.contains(c),
                    (Inst::Any, Some(_)) => true,
                    (Inst::AnyNotNewline, Some(c)) => c != '\n',
                    // Empty transitions are recorded in the set only to stop revisits.
                    _ => false,
                };
                if ok {
                    scratch.copy_from_slice(clist.caps(pc));
                    self.add(nlist, stack, scratch, pc + 1, next_at, cur, after);
                }
            }
            if cur.is_none() {
                break;
            }
            at = next_at;
            prev = cur;
            std::mem::swap(clist, nlist);
            nlist.dense.clear();
        }
        matched.then_some(())
    }

    /// Follow empty transitions from `pc`, queueing reachable consuming instructions in priority
    /// order with the capture slots in effect on the way.
    #[allow(clippy::too_many_arguments)]
    fn add(
        &self,
        list: &mut Threads,
        stack: &mut Vec<Frame>,
        caps: &mut [Option<usize>],
        pc: usize,
        at: usize,
        prev: Option<char>,
        next: Option<char>,
    ) {
        stack.push(Frame::Explore(pc));
        while let Some(f) = stack.pop() {
            let mut pc = match f {
                Frame::Explore(pc) => pc,
                Frame::Restore(slot, v) => {
                    caps[slot] = v;
                    continue;
                }
            };
            loop {
                if list.contains(pc) {
                    break;
                }
                list.insert(pc);
                match &self.insts[pc] {
                    Inst::Jmp(x) => pc = *x,
                    Inst::Split(x, y) => {
                        stack.push(Frame::Explore(*y));
                        pc = *x;
                    }
                    Inst::Save(s) => {
                        if *s < caps.len() {
                            stack.push(Frame::Restore(*s, caps[*s]));
                            caps[*s] = Some(at);
                        }
                        pc += 1;
                    }
                    Inst::Look(l) => {
                        let holds = match l {
                            Look::Start => at == 0,
                            Look::End => next.is_none(),
                            Look::WordBoundary => is_word(prev) != is_word(next),
                            Look::NotWordBoundary => is_word(prev) == is_word(next),
                        };
                        if !holds {
                            break;
                        }
                        pc += 1;
                    }
                    _ => {
                        let s = list.slots;
                        list.caps[pc * s..(pc + 1) * s].copy_from_slice(caps);
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, h: &str) -> bool {
        Regex::new(p).unwrap().is_match(h)
    }

    #[test]
    fn literals_and_alternation() {
        assert!(m("abc", "xxabcxx"));
        assert!(!m("abc", "ab"));
        assert!(m("foo|bar", "a bar"));
        assert!(m("(?i)Instagram|FBAN", "xx fban/yy"));
        assert!(!m("Instagram", "instagram"));
        assert!(m("", "anything"));
        assert!(m("a|", "zzz"), "empty branch matches");
    }

    #[test]
    fn anchors() {
        assert!(m("^ab", "abc") && !m("^ab", "cab"));
        assert!(m("ab$", "cab") && !m("ab$", "abc"));
        assert!(m("^(mail|docs)\\.", "docs.google.com"));
        assert!(!m("^(mail|docs)\\.", "xdocs.google.com"));
        assert!(m("^$", "") && !m("^$", "a"));
        assert!(!m("a$", "a\n"), "$ is end of text only");
        assert!(m("(^|\\.)google\\.[a-z.]+$", "www.google.co.uk"));
        assert!(!m("(^|\\.)google\\.[a-z.]+$", "notgoogle.com"));
    }

    #[test]
    fn classes() {
        assert!(m("[a-c]x", "bx") && !m("[a-c]x", "dx"));
        assert!(m("[^\"]+", "abc") && !m("^[^\"]+$", "a\"b"));
        assert!(m("[-_. ]?ads?$", "google_ads"));
        assert!(m("[]a]", "]"));
        assert!(m("[a-]", "-"));
        assert!(m("[\\d.]+", "1.2"));
        assert!(m("(?i)[a-c]", "B") && !m("[a-c]", "B"));
        assert!(!m("(?i)[^a]", "A"), "negation applies after folding");
        assert!(m("\\D", "é"), "negated ASCII classes include non-ASCII");
        assert!(!m("\\d", "٣"), "\\d is ASCII");
    }

    #[test]
    fn word_boundaries() {
        assert!(m("\\bGPTBot\\b", "Mozilla GPTBot/1.0"));
        assert!(!m("\\bGPTBot\\b", "XGPTBot/1.0"));
        assert!(!m("\\bGPTBot\\b", "GPTBot_x"));
        assert!(!m("\\bGPTBot\\b", "éGPTBot") && !m("\\bGPTBot\\b", "GPTBot日本"));
        assert!(m("\\w", "é") && m("\\s", "\u{a0}") && !m("\\W", "é"));
        assert!(m("\\bbot\\b", "bot"));
        assert!(m("\\Bot", "bot2bot") && !m("\\Bbot", "bot"));
    }

    #[test]
    fn repetition() {
        assert!(m("^a{2,3}$", "aa") && m("^a{2,3}$", "aaa"));
        assert!(!m("^a{2,3}$", "a") && !m("^a{2,3}$", "aaaa"));
        assert!(m("^a{2,}$", "aaaaa") && m("^a{2}$", "aa"));
        assert!(m("^(ab)+$", "ababab") && !m("^(ab)+$", "aba"));
        assert!(m("^(a*)*$", "aaa"), "nested empty loops terminate");
        assert!(m("\\bAFT[A-Z0-9]{1,6}\\b", "AFTMM Build"));
        assert!(!m("\\bAFT[A-Z0-9]{1,6}\\b", "AFTABCDEFG"));
    }

    #[test]
    fn dot_and_flags() {
        assert!(m("a.c", "abc") && !m("a.c", "a\nc"));
        assert!(m("(?s)a.c", "a\nc"));
        assert!(m("(?i:ab)c", "ABc") && !m("(?i:ab)c", "ABC"));
        assert!(m("a(?i)b", "aB") && !m("a(?i)b", "AB"));
        assert!(m("(?i)a(?-i)b", "Ab") && !m("(?i)a(?-i)b", "AB"));
    }

    #[test]
    fn captures_follow_leftmost_first_priority() {
        let r = Regex::new(r"OS (\d+[_.]\d+(?:[_.]\d+)?)").unwrap();
        let c = r.captures("iPhone OS 18_6_2 like Mac").unwrap();
        assert_eq!(&c[1], "18_6_2");
        let r = Regex::new("(a|ab)(c|bcd)").unwrap();
        let c = r.captures("abcd").unwrap();
        assert_eq!((&c[0], &c[1], &c[2]), ("abcd", "a", "bcd"));
        let r = Regex::new("(a+?)(a*)").unwrap();
        let c = r.captures("aaa").unwrap();
        assert_eq!((&c[1], &c[2]), ("a", "aa"));
        let r = Regex::new("(x)?y").unwrap();
        assert_eq!(r.captures("y").unwrap().get(1), None);
        assert_eq!(
            Regex::new(r"\((Phone|Tablet|PC); OpenHarmony")
                .unwrap()
                .captures("Mozilla (Tablet; OpenHarmony 5.0)")
                .map(|c| c[1].to_string()),
            Some("Tablet".into())
        );
    }

    #[test]
    fn find_and_replace_first() {
        let r = Regex::new("[-_. ]?ads?$").unwrap();
        assert_eq!(r.replace_first("google_ads", ""), "google");
        assert_eq!(r.replace_first("fbads", ""), "fb");
        assert_eq!(
            r.replace_first("threads", "!"),
            "thre!",
            "callers gate this"
        );
        let tld = Regex::new(r"\.[a-z]{2,3}(\.[a-z]{2})?$").unwrap();
        assert_eq!(tld.replace_first("google.co.uk", ""), "google");
        assert_eq!(tld.find("nodot"), None);
        let r = Regex::new("b*").unwrap();
        assert_eq!(r.find("abb"), Some((0, 0)), "leftmost wins over longest");
    }

    #[test]
    fn utf8_haystacks() {
        assert!(m("é+", "café"));
        assert!(m("^.{4}$", "café"), "dot consumes a whole character");
        let r = Regex::new("(é)").unwrap();
        assert_eq!(r.find("aéb"), Some((1, 3)));
    }

    #[test]
    fn unsupported_syntax_is_an_error() {
        for bad in [
            "(",
            ")",
            "(?=a)",
            "(?!a)",
            "(?<=a)",
            "a**",
            "*a",
            "[a",
            "[[:alpha:]]",
            "[a&&b]",
            "\\1",
            "\\p{L}",
            "\\<",
            "[\\W]",
            "a{2,1}",
            "a{1001}",
            "a{",
            "(?m)^a",
            "\\",
            "\\q",
            "a{,2}",
        ] {
            assert!(Regex::new(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn literal_analysis() {
        let lit = |p: &str| Regex::new(p).unwrap().literals.is_some();
        assert!(lit("foo|bar") && lit("^(a|b)$") && lit("(?i)abc") && lit("ads?"));
        assert!(!lit("\\bfoo\\b") && !lit("a.c") && !lit("(?i:a)B"));
        let req = |p: &str| Regex::new(p).unwrap().pre.required;
        assert_eq!(req("\\bGPTBot\\b"), Some(vec!["gptbot".into()]));
        assert_eq!(
            req("Version/[\\d.]+.*Safari/"),
            Some(vec!["version/".into()])
        );
        assert_eq!(req("a*"), None);
        assert_eq!(
            req("x+\\bfoo|bar\\b"),
            Some(vec!["bar".into(), "foo".into()])
        );
    }

    #[test]
    fn backtracker_and_pike_vm_agree() {
        let pats = [
            r"OS (\d+[_.]\d+(?:[_.]\d+)?)",
            "(a|ab)(c|bcd)",
            "(a+?)(a*)",
            "(x)?y",
            r"\bfoo\b|bar$",
            "^(.*(([^a-df-z]|^)shop|shopping).*)$",
            r"\.[a-z]{2,3}(\.[a-z]{2})?$",
            "[-_. ]?ads?$",
            "b*",
            r#"^\S+ \S+ \S+ \[[^\]]+\] "(?:[A-Z]+) (\S+)[^"]*" \d{3} \S+ "((?:[^"\\]|\\.)*)""#,
            "(?s)a.c|é+",
            r"\Bot",
            "(?i)ShOp",
            "[^a]b",
            r"\d+|é",
            r"(?:\s|x)?y\b",
        ];
        let hays = [
            "",
            "a",
            "abcd",
            "aaa",
            "y",
            "xy",
            "foo",
            "a foo b",
            "xbar",
            "google.co.uk",
            "fbads",
            "myshop",
            "shopping",
            "iPhone OS 18_6_2 like",
            "a\nc",
            "café",
            "bot2bot",
            r#"1.2.3.4 - - [24/Oct/2026:10:00:00 -0500] "GET /x?a=1 HTTP/1.1" 200 5 "ref\"x" "ua""#,
        ];
        let mut cache = Cache::new();
        for p in pats {
            let r = Regex::new(p).unwrap();
            for h in hays {
                for slots in [0, 2, r.slots] {
                    let mut a = vec![None; slots];
                    let mut b = vec![None; slots];
                    let ra = r.backtrack(&mut cache, h, &mut a);
                    let rb = r.pike(&mut cache, h, &mut b);
                    assert_eq!((ra, &a), (rb, &b), "{p:?} on {h:?} with {slots} slots");
                }
            }
        }
    }

    #[test]
    fn screen_never_drops_a_pattern_that_matches() {
        let pats = [
            "Googlebot",
            "(?i)bingbot",
            "Applebot(-Extended)?",
            r"^curl/\d",
            r"bot\b",
            "(a|b)c",
            "x*",
            "",
            "FBAN|FBAV",
            r"\(iPhone",
            "Instagram [0-9]",
            "^$",
            "é",
            "(?i)Crawler|spider",
        ];
        let rx: Vec<Regex> = pats.iter().map(|p| Regex::new(p).unwrap()).collect();
        let screen = Screen::new(&rx);
        let hays = [
            "",
            "Mozilla/5.0 (compatible; Googlebot/2.1)",
            "BINGBOT",
            "Applebot-Extended/0.1",
            "curl/8.4.0",
            "robot",
            "a bot",
            "bc",
            "FBAN/FBIOS;FBAV/400",
            "Mozilla (iPhone; CPU",
            "Instagram 312.0",
            "café",
            "Some SPIDER",
            "googlebo",
            "xxxx",
        ];
        for h in hays {
            let hay = Haystack::new(h);
            let cands: Vec<usize> = screen.candidates(&hay).into_indices().collect();
            assert!(cands.windows(2).all(|w| w[0] < w[1]), "{h:?}: {cands:?}");
            for (i, r) in rx.iter().enumerate() {
                if r.is_match_in(&hay) {
                    assert!(
                        cands.contains(&i),
                        "{:?} matches {h:?} but was screened out",
                        pats[i]
                    );
                }
            }
        }
        let hay = Haystack::new("nothing relevant");
        let left: Vec<usize> = screen.candidates(&hay).into_indices().collect();
        // Only the patterns with no required literal survive: `x*`, `` and `^$`.
        assert_eq!(left, [6, 7, 11]);
    }

    #[test]
    fn long_haystacks_use_the_pike_vm() {
        let r = Regex::new(r"\bneedle\b").unwrap();
        let hay = "x ".repeat(MAX_VISITED_BITS) + "needle";
        assert!(r.insts.len() * (hay.len() + 1) > MAX_VISITED_BITS);
        assert!(r.is_match(&hay));
        assert_eq!(r.find(&hay).map(|m| m.1), Some(hay.len()));
    }

    #[test]
    fn hostile_input_is_linear() {
        let r = Regex::new("^(a+)+$").unwrap();
        let hay = "a".repeat(20_000) + "!";
        let t = std::time::Instant::now();
        assert!(!r.is_match(&hay));
        assert!(t.elapsed().as_secs() < 2, "{:?}", t.elapsed());
    }
}
