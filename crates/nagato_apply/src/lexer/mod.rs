use nagato_core::{get_line, Error};

mod token;
mod tokenizer;

pub use token::*;

#[derive(Debug, Clone, PartialEq)]
pub struct LexerItem<'a> {
  pub token: TokenKind<'a>,
  pub line_num: u32,
}

#[doc(hidden)]
pub struct Lexer<'a> {
  input: &'a [u8],
  pos: usize,
  line_num: u32,
  is_new_file_context: bool,
  is_in_binary_patch: bool,
}

impl<'a> Lexer<'a> {
  #[doc(hidden)]
  pub fn new(input: &'a [u8]) -> Self {
    Lexer {
      input,
      pos: 0,
      line_num: 0,
      is_new_file_context: false,
      is_in_binary_patch: false,
    }
  }

  fn parse_line(&mut self) -> Option<Result<LexerItem<'a>, Error>> {
    let line = self.next_line()?;
    let line_num = self.line_num;

    if self.is_in_binary_patch {
      return self.parse_binary_line(line, line_num);
    }

    if line.is_empty() {
      self.is_new_file_context = true;
      return Some(Ok(LexerItem {
        token: TokenKind::Context(&[]),
        line_num,
      }));
    }

    let token_result = self.dispatch_line(line);

    Some(
      token_result
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
