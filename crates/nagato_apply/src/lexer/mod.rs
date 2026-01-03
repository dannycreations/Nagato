use nagato_core::{get_line, Error};

mod token;
mod tokenizer;

pub use token::*;

#[derive(Debug, Clone, PartialEq)]
pub struct LexerItem<'a> {
  pub token: TokenKind<'a>,
  pub line_num: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexerMode {
  Text,
  Binary,
}

#[doc(hidden)]
pub struct Lexer<'a> {
  input: &'a [u8],
  pos: usize,
  line_num: u32,
  mode: LexerMode,
}

impl<'a> Lexer<'a> {
  #[doc(hidden)]
  pub fn new(input: &'a [u8]) -> Self {
    Lexer {
      input,
      pos: 0,
      line_num: 0,
      mode: LexerMode::Text,
    }
  }

  /// Set the lexer mode.
  pub(crate) fn set_mode(&mut self, mode: LexerMode) {
    self.mode = mode;
  }

  #[inline]
  fn parse_line(&mut self) -> Option<Result<LexerItem<'a>, Error>> {
    let line = self.next_line()?;
    let line_num = self.line_num;

    let res = match self.mode {
      LexerMode::Binary => self.tokenize_binary(line),
      LexerMode::Text => self.tokenize_text(line),
    };

    Some(
      res
        .map(|token| LexerItem { token, line_num })
        .map_err(|kind| Error::with_line(kind, line_num)),
    )
  }

  /// Advance to the next line and return it, normalized.
  #[inline]
  fn next_line(&mut self) -> Option<&'a [u8]> {
    let (line, next_input) = get_line(&self.input[self.pos..])?;
    self.line_num += 1;
    self.pos = self.input.len() - next_input.len();
    Some(line)
  }
}

impl<'a> Iterator for Lexer<'a> {
  type Item = Result<LexerItem<'a>, Error>;

  #[inline]
  fn next(&mut self) -> Option<Self::Item> {
    self.parse_line()
  }
}
