use std::iter::Peekable;
use std::str::Chars;

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Select,
    From,
    Where,
    Insert,
    Into,
    Values,
    Create,
    Table,
    Delete,
    Identifier(String),
    Number(i32),
    StringLiteral(String),
    Comma,
    OpenParen,
    CloseParen,
    Equals,
    Semicolon,
    Eof,
}

pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars().peekable(),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        if let Some(&c) = self.chars.peek() {
            match c {
                '(' => {
                    self.chars.next();
                    Token::OpenParen
                }
                ')' => {
                    self.chars.next();
                    Token::CloseParen
                }
                ',' => {
                    self.chars.next();
                    Token::Comma
                }
                '=' => {
                    self.chars.next();
                    Token::Equals
                }
                ';' => {
                    self.chars.next();
                    Token::Semicolon
                }
                '\'' => {
                    self.chars.next();
                    let mut s = String::new();
                    for ch in self.chars.by_ref() {
                        if ch == '\'' {
                            break;
                        }
                        s.push(ch);
                    }
                    Token::StringLiteral(s)
                }
                _ if c.is_ascii_digit() => {
                    let mut num = 0;
                    while let Some(&ch) = self.chars.peek() {
                        if ch.is_ascii_digit() {
                            num = num * 10 + ch.to_digit(10).unwrap() as i32;
                            self.chars.next();
                        } else {
                            break;
                        }
                    }
                    Token::Number(num)
                }
                _ if c.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    while let Some(&ch) = self.chars.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' {
                            s.push(ch);
                            self.chars.next();
                        } else {
                            break;
                        }
                    }
                    match s.to_uppercase().as_str() {
                        "SELECT" => Token::Select,
                        "FROM" => Token::From,
                        "WHERE" => Token::Where,
                        "INSERT" => Token::Insert,
                        "INTO" => Token::Into,
                        "VALUES" => Token::Values,
                        "CREATE" => Token::Create,
                        "TABLE" => Token::Table,
                        "DELETE" => Token::Delete,
                        _ => Token::Identifier(s),
                    }
                }
                _ => {
                    self.chars.next(); // Skip unknown
                    self.next_token()
                }
            }
        } else {
            Token::Eof
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Ast {
    Select {
        columns: Vec<String>,
        table: String,
        // predicate mapping not implemented here for simplicity
    },
    Insert {
        table: String,
        values: Vec<String>,
    },
    Create {
        table: String,
        columns: Vec<String>,
    },
    Delete {
        table: String,
    },
}

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current_token: Token,
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Lexer::new(input);
        let current_token = lexer.next_token();
        Self {
            lexer,
            current_token,
        }
    }

    fn advance(&mut self) {
        self.current_token = self.lexer.next_token();
    }

    fn expect(&mut self, token: Token) -> Result<(), String> {
        if std::mem::discriminant(&self.current_token) == std::mem::discriminant(&token) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected {:?}, found {:?}",
                token, self.current_token
            ))
        }
    }

    pub fn parse(&mut self) -> Result<Ast, String> {
        match self.current_token.clone() {
            Token::Select => self.parse_select(),
            Token::Insert => self.parse_insert(),
            Token::Create => self.parse_create(),
            Token::Delete => self.parse_delete(),
            _ => Err(format!(
                "Unexpected statement start: {:?}",
                self.current_token
            )),
        }
    }

    fn parse_select(&mut self) -> Result<Ast, String> {
        self.expect(Token::Select)?;
        let mut columns = Vec::new();

        while let Token::Identifier(id) = &self.current_token {
            columns.push(id.clone());
            self.advance();

            if self.current_token == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(Token::From)?;

        let table = if let Token::Identifier(id) = &self.current_token {
            let t = id.clone();
            self.advance();
            t
        } else {
            return Err("Expected table name".into());
        };

        Ok(Ast::Select { columns, table })
    }

    fn parse_insert(&mut self) -> Result<Ast, String> {
        self.expect(Token::Insert)?;
        self.expect(Token::Into)?;

        let table = if let Token::Identifier(id) = &self.current_token {
            let t = id.clone();
            self.advance();
            t
        } else {
            return Err("Expected table name".into());
        };

        self.expect(Token::Values)?;
        self.expect(Token::OpenParen)?;

        let mut values = Vec::new();
        loop {
            match &self.current_token {
                Token::Number(n) => values.push(n.to_string()),
                Token::StringLiteral(s) => values.push(s.clone()),
                _ => break,
            }
            self.advance();
            if self.current_token == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(Token::CloseParen)?;
        Ok(Ast::Insert { table, values })
    }

    fn parse_create(&mut self) -> Result<Ast, String> {
        self.expect(Token::Create)?;
        self.expect(Token::Table)?;

        let table = if let Token::Identifier(id) = &self.current_token {
            let t = id.clone();
            self.advance();
            t
        } else {
            return Err("Expected table name".into());
        };

        self.expect(Token::OpenParen)?;
        let mut columns = Vec::new();
        while let Token::Identifier(id) = &self.current_token {
            columns.push(id.clone());
            self.advance();

            // Skip type for now
            if let Token::Identifier(_) = &self.current_token {
                self.advance();
            }

            if self.current_token == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::CloseParen)?;
        Ok(Ast::Create { table, columns })
    }

    fn parse_delete(&mut self) -> Result<Ast, String> {
        self.expect(Token::Delete)?;
        self.expect(Token::From)?;

        let table = if let Token::Identifier(id) = &self.current_token {
            let t = id.clone();
            self.advance();
            t
        } else {
            return Err("Expected table name".into());
        };
        Ok(Ast::Delete { table })
    }
}
