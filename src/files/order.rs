//! The order names are listed in, as a file explorer lists them: without regard to case, and a run of
//! digits compared as the number it writes, so `file2` stands before `file10`.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

/// One name against another in that order; names that differ only in case fall back to the bytes.
pub fn natural(one: &str, other: &str) -> Ordering {
  let mut left = one.chars().peekable();
  let mut right = other.chars().peekable();

  loop {
    match (left.peek().copied(), right.peek().copied()) {
      (None, None) => return one.cmp(other),
      (None, Some(_)) => return Ordering::Less,
      (Some(_), None) => return Ordering::Greater,
      (Some(a), Some(b)) if a.is_ascii_digit() && b.is_ascii_digit() => {
        let ordered = number(&mut left).cmp(&number(&mut right));

        if ordered != Ordering::Equal {
          return ordered;
        }
      }
      (Some(a), Some(b)) => {
        let ordered = a.to_lowercase().cmp(b.to_lowercase());

        if ordered != Ordering::Equal {
          return ordered;
        }

        left.next();
        right.next();
      }
    }
  }
}

// A run of digits taken off the front, compared as a number however long it is: its leading zeros
// dropped, then a longer run is the larger number.
fn number(chars: &mut Peekable<Chars<'_>>) -> (usize, String) {
  let mut digits = String::new();

  while let Some(digit) = chars.next_if(char::is_ascii_digit) {
    digits.push(digit);
  }

  let trimmed = digits.trim_start_matches('0').to_owned();

  (trimmed.len(), trimmed)
}
