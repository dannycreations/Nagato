use nagato_core::Error;

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
  last_line_was_new_file: bool,
  in_binary_patch: bool,
}

impl<'a> Lexer<'a> {
  #[doc(hidden)]
  pub fn new(input: &'a [u8]) -> Self {
    Lexer {
      input,
      pos: 0,
      line_num: 0,
      last_line_was_new_file: false,
      in_binary_patch: false,
    }
  }

  fn parse_line(&mut self) -> Option<Result<LexerItem<'a>, Error>> {
    let line = self.next_line()?;
    let line_num = self.line_num;

    if self.in_binary_patch {
      return self.parse_binary_line(line, line_num);
    }

    if line.is_empty() {
      self.last_line_was_new_file = true;
      return Some(Ok(LexerItem {
        token: TokenKind::Context(&[]),
        line_num,
      }));
    }

    let token_result = match line[0] {
      b'+' => self.parse_plus_line(line),
      b'-' => self.parse_minus_line(line),
      b' ' => {
        self.last_line_was_new_file = true;
        Ok(TokenKind::Context(&line[1..]))
      }
      b'@' => self.parse_at_line(line),
      b'd' => self.parse_d_line(line),
      b'f' => self.parse_f_line(line),
      b'G' => self.parse_g_line(line),
      b'i' => self.parse_i_line(line),
      b'n' => self.parse_n_line(line),
      b'o' => self.parse_o_line(line),
      b'r' => self.parse_r_line(line),
      b'c' => self.parse_c_line(line),
      b's' => self.parse_s_line(line),
      b'B' => self.parse_b_line(line),
      _ => self.parse_non_keyword_line(line),
    };

    Some(
      token_result
        .map(|token| LexerItem { token, line_num })
        .map_err(|kind| Error::with_line(kind, line_num)),
    )
  }

  /// Advance to the next line and return it, normalized.
  #[inline]
  fn next_line(&mut self) -> Option<&'a [u8]> {
    let (line, next_input) = crate::get_line(&self.input[self.pos..])?;
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
