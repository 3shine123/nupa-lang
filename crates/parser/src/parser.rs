use nupa_lexer::{KeywordKind, Lexer, Token, TokenKind};
use nupa_cst::*;

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    source: &'a str,
    current: Token,
    previous: Token,
    has_error: bool,
    error_count: usize,
    err_msg: String,
    panic_mode: bool,
    type_names: Vec<String>,
    type_params: Vec<String>,
    macro_names: Vec<String>,
    macro_values: Vec<String>,
    generic_class_names: Vec<String>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token();
        Parser {
            lexer,
            source,
            current,
            previous: Token {
                kind: TokenKind::Eof, keyword: KeywordKind::None,
                start: 0, length: 0, line: 0, column: 0, char_val: 0,
            },
            has_error: false,
            error_count: 0,
            err_msg: String::new(),
            panic_mode: false,
            type_names: vec![
                // Built-in C types (from old C parser's register_builtin_types)
                "fd_set".into(), "timeval".into(), "timespec".into(),
                "termios".into(), "sigaction".into(), "stat".into(),
                "sockaddr".into(), "in_addr".into(), "sockaddr_in".into(),
                "addrinfo".into(), "dirent".into(), "passwd".into(),
                "FILE".into(), "size_t".into(), "ssize_t".into(),
                "int8_t".into(), "int16_t".into(), "int32_t".into(), "int64_t".into(),
                "uint8_t".into(), "uint16_t".into(), "uint32_t".into(), "uint64_t".into(),
                "uintptr_t".into(), "intptr_t".into(),
                "pthread_t".into(), "pthread_mutex_t".into(), "pthread_cond_t".into(),
            ],
            type_params: Vec::new(),
            macro_names: Vec::new(),
            macro_values: Vec::new(),
            generic_class_names: Vec::new(),
        }
    }

    fn current_text(&self) -> &'a str {
        &self.source[self.current.start..self.current.start + self.current.length]
    }

    fn previous_text(&self) -> &'a str {
        &self.source[self.previous.start..self.previous.start + self.previous.length]
    }

    fn peek(&self) -> &Token {
        &self.current
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current.kind == kind
    }

    fn check_keyword(&self, kw: KeywordKind) -> bool {
        self.current.kind == TokenKind::Keyword && self.current.keyword == kw
    }

    fn advance(&mut self) {
        self.previous = std::mem::replace(&mut self.current, self.lexer.next_token());
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_keyword(&mut self, kw: KeywordKind) -> bool {
        if self.check_keyword(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_name(&mut self) -> bool {
        if self.match_token(TokenKind::Identifier) {
            return true;
        }
        if self.current.kind == TokenKind::Keyword {
            let kw = self.current.keyword;
            if kw == KeywordKind::Id || kw == KeywordKind::Class ||
               kw == KeywordKind::Sel || kw == KeywordKind::Instancetype {
                self.advance();
                return true;
            }
        }
        false
    }

    fn is_name_token(&self) -> bool {
        self.current.kind == TokenKind::Identifier ||
        (self.current.kind == TokenKind::Keyword && self.current.keyword == KeywordKind::Self_)
    }

    fn consume(&mut self, kind: TokenKind, msg: &str) {
        if self.check(kind) {
            self.advance();
        } else {
            self.error(&format!("{} (got {})", msg, self.current.kind));
            self.advance();
        }
    }

    fn consume_keyword(&mut self, kw: KeywordKind, msg: &str) {
        if self.check_keyword(kw) {
            self.advance();
        } else {
            self.error(msg);
        }
    }

    fn error(&mut self, msg: &str) {
        if self.panic_mode { return; }
        self.panic_mode = true;
        self.has_error = true;
        self.error_count += 1;
        self.err_msg = msg.to_string();
        eprintln!("error:{}:{}: {}", self.previous.line, self.previous.column, msg);
    }

    fn synchronize(&mut self) {
        self.panic_mode = false;
        while self.current.kind != TokenKind::Eof {
            if self.previous.kind == TokenKind::Semicolon { return; }
            if self.current.kind == TokenKind::Keyword {
                match self.current.keyword {
                    KeywordKind::AtInterface | KeywordKind::AtImplementation |
                    KeywordKind::AtProtocol | KeywordKind::AtEnd |
                    KeywordKind::AtClass | KeywordKind::Return |
                    KeywordKind::If | KeywordKind::While | KeywordKind::For |
                    KeywordKind::Do | KeywordKind::Switch |
                    KeywordKind::Break | KeywordKind::Continue | KeywordKind::Else => return,
                    _ => {}
                }
            }
            if self.current.kind == TokenKind::Eof { return; }
            self.advance();
        }
    }

    fn add_type_name(&mut self, name: &str) {
        if !name.is_empty() && !self.type_names.iter().any(|n| n == name) {
            self.type_names.push(name.to_string());
        }
    }

    fn peek_colon_colon(&self) -> bool {
        // Check if the source after the current identifier has ::
        let start = self.current.start + self.current.length;
        let after = &self.source[start..];
        after.starts_with("::")
    }

    fn is_type_name(&self, name: &str) -> bool {
        self.type_names.iter().any(|n| n == name)
    }

    fn add_type_param(&mut self, name: &str) {
        if !name.is_empty() && !self.type_params.iter().any(|n| n == name) {
            self.type_params.push(name.to_string());
        }
    }

    fn is_type_param(&self, name: &str) -> bool {
        self.type_params.iter().any(|n| n == name)
    }

    fn is_generic_class(&self, name: &str) -> bool {
        self.generic_class_names.iter().any(|n| n == name)
    }

    fn add_macro(&mut self, name: &str, value: &str) {
        self.macro_names.push(name.to_string());
        self.macro_values.push(value.to_string());
    }

    fn lookup_macro(&self, name: &str) -> Option<&str> {
        self.macro_names.iter().rev()
            .zip(self.macro_values.iter().rev())
            .find(|(n, _)| n.as_str() == name)
            .map(|(_, v)| v.as_str())
    }

    fn resolve_macro_int(&self, name: &str) -> Option<i32> {
        self.lookup_macro(name).and_then(|v| v.parse().ok())
    }

    // ─── Type parsing ─────────────────────────────────────────────────────

    fn keyword_to_type_prim(kw: KeywordKind) -> TypePrim {
        match kw {
            KeywordKind::Void => TypePrim::Void,
            KeywordKind::Char => TypePrim::Char,
            KeywordKind::Short => TypePrim::Short,
            KeywordKind::Int => TypePrim::Int,
            KeywordKind::Long => TypePrim::Long,
            KeywordKind::Float => TypePrim::Float,
            KeywordKind::Double => TypePrim::Double,
            KeywordKind::Bool => TypePrim::Bool,
            KeywordKind::Signed => TypePrim::Signed,
            KeywordKind::Unsigned => TypePrim::Unsigned,
            KeywordKind::Id => TypePrim::Id,
            KeywordKind::Class => TypePrim::Class,
            KeywordKind::Sel => TypePrim::Sel,
            KeywordKind::Instancetype => TypePrim::Instancetype,
            _ => TypePrim::Named,
        }
    }

    fn parse_type_name(&mut self) -> Option<CstType> {
        let mut t = CstType::new(TypePrim::Void);

        while {
            if self.match_keyword(KeywordKind::Const) { t.is_const = true; true }
            else if self.match_keyword(KeywordKind::Volatile) { t.is_volatile = true; true }
            else if self.match_keyword(KeywordKind::Static) { true }
            else if self.match_keyword(KeywordKind::Extern) { true }
            else if self.match_keyword(KeywordKind::Weak) { t.is_weak_qual = true; true }
            else if self.match_keyword(KeywordKind::Block) { t.is_block_qual = true; true }
            else { false }
        } {}

        if self.match_keyword(KeywordKind::Unsigned) { t.prim = TypePrim::Unsigned; t.is_unsigned = true; }
        else if self.match_keyword(KeywordKind::Signed) { t.prim = TypePrim::Signed; }

        if self.match_keyword(KeywordKind::Long) {
            if self.match_keyword(KeywordKind::Long) { t.prim = TypePrim::LongLong; }
            else if t.prim == TypePrim::Void || t.prim == TypePrim::Named { t.prim = TypePrim::Long; }
            else { t.prim = TypePrim::Long; }
        }
        if self.match_keyword(KeywordKind::Short) { t.prim = TypePrim::Short; }

        if t.prim == TypePrim::Signed || t.prim == TypePrim::Unsigned {
            if self.match_keyword(KeywordKind::Char) { t.prim = TypePrim::Char; }
            else if self.match_keyword(KeywordKind::Short) { t.prim = TypePrim::Short; }
            else if self.match_keyword(KeywordKind::Int) { t.prim = TypePrim::Int; }
            else if self.match_keyword(KeywordKind::Long) {
                t.prim = TypePrim::Long;
                if self.match_keyword(KeywordKind::Long) { t.prim = TypePrim::LongLong; }
            }
        } else if t.prim == TypePrim::Long || t.prim == TypePrim::LongLong {
            if self.match_keyword(KeywordKind::Int) {}
        } else if t.prim == TypePrim::Short {
            if self.match_keyword(KeywordKind::Int) {}
        }

        if t.prim != TypePrim::Signed && t.prim != TypePrim::Unsigned
            && t.prim != TypePrim::Long && t.prim != TypePrim::LongLong && t.prim != TypePrim::Short
            && t.prim != TypePrim::Char && t.prim != TypePrim::Int
            && t.prim != TypePrim::Float && t.prim != TypePrim::Double && t.prim != TypePrim::Bool
            && t.prim != TypePrim::Id && t.prim != TypePrim::Class && t.prim != TypePrim::Sel
            && t.prim != TypePrim::Instancetype
        {
            if self.match_keyword(KeywordKind::Void) { t.prim = TypePrim::Void; }
            else if self.match_keyword(KeywordKind::Char) { t.prim = TypePrim::Char; }
            else if self.match_keyword(KeywordKind::Int) { t.prim = TypePrim::Int; }
            else if self.match_keyword(KeywordKind::Float) { t.prim = TypePrim::Float; }
            else if self.match_keyword(KeywordKind::Double) { t.prim = TypePrim::Double; }
            else if self.match_keyword(KeywordKind::Bool) { t.prim = TypePrim::Bool; }
            else if self.match_keyword(KeywordKind::Id) { t.prim = TypePrim::Id; }
            else if self.match_keyword(KeywordKind::Class) { t.prim = TypePrim::Class; }
            else if self.match_keyword(KeywordKind::Sel) { t.prim = TypePrim::Sel; }
            else if self.match_keyword(KeywordKind::Instancetype) { t.prim = TypePrim::Instancetype; }
            else if self.match_keyword(KeywordKind::Struct) || self.match_keyword(KeywordKind::Union) || self.match_keyword(KeywordKind::Enum) {
                if self.current.kind == TokenKind::Identifier {
                    self.advance();
                    t.prim = TypePrim::Named;
                    t.is_struct = true;
                    t.name = Some(self.previous_text().to_string());
                }
            }
            else if self.current.kind == TokenKind::Identifier {
                let tname = self.current_text().to_string();
                if self.is_type_param(&tname) {
                    self.advance();
                    t.prim = TypePrim::Param;
                    t.name = Some(tname);
                } else {
                    t.prim = TypePrim::Named;
                    t.name = self.parse_qualified_name();
                    if t.name.is_none() { return None; }
                }
            } else {
                return None;
            }
        }

        Some(t)
    }

    fn parse_type_full(&mut self) -> Option<CstType> {
        let mut t = self.parse_type_name()?;

        // Protocol qualifiers or generic type args: <...>
        if matches!(t.prim, TypePrim::Id | TypePrim::Named | TypePrim::Class | TypePrim::Instancetype) {
            if self.match_token(TokenKind::Less) {
                let mut is_protocol = false;
                if self.current.kind == TokenKind::Identifier {
                    let mut is_generic = false;
                    if t.prim == TypePrim::Id {
                        is_generic = false; // id<P> always protocols
                    } else if let Some(ref name) = t.name {
                        // Check both the fully qualified name and the simple name
                        // (e.g. "System::IO::Buffer" should match registered "Buffer")
                        is_generic = self.is_generic_class(name)
                            || name.rsplit("::").next().map_or(false, |s| self.is_generic_class(s));
                    }
                    if !is_generic {
                        // Protocols path: <Proto1, Proto2>
                        let mut protocols = Vec::new();
                        let mut protocol_ok = true;
                        loop {
                            if self.current.kind != TokenKind::Identifier {
                                protocol_ok = false;
                                break;
                            }
                            protocols.push(self.current_text().to_string());
                            self.advance();
                            if !self.match_token(TokenKind::Comma) { break; }
                        }
                        if protocol_ok && self.current.kind == TokenKind::Greater {
                            self.advance();
                            if t.prim == TypePrim::Id || t.prim == TypePrim::Named ||
                               t.prim == TypePrim::Class || t.prim == TypePrim::Instancetype {
                                t.protocols = protocols;
                                is_protocol = true;
                            }
                        }
                    }
                }
                if !is_protocol {
                    let mut type_args = Vec::new();
                    loop {
                        if let Some(arg) = self.parse_type_full() {
                            type_args.push(arg);
                        } else {
                            while self.current.kind != TokenKind::Eof &&
                                  self.current.kind != TokenKind::Comma &&
                                  self.current.kind != TokenKind::Greater {
                                self.advance();
                            }
                            if self.current.kind == TokenKind::Eof { break; }
                        }
                        if !self.match_token(TokenKind::Comma) { break; }
                    }
                    if self.current.kind == TokenKind::Greater {
                        self.advance();
                    } else {
                        self.error("expected '>' after type arguments");
                        while self.current.kind != TokenKind::Eof &&
                              self.current.kind != TokenKind::Greater {
                            self.advance();
                        }
                        if self.current.kind == TokenKind::Greater { self.advance(); }
                    }
                    t.type_args = type_args;
                }
            }
        }

        // Pointer *
        while self.match_token(TokenKind::Star) {
            let mut ptr = CstType::new(t.prim);
            ptr.is_pointer = true;
            ptr.name = t.name.clone();
            ptr.is_struct = t.is_struct;
            ptr.protocols = std::mem::take(&mut t.protocols);
            ptr.type_args = std::mem::take(&mut t.type_args);
            ptr.subtype = Some(Box::new(t));
            t = ptr;
        }

        // Block type: T (^)(params) or T (^name)(params)
        if self.match_token(TokenKind::LParen) {
            if self.match_token(TokenKind::Caret) {
                let mut bt = CstType::new(t.prim);
                bt.is_block = true;
                bt.subtype = Some(Box::new(t));
                if self.current.kind == TokenKind::Identifier {
                    bt.block_name = Some(self.current_text().to_string());
                    self.advance();
                }
                self.consume(TokenKind::RParen, "expected ')' after ^");
                if self.match_token(TokenKind::LParen) {
                    let mut params: Vec<CstType> = Vec::new();
                    while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                        if let Some(ptype) = self.parse_type_full() {
                            if self.current.kind == TokenKind::Identifier {
                                self.advance();
                            }
                            params.push(ptype);
                        } else {
                            self.advance();
                        }
                        if !self.match_token(TokenKind::Comma) { break; }
                    }
                    // Link params via next
                    let mut head = None;
                    let mut tail: &mut Option<Box<CstType>> = &mut head;
                    for p in params {
                        let boxed = Box::new(p);
                        tail = &mut tail.insert(boxed).next;
                    }
                    bt.block_params = head;
                    self.consume(TokenKind::RParen, "expected ')' after block param list");
                }
                t = bt;
            } else {
                self.consume(TokenKind::RParen, "expected ')' after function type");
            }
        }

        Some(t)
    }

    // ─── Qualified name parsing ──────────────────────────────────────────

    fn parse_qualified_name(&mut self) -> Option<String> {
        if self.current.kind != TokenKind::Identifier && self.current.kind != TokenKind::Keyword {
            return None;
        }
        let mut name = String::new();
        if self.match_name() {
            name.push_str(self.previous_text());
        } else {
            return None;
        }
        while self.match_token(TokenKind::ColonColon) {
            name.push_str("::");
            if self.match_name() {
                name.push_str(self.previous_text());
            } else {
                break;
            }
        }
        Some(name)
    }

    // Render a CstType back into a fully-qualified source-level type string,
    // including protocol qualifiers (`id<P>`) and generic type arguments
    // (`Name<T*>`). Used by `@using Alias = FQN` so aliases can target
    // protocol-qualified ids and generic instantiations.
    fn type_to_fqn(t: &CstType) -> String {
        let mut s = String::new();
        match t.prim {
            TypePrim::Void => s.push_str("void"),
            TypePrim::Char => s.push_str("char"),
            TypePrim::Short => s.push_str("short"),
            TypePrim::Int => s.push_str("int"),
            TypePrim::Long => s.push_str("long"),
            TypePrim::LongLong => s.push_str("long long"),
            TypePrim::Float => s.push_str("float"),
            TypePrim::Double => s.push_str("double"),
            TypePrim::Bool => s.push_str("_Bool"),
            TypePrim::Signed => s.push_str("signed"),
            TypePrim::Unsigned => s.push_str("unsigned"),
            TypePrim::Id => s.push_str("id"),
            TypePrim::Class => s.push_str("Class"),
            TypePrim::Sel => s.push_str("SEL"),
            TypePrim::Instancetype => s.push_str("instancetype"),
            TypePrim::Named | TypePrim::Param => {
                if let Some(ref n) = t.name {
                    s.push_str(n);
                }
            }
        }
        if !t.protocols.is_empty() {
            s.push('<');
            for (i, p) in t.protocols.iter().enumerate() {
                if i > 0 { s.push_str(", "); }
                s.push_str(p);
            }
            s.push('>');
        }
        if !t.type_args.is_empty() {
            s.push('<');
            for (i, a) in t.type_args.iter().enumerate() {
                if i > 0 { s.push_str(", "); }
                s.push_str(&Self::type_to_fqn(a));
            }
            s.push('>');
        }
        if t.is_pointer { s.push('*'); }
        s
    }

    fn parse_qualified_name_from(&mut self, receiver: &CstExpr) -> Option<String> {
        let mut name = String::new();
        if let CstExprData::Ident(ref ident) = receiver.data {
            name.push_str(ident);
        } else {
            return None;
        }
        while self.match_token(TokenKind::ColonColon) {
            name.push_str("::");
            if self.match_name() {
                name.push_str(self.previous_text());
            } else {
                break;
            }
        }
        Some(name)
    }

    // ─── Expression parsing ──────────────────────────────────────────────

    fn parse_primary(&mut self) -> Option<CstExpr> {
        if self.match_token(TokenKind::Identifier) {
            let text = self.previous_text().to_string();
            let line = self.previous.line;
            let col = self.previous.column;
            if self.current.kind == TokenKind::ColonColon {
                let ident_expr = CstExpr {
                    kind: CstExprKind::Ident, expr_type: None,
                    line, col,
                    data: CstExprData::Ident(text.clone()),
                };
                if let Some(qn) = self.parse_qualified_name_from(&ident_expr) {
                    return Some(CstExpr {
                        kind: CstExprKind::Ident, expr_type: None,
                        line, col,
                        data: CstExprData::Ident(qn),
                    });
                }
            }
            return Some(CstExpr {
                kind: CstExprKind::Ident,
                expr_type: None,
                line, col,
                data: CstExprData::Ident(text),
            });
        }

        if self.current.kind == TokenKind::Keyword {
            let kw = self.current.keyword;
            if kw == KeywordKind::Id || kw == KeywordKind::Class ||
               kw == KeywordKind::Sel || kw == KeywordKind::Instancetype {
                self.advance();
                let text = self.previous_text().to_string();
                return Some(CstExpr {
                    kind: CstExprKind::Ident,
                    expr_type: None,
                    line: self.previous.line,
                    col: self.previous.column,
                    data: CstExprData::Ident(text),
                });
            }
        }

        if self.match_keyword(KeywordKind::Self_) {
            return Some(CstExpr {
                kind: CstExprKind::Self_, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Ident("self".into()),
            });
        }
        if self.match_keyword(KeywordKind::Super) {
            return Some(CstExpr {
                kind: CstExprKind::Super, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Ident("super".into()),
            });
        }
        if self.match_keyword(KeywordKind::Cmd) {
            return Some(CstExpr {
                kind: CstExprKind::Cmd, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Ident("_cmd".into()),
            });
        }
        if self.match_keyword(KeywordKind::Nil) {
            return Some(CstExpr {
                kind: CstExprKind::Nil, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Ident("nil".into()),
            });
        }
        if self.match_keyword(KeywordKind::Null) {
            return Some(CstExpr {
                kind: CstExprKind::Null, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Ident("NULL".into()),
            });
        }
        if self.match_keyword(KeywordKind::Yes) || self.match_keyword(KeywordKind::No) {
            let val = self.previous.keyword == KeywordKind::Yes;
            return Some(CstExpr {
                kind: CstExprKind::Bool, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Bool(val),
            });
        }
        if self.match_token(TokenKind::Integer) {
            let text = self.previous_text();
            let val = if text.len() > 2 && (text.starts_with("0x") || text.starts_with("0X")) {
                i64::from_str_radix(&text[2..], 16).unwrap_or(0)
            } else {
                text.parse::<i64>().unwrap_or(0)
            };
            return Some(CstExpr {
                kind: CstExprKind::Integer, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Integer(val),
            });
        }
        if self.match_token(TokenKind::Float) {
            let text = self.previous_text().trim_end_matches(|c: char| c == 'f' || c == 'F');
            let val = text.parse::<f64>().unwrap_or(0.0);
            return Some(CstExpr {
                kind: CstExprKind::Float, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Float(val),
            });
        }
        if self.match_token(TokenKind::String) {
            let text = self.previous_text().to_string();
            return Some(CstExpr {
                kind: CstExprKind::String, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::String(text),
            });
        }
        if self.match_token(TokenKind::Char) {
            return Some(CstExpr {
                kind: CstExprKind::Char, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Char(self.previous.char_val),
            });
        }

        // @selector(...)
        if self.match_keyword(KeywordKind::AtSelector) {
            self.consume(TokenKind::LParen, "expected '(' after @selector");
            let mut sel = String::new();
            while self.current.kind == TokenKind::Identifier ||
                  (self.current.kind == TokenKind::Keyword && !matches!(self.current.keyword,
                      KeywordKind::AtInterface | KeywordKind::AtImplementation | KeywordKind::AtEnd |
                      KeywordKind::AtProperty | KeywordKind::AtSynthesize | KeywordKind::AtDynamic |
                      KeywordKind::AtSelector | KeywordKind::AtEncode | KeywordKind::AtProtocol |
                      KeywordKind::AtOptional | KeywordKind::AtRequired | KeywordKind::AtClass |
                      KeywordKind::AtTry | KeywordKind::AtCatch | KeywordKind::AtFinally |
                      KeywordKind::AtThrow | KeywordKind::AtSynchronized | KeywordKind::AtAutoreleasepool |
                      KeywordKind::AtPublic | KeywordKind::AtPackage | KeywordKind::AtProtected |
                      KeywordKind::AtPrivate | KeywordKind::AtDefs | KeywordKind::AtNamespace |
                      KeywordKind::AtUsing | KeywordKind::Self_ | KeywordKind::Super |
                      KeywordKind::Return | KeywordKind::If | KeywordKind::Else |
                      KeywordKind::Switch | KeywordKind::Case | KeywordKind::Default |
                      KeywordKind::While | KeywordKind::Do | KeywordKind::For |
                      KeywordKind::Break | KeywordKind::Continue | KeywordKind::Goto |
                      KeywordKind::Sizeof | KeywordKind::Typeof | KeywordKind::Typedef |
                      KeywordKind::Struct | KeywordKind::Union | KeywordKind::Enum |
                      KeywordKind::Const | KeywordKind::Volatile | KeywordKind::Extern |
                      KeywordKind::Static | KeywordKind::Auto | KeywordKind::Register |
                      KeywordKind::Inline | KeywordKind::Restrict |
                      KeywordKind::Imp | KeywordKind::NpZone |
                      KeywordKind::Import | KeywordKind::Include | KeywordKind::Define |
                      KeywordKind::Ifdef | KeywordKind::Ifndef | KeywordKind::Endif |
                      KeywordKind::Pragma | KeywordKind::Elif | KeywordKind::Undef
                  )) {
                self.advance();
                sel.push_str(self.previous_text());
                if self.current.kind == TokenKind::Colon {
                    self.advance();
                    sel.push(':');
                } else {
                    break;
                }
            }
            self.consume(TokenKind::RParen, "expected ')' after @selector");
            return Some(CstExpr {
                kind: CstExprKind::Selector, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Selector(sel),
            });
        }

        // @protocol(...)
        if self.match_keyword(KeywordKind::AtProtocol) {
            self.consume(TokenKind::LParen, "expected ( after @protocol");
            let mut proto = String::new();
            if self.current.kind == TokenKind::Identifier {
                self.advance();
                proto = self.previous_text().to_string();
            }
            self.consume(TokenKind::RParen, "expected ) after @protocol");
            return Some(CstExpr {
                kind: CstExprKind::Protocol, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Protocol(proto),
            });
        }

        // @encode(...)
        if self.match_keyword(KeywordKind::AtEncode) {
            self.consume(TokenKind::LParen, "expected ( after @encode");
            let ty = self.parse_type_full().unwrap_or_else(|| CstType::new(TypePrim::Void));
            self.consume(TokenKind::RParen, "expected ) after @encode");
            return Some(CstExpr {
                kind: CstExprKind::Encode, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Encode(ty),
            });
        }

        // ( expression ) or (type)cast
        if self.match_token(TokenKind::LParen) {
            let is_cast = {
                if self.current.kind == TokenKind::Keyword {
                    let kw = self.current.keyword;
                    matches!(kw, KeywordKind::Int | KeywordKind::Char | KeywordKind::Short |
                        KeywordKind::Long | KeywordKind::Float | KeywordKind::Double |
                        KeywordKind::Void | KeywordKind::Bool | KeywordKind::Signed |
                        KeywordKind::Unsigned | KeywordKind::Const |
                        KeywordKind::Id | KeywordKind::Class | KeywordKind::Sel |
                        KeywordKind::Instancetype | KeywordKind::Struct |
                        KeywordKind::Union | KeywordKind::Enum)
                } else if self.current.kind == TokenKind::Identifier {
                    let tname = self.current_text().to_string();
                    // Check if this is a qualified name (Namespace::Type) or a known type name
                    self.is_type_name(&tname) || self.peek_colon_colon()
                } else {
                    false
                }
            };
            if is_cast {
                if let Some(ct) = self.parse_type_full() {
                    if self.match_token(TokenKind::RParen) {
                        let expr = self.parse_unary();
                        return Some(CstExpr {
                            kind: CstExprKind::Cast, expr_type: None,
                            line: self.previous.line, col: self.previous.column,
                            data: CstExprData::Cast { target_type: ct, expr: Box::new(expr.unwrap_or_else(||
                                CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) }
                            ))},
                        });
                    }
                }
            }
            let expr = self.parse_expression();
            self.consume(TokenKind::RParen, "expected ')' after expression");
            return expr.map(|e| CstExpr {
                kind: CstExprKind::Paren, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Paren(Box::new(e)),
            });
        }

        // Block literal: ^(params) { ... }
        if self.match_token(TokenKind::Caret) {
            return self.parse_block_literal();
        }

        // Message send [receiver ...]
        if self.match_token(TokenKind::LBracket) {
            // The receiver may be a generic-instantiated type expression like
            // `VectorBuffer<RenderPoint2D*>` in `[[VectorBuffer<RenderPoint2D*> alloc] init]`.
            // parse_expression() would treat `<` as a comparison operator, so try
            // parse_type_full first; if it parses a type with generic args and the
            // next token is an identifier (the start of a selector), treat the
            // rendered type string as the receiver identifier. Otherwise rewind
            // both lexer and current token so parse_expression handles it normally.
            let mut receiver = None;
            let saved_lex = self.lexer.save_pos();
            let saved_tok = self.current.clone();
            if self.current.kind == TokenKind::Identifier {
                if let Some(t) = self.parse_type_full() {
                    if (!t.type_args.is_empty() || !t.protocols.is_empty())
                        && self.current.kind == TokenKind::Identifier {
                        // Type receiver confirmed — build ident expr from rendered type.
                        // For protocols-only (e.g. NPObject<P>), use base class name
                        // to avoid codegen interpreting <P> as generic args.
                        let rstr = if !t.type_args.is_empty() {
                            Self::type_to_fqn(&t)
                        } else {
                            t.name.clone().unwrap_or_else(|| Self::type_to_fqn(&t))
                        };
                        receiver = Some(CstExpr {
                            kind: CstExprKind::Ident, expr_type: None,
                            line: self.previous.line, col: self.previous.column,
                            data: CstExprData::Ident(rstr),
                        });
                    }
                }
                if receiver.is_none() {
                    // Rewind lexer + current token to the saved position.
                    self.lexer.restore_pos(saved_lex);
                    self.current = saved_tok;
                }
            }
            if receiver.is_none() {
                receiver = self.parse_expression();
            }
            if let Some(ref mut r) = receiver {
                if r.kind == CstExprKind::Ident && self.current.kind == TokenKind::ColonColon {
                    if let Some(qn) = self.parse_qualified_name_from(r) {
                        r.data = CstExprData::Ident(qn);
                    }
                }
            }
            let sel_start = self.current.start;
            let mut selector = String::new();
            let mut args = Vec::new();
            let mut has_args = false;
            while self.current.kind == TokenKind::Identifier ||
                  (self.current.kind == TokenKind::Keyword && !matches!(self.current.keyword,
                      // @-structural directives
                      KeywordKind::AtInterface | KeywordKind::AtImplementation | KeywordKind::AtEnd |
                      KeywordKind::AtProperty | KeywordKind::AtSynthesize | KeywordKind::AtDynamic |
                      KeywordKind::AtSelector | KeywordKind::AtEncode | KeywordKind::AtProtocol |
                      KeywordKind::AtOptional | KeywordKind::AtRequired | KeywordKind::AtClass |
                      KeywordKind::AtTry | KeywordKind::AtCatch | KeywordKind::AtFinally |
                      KeywordKind::AtThrow | KeywordKind::AtSynchronized | KeywordKind::AtAutoreleasepool |
                      KeywordKind::AtPublic | KeywordKind::AtPackage | KeywordKind::AtProtected |
                      KeywordKind::AtPrivate | KeywordKind::AtDefs | KeywordKind::AtNamespace |
                      KeywordKind::AtUsing |
                      // Flow control / declaration keywords (never selectors)
                      KeywordKind::Return | KeywordKind::If | KeywordKind::Else |
                      KeywordKind::Switch | KeywordKind::Case | KeywordKind::Default |
                      KeywordKind::While | KeywordKind::Do | KeywordKind::For |
                      KeywordKind::Break | KeywordKind::Continue | KeywordKind::Goto |
                      KeywordKind::Sizeof | KeywordKind::Typeof | KeywordKind::Typedef |
                      KeywordKind::Struct | KeywordKind::Union | KeywordKind::Enum |
                      KeywordKind::Const | KeywordKind::Volatile | KeywordKind::Extern |
                      KeywordKind::Static | KeywordKind::Auto | KeywordKind::Register |
                      KeywordKind::Inline | KeywordKind::Restrict |
                      KeywordKind::Imp | KeywordKind::NpZone |
                      KeywordKind::Import | KeywordKind::Include | KeywordKind::Define |
                      KeywordKind::Ifdef | KeywordKind::Ifndef | KeywordKind::Endif |
                      KeywordKind::Pragma | KeywordKind::Elif | KeywordKind::Undef
                  ))
            {
                has_args = true;
                let kw = self.current.keyword;
                if kw == KeywordKind::Id || kw == KeywordKind::Class ||
                   kw == KeywordKind::Sel || kw == KeywordKind::Instancetype {
                    // These keywords can be part of a selector
                }
                self.advance();
                let part = self.previous_text().to_string();
                selector.push_str(&part);
                if self.current.kind == TokenKind::Colon {
                    self.advance();
                    selector.push(':');
                    let arg = self.parse_assignment();
                    if let Some(a) = arg {
                        args.push(a);
                    }
                } else if !args.is_empty() {
                    args.push(CstExpr {
                        kind: CstExprKind::Ident, expr_type: None,
                        line: 0, col: 0,
                        data: CstExprData::Ident(part),
                    });
                }
                if self.current.kind == TokenKind::Comma && self.current.kind != TokenKind::RBracket {
                    // Nope, commas don't separate message parts
                    break;
                }
            }
            // If no args were parsed, it's a zero-arg message
            self.consume(TokenKind::RBracket, "expected ']' after message send");
            return Some(CstExpr {
                kind: CstExprKind::MessageSend, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Message {
                    receiver: Box::new(receiver.unwrap_or_else(||
                        CstExpr { kind: CstExprKind::Ident, expr_type: None, line: 0, col: 0, data: CstExprData::Ident("self".into()) }
                    )),
                    selector,
                    args,
                },
            });
        }

        // @[...] array literal
        if self.match_token(TokenKind::LBracket) {
            // This is the @[ case — the @ was consumed as LBracket, now [ is the next token
            // Actually, @[ produces LBracket (for @), then LBracket (for [).
            // So we've already consumed the @ via LBracket, and now we need to consume [.
            // But wait: the @ was consumed as a LBracket, and [ is the next token as LBracket.
            // Let me re-check: the lexer for @[ returns LBracket (for @) with length 1.
            // Then next call to next() returns LBracket (for [).
            // So match_token(LBracket) consumed the @, and now we need to consume the [.
            // But wait, match_token(LBracket) already consumed one token. If we're here,
            // it means the token was an LBracket. But @[ produces TWO LBrackets.
            // So we need to handle this differently.
            // Actually, let me re-think: the parser sees @[ as two tokens: LBracket (for @) and LBracket (for [).
            // The parser's @[] handling should consume BOTH.
            // But we already consumed one LBracket to get here. Let me consume the second one.
            // Hmm, actually I think this is wrong. Let me just handle array literals differently.
            // The @[ case: the lexer outputs LBracket (for @), then LBracket (for [).
            // The parser hits this code when it sees LBracket in parse_primary.
            // But we already consumed the first LBracket (the @).
            // Now we need to consume the second LBracket (the [).
            // Let me just check: is the current token LBracket?
            if self.current.kind == TokenKind::LBracket {
                self.advance(); // consume the [
            }

            let mut elements = Vec::new();
            while !self.check(TokenKind::RBracket) && !self.check(TokenKind::Eof) {
                if let Some(e) = self.parse_assignment() {
                    elements.push(e);
                }
                if !self.match_token(TokenKind::Comma) { break; }
            }
            self.consume(TokenKind::RBracket, "expected ']' after array literal");
            return Some(CstExpr {
                kind: CstExprKind::ArrayLit, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::ArrayLit(elements),
            });
        }

        if self.match_token(TokenKind::LBrace) {
            return self.parse_init_list_or_dict();
        }

        // @{...} dictionary literal — @{ produces LBrace, then { produces LBrace
        if self.match_token(TokenKind::LBrace) {
            // Handle the @{ case
            if self.current.kind == TokenKind::LBrace {
                self.advance();
            }
            let mut keys = Vec::new();
            let mut values = Vec::new();
            let mut is_dict = false;
            while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
                if let Some(k) = self.parse_assignment() {
                    if self.match_token(TokenKind::Colon) {
                        is_dict = true;
                        if let Some(v) = self.parse_expression() {
                            keys.push(k);
                            values.push(v);
                        }
                    } else {
                        if !is_dict {
                            keys.push(k);
                        }
                    }
                }
                if !self.match_token(TokenKind::Comma) { break; }
            }
            self.consume(TokenKind::RBrace, "expected '}' after literal");
            if is_dict {
                return Some(CstExpr {
                    kind: CstExprKind::DictLit, expr_type: None,
                    line: self.previous.line, col: self.previous.column,
                    data: CstExprData::DictLit { keys, values },
                });
            }
            return Some(CstExpr {
                kind: CstExprKind::ArrayLit, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::ArrayLit(keys),
            });
        }

        // @(number) — @( produces LParen (for @), then LParen (for ()
        // Actually same pattern: @( is LParen, then LParen
        // We handle this in parse_expression normally

        None
    }

    fn parse_init_list_or_dict(&mut self) -> Option<CstExpr> {
        let mut elements = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
            if let Some(e) = self.parse_assignment() {
                elements.push(e);
            }
            if !self.match_token(TokenKind::Comma) { break; }
        }
        self.consume(TokenKind::RBrace, "expected '}'");
        Some(CstExpr {
            kind: CstExprKind::InitList, expr_type: None,
            line: self.previous.line, col: self.previous.column,
            data: CstExprData::InitList(elements),
        })
    }

    fn parse_block_literal(&mut self) -> Option<CstExpr> {
        let mut params: Option<Box<CstParam>> = None;
        let mut param_count = 0;
        let mut return_type: Option<Box<CstType>> = None;

        // Optional return type: ^returnType(params) { body }
        // Use parse_type_name (not parse_type_full) to avoid consuming ( as block/function type
        if self.current.kind == TokenKind::Keyword &&
            matches!(self.current.keyword, KeywordKind::Void | KeywordKind::Int |
                KeywordKind::Char | KeywordKind::Short | KeywordKind::Long |
                KeywordKind::Float | KeywordKind::Double | KeywordKind::Bool |
                KeywordKind::Signed | KeywordKind::Unsigned | KeywordKind::Id |
                KeywordKind::Class | KeywordKind::Sel | KeywordKind::Instancetype) {
            return_type = self.parse_type_name().map(Box::new);
        } else if self.current.kind == TokenKind::Identifier {
            return_type = self.parse_type_full().map(Box::new);
        }

        if self.match_token(TokenKind::LParen) {
            // parse params: ^int(int x, float y) or ^(int x, float y)
            let mut head: Option<Box<CstParam>> = None;
            let mut tail: &mut Option<Box<CstParam>> = &mut head;
            while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                if let Some(ptype) = self.parse_type_full() {
                    let mut p = CstParam {
                        par_type: Some(Box::new(ptype)),
                        name: None,
                        external_name: None,
                        next: None,
                    };
                    if self.current.kind == TokenKind::Identifier {
                        p.name = Some(self.current_text().to_string());
                        self.advance();
                    }
                    param_count += 1;
                    tail = &mut tail.insert(Box::new(p)).next;
                } else {
                    self.advance();
                }
                if !self.match_token(TokenKind::Comma) { break; }
            }
            self.consume(TokenKind::RParen, "expected ')' after block params");
            params = head;
        }

        // Return type: ^(int x, float y) -> int { ... } or ^(void) { ... }
        // In ObjC, block return type is inferred or specified as ^int(^)(void)
        // Skip for now

        if self.check(TokenKind::LBrace) {
            let body = self.parse_compound_statement();
            return Some(CstExpr {
                kind: CstExprKind::Block, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Block {
                    params,
                    param_count,
                    return_type,
                    body: body.map(Box::new),
                },
            });
        }

        Some(CstExpr {
            kind: CstExprKind::Block, expr_type: None,
            line: self.previous.line, col: self.previous.column,
            data: CstExprData::Block {
                params,
                param_count,
                return_type,
                body: None,
            },
        })
    }

    fn parse_postfix(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_primary()?;
        loop {
            // Namespace qualifier: expr::name (e.g. Engine::Core::WeaponSystem)
            if self.match_token(TokenKind::ColonColon) {
                if self.current.kind == TokenKind::Identifier ||
                   (self.current.kind == TokenKind::Keyword && matches!(self.current.keyword, KeywordKind::Id | KeywordKind::Class | KeywordKind::Sel | KeywordKind::Instancetype)) {
                    let name = self.current_text().to_string();
                    self.advance();
                    // Build qualified name from the previous expression + :: + name
                    let prev_name = match &expr.data {
                        CstExprData::Ident(s) => s.clone(),
                        _ => String::new(),
                    };
                    let qualified = if prev_name.is_empty() { name } else { format!("{}::{}", prev_name, name) };
                    expr = CstExpr {
                        kind: CstExprKind::Ident, expr_type: None,
                        line: expr.line, col: expr.col,
                        data: CstExprData::Ident(qualified),
                    };
                    continue; // Check for more :: qualifiers
                } else {
                    break;
                }
            }
            // Dot access: expr.property
            if self.match_token(TokenKind::Dot) {
                if self.current.kind == TokenKind::Identifier ||
                   (self.current.kind == TokenKind::Keyword && matches!(self.current.keyword, KeywordKind::Id | KeywordKind::Class | KeywordKind::Sel | KeywordKind::Instancetype)) {
                    let prop = self.current_text().to_string();
                    self.advance();
                    expr = CstExpr {
                        kind: CstExprKind::DotAccess, expr_type: None,
                        line: expr.line, col: expr.col,
                        data: CstExprData::Dot {
                            object: Box::new(expr),
                            property: prop,
                        },
                    };
                } else {
                    break;
                }
            }
            // Subscript: expr[key]
            else if self.match_token(TokenKind::LBracket) {
                let key = self.parse_expression();
                self.consume(TokenKind::RBracket, "expected ']' after subscript");
                expr = CstExpr {
                    kind: CstExprKind::Subscript, expr_type: None,
                    line: expr.line, col: expr.col,
                    data: CstExprData::Subscript {
                        object: Box::new(expr),
                        key: Box::new(key.unwrap_or_else(||
                            CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) }
                        )),
                    },
                };
            }
            // Arrow access: expr->property
            else if self.match_token(TokenKind::Arrow) {
                if self.current.kind == TokenKind::Identifier ||
                   (self.current.kind == TokenKind::Keyword && matches!(self.current.keyword, KeywordKind::Id | KeywordKind::Class | KeywordKind::Sel | KeywordKind::Instancetype)) {
                    let prop = self.current_text().to_string();
                    self.advance();
                    expr = CstExpr {
                        kind: CstExprKind::Arrow, expr_type: None,
                        line: expr.line, col: expr.col,
                        data: CstExprData::Arrow {
                            object: Box::new(expr),
                            property: prop,
                        },
                    };
                } else {
                    break;
                }
            }
            // Function call: expr(args)
            else if self.match_token(TokenKind::LParen) {
                let mut args = Vec::new();
                while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                    if let Some(a) = self.parse_assignment() {
                        args.push(a);
                    }
                    if !self.match_token(TokenKind::Comma) { break; }
                }
                self.consume(TokenKind::RParen, "expected ')' after args");
                expr = CstExpr {
                    kind: CstExprKind::Call, expr_type: None,
                    line: expr.line, col: expr.col,
                    data: CstExprData::Call {
                        callee: Box::new(expr),
                        args,
                    },
                };
            }
            // Postfix ++/--
            else if self.match_token(TokenKind::Incr) {
                expr = CstExpr {
                    kind: CstExprKind::Unary, expr_type: None,
                    line: expr.line, col: expr.col,
                    data: CstExprData::Unary {
                        op: 1, // ++
                        operand: Box::new(expr),
                        is_postfix: true,
                    },
                };
            }
            else if self.match_token(TokenKind::Decr) {
                expr = CstExpr {
                    kind: CstExprKind::Unary, expr_type: None,
                    line: expr.line, col: expr.col,
                    data: CstExprData::Unary {
                        op: 2, // --
                        operand: Box::new(expr),
                        is_postfix: true,
                    },
                };
            } else {
                break;
            }
        }
        Some(expr)
    }

    fn parse_unary(&mut self) -> Option<CstExpr> {
        if self.match_token(TokenKind::Incr) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 1, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Decr) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 2, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Star) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 3, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Ampersand) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 4, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Minus) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 5, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Plus) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 6, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Tilde) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 7, operand: Box::new(operand), is_postfix: false },
            });
        }
        if self.match_token(TokenKind::Exclam) {
            let operand = self.parse_unary()?;
            return Some(CstExpr {
                kind: CstExprKind::Unary, expr_type: None,
                line: self.previous.line, col: self.previous.column,
                data: CstExprData::Unary { op: 8, operand: Box::new(operand), is_postfix: false },
            });
        }
        // sizeof / sizeof(type)
        if self.match_keyword(KeywordKind::Sizeof) {
            if self.match_token(TokenKind::LParen) {
                // Could be sizeof(type) or sizeof(expr)
                let saved = self.current.clone();
                if let Some(ty) = self.parse_type_full() {
                    if self.match_token(TokenKind::RParen) {
                        // Only accept as sizeof(type) if it's a real type (struct, or known type name)
                        let is_real_type = ty.is_struct ||
                            matches!(ty.prim, TypePrim::Void | TypePrim::Char | TypePrim::Short |
                                TypePrim::Int | TypePrim::Long | TypePrim::LongLong |
                                TypePrim::Float | TypePrim::Double | TypePrim::Bool |
                                TypePrim::Signed | TypePrim::Unsigned | TypePrim::Id |
                                TypePrim::Class | TypePrim::Sel | TypePrim::Instancetype) ||
                            (ty.prim == TypePrim::Named && ty.name.as_ref().map_or(false, |n| self.is_type_name(n)));
                        if is_real_type {
                            return Some(CstExpr {
                                kind: CstExprKind::Sizeof, expr_type: None,
                                line: self.previous.line, col: self.previous.column,
                                data: CstExprData::Sizeof { type_expr: ty, expr: None },
                            });
                        }
                        // Simple identifier that is not a known type — treat as sizeof(expr)
                        // Convert the parsed type name back to an identifier expression
                        if let Some(name) = ty.name {
                            let expr = CstExpr {
                                kind: CstExprKind::Ident, expr_type: None,
                                line: self.previous.line, col: self.previous.column,
                                data: CstExprData::Ident(name),
                            };
                            return Some(CstExpr {
                                kind: CstExprKind::Sizeof, expr_type: None,
                                line: self.previous.line, col: self.previous.column,
                                data: CstExprData::Sizeof { type_expr: CstType::new(TypePrim::Void), expr: Some(Box::new(expr)) },
                            });
                        }
                    }
                }
                // Not a type, parse as expression
                self.current = saved;
                self.advance(); // re-consume the ( we peeked at
                let expr = self.parse_expression();
                self.consume(TokenKind::RParen, "expected ')' after sizeof");
                return expr.map(|e| CstExpr {
                    kind: CstExprKind::Sizeof, expr_type: None,
                    line: self.previous.line, col: self.previous.column,
                    data: CstExprData::Sizeof { type_expr: CstType::new(TypePrim::Void), expr: Some(Box::new(e)) },
                });
            }
        }
        self.parse_postfix()
    }

    fn parse_multiplicative(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_unary()?;
        let ops = [TokenKind::Star, TokenKind::Slash, TokenKind::Percent];
        while let Some(op) = ops.iter().find(|o| self.match_token(**o)) {
            let right = self.parse_unary()?;
            let op_val = match op {
                TokenKind::Star => 1,
                TokenKind::Slash => 2,
                TokenKind::Percent => 3,
                _ => 0,
            };
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: op_val, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_additive(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_multiplicative()?;
        let ops = [TokenKind::Plus, TokenKind::Minus];
        while let Some(op) = ops.iter().find(|o| self.match_token(**o)) {
            let right = self.parse_multiplicative()?;
            let op_val = match op {
                TokenKind::Plus => 4,
                TokenKind::Minus => 5,
                _ => 0,
            };
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: op_val, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_shift(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_additive()?;
        let ops = [TokenKind::LShift, TokenKind::RShift];
        while let Some(op) = ops.iter().find(|o| self.match_token(**o)) {
            let right = self.parse_additive()?;
            let op_val = match op {
                TokenKind::LShift => 6,
                TokenKind::RShift => 7,
                _ => 0,
            };
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: op_val, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_relational(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_shift()?;
        let ops = [TokenKind::Less, TokenKind::Greater, TokenKind::Leq, TokenKind::Geq];
        while let Some(op) = ops.iter().find(|o| self.match_token(**o)) {
            let right = self.parse_shift()?;
            let op_val = match op {
                TokenKind::Less => 8,
                TokenKind::Greater => 9,
                TokenKind::Leq => 10,
                TokenKind::Geq => 11,
                _ => 0,
            };
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: op_val, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_equality(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_relational()?;
        let ops = [TokenKind::Eq, TokenKind::Neq];
        while let Some(op) = ops.iter().find(|o| self.match_token(**o)) {
            let right = self.parse_relational()?;
            let op_val = match op {
                TokenKind::Eq => 12,
                TokenKind::Neq => 13,
                _ => 0,
            };
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: op_val, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_bitwise_and(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_equality()?;
        while self.match_token(TokenKind::Ampersand) {
            let right = self.parse_equality()?;
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: 14, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_bitwise_xor(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_bitwise_and()?;
        while self.match_token(TokenKind::Caret) {
            let right = self.parse_bitwise_and()?;
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: 15, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_bitwise_or(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_bitwise_xor()?;
        while self.match_token(TokenKind::Pipe) {
            let right = self.parse_bitwise_xor()?;
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: 16, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_logical_and(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_bitwise_or()?;
        while self.match_token(TokenKind::LogicalAnd) {
            let right = self.parse_bitwise_or()?;
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: 17, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_logical_or(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_logical_and()?;
        while self.match_token(TokenKind::LogicalOr) {
            let right = self.parse_logical_and()?;
            expr = CstExpr {
                kind: CstExprKind::Binary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Binary { op: 18, left: Box::new(expr), right: Box::new(right) },
            };
        }
        Some(expr)
    }

    fn parse_conditional(&mut self) -> Option<CstExpr> {
        let mut expr = self.parse_logical_or()?;
        if self.match_token(TokenKind::Question) {
            let true_expr = self.parse_expression()?;
            self.consume(TokenKind::Colon, "expected ':' in ternary");
            let false_expr = self.parse_conditional()?;
            expr = CstExpr {
                kind: CstExprKind::Ternary, expr_type: None,
                line: expr.line, col: expr.col,
                data: CstExprData::Ternary {
                    cond: Box::new(expr),
                    true_expr: Box::new(true_expr),
                    false_expr: Box::new(false_expr),
                },
            };
        }
        Some(expr)
    }

    fn parse_assignment(&mut self) -> Option<CstExpr> {
        let expr = self.parse_conditional()?;
        let assign_ops = [
            TokenKind::Assign, TokenKind::PlusAssign, TokenKind::MinusAssign,
            TokenKind::StarAssign, TokenKind::SlashAssign, TokenKind::PercentAssign,
            TokenKind::AndAssign, TokenKind::OrAssign, TokenKind::XorAssign,
            TokenKind::LShiftAssign, TokenKind::RShiftAssign,
        ];
        for &op in &assign_ops {
            if self.match_token(op) {
                let value = self.parse_assignment()?;
                if op == TokenKind::Assign {
                    return Some(CstExpr {
                        kind: CstExprKind::Assign, expr_type: None,
                        line: expr.line, col: expr.col,
                        data: CstExprData::Assign { target: Box::new(expr), value: Box::new(value) },
                    });
                }
                let op_val = match op {
                    TokenKind::PlusAssign => 100, TokenKind::MinusAssign => 101,
                    TokenKind::StarAssign => 102, TokenKind::SlashAssign => 103,
                    TokenKind::PercentAssign => 104, TokenKind::AndAssign => 105,
                    TokenKind::OrAssign => 106, TokenKind::XorAssign => 107,
                    TokenKind::LShiftAssign => 108, TokenKind::RShiftAssign => 109,
                    _ => 0,
                };
                return Some(CstExpr {
                    kind: CstExprKind::Binary, expr_type: None,
                    line: expr.line, col: expr.col,
                    data: CstExprData::Binary { op: op_val, left: Box::new(expr), right: Box::new(value) },
                });
            }
        }
        Some(expr)
    }

    fn parse_expression(&mut self) -> Option<CstExpr> {
        let expr = self.parse_assignment()?;
        if self.match_token(TokenKind::Comma) {
            let line = expr.line;
            let col = expr.col;
            let mut exprs = vec![expr];
            while let Some(e) = self.parse_assignment() {
                exprs.push(e);
                if !self.match_token(TokenKind::Comma) { break; }
            }
            return Some(CstExpr {
                kind: CstExprKind::Comma, expr_type: None,
                line, col,
                data: CstExprData::Comma(exprs),
            });
        }
        Some(expr)
    }

    // ─── Statement parsing ──────────────────────────────────────────────

    fn parse_expression_statement(&mut self) -> Option<CstStmt> {
        let expr = self.parse_expression();
        self.consume(TokenKind::Semicolon, "expected ';' after expression");
        expr.map(|e| CstStmt {
            kind: CstStmtKind::Expr,
            line: e.line, column: e.col,
            data: CstStmtData::Expr(e),
        })
    }

    fn parse_compound_statement(&mut self) -> Option<CstStmt> {
        self.consume(TokenKind::LBrace, "expected '{'");
        let mut stmts = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
            if let Some(s) = self.parse_statement() {
                stmts.push(s);
            } else {
                self.advance();
            }
        }
        self.consume(TokenKind::RBrace, "expected '}'");
        Some(CstStmt {
            kind: CstStmtKind::Compound,
            line: self.previous.line, column: self.previous.column,
            data: CstStmtData::Compound(stmts),
        })
    }

    fn parse_statement(&mut self) -> Option<CstStmt> {
        // Jump statements
        if self.match_keyword(KeywordKind::Return) {
            let expr = if !self.check(TokenKind::Semicolon) && !self.check(TokenKind::RBrace) {
                self.parse_expression()
            } else { None };
            self.consume(TokenKind::Semicolon, "expected ';' after return");
            return Some(CstStmt {
                kind: CstStmtKind::Return,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Return(expr.map(Box::new)),
            });
        }
        if self.match_keyword(KeywordKind::Break) {
            self.consume(TokenKind::Semicolon, "expected ';' after break");
            return Some(CstStmt {
                kind: CstStmtKind::Break,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Return(None),
            });
        }
        if self.match_keyword(KeywordKind::Continue) {
            self.consume(TokenKind::Semicolon, "expected ';' after continue");
            return Some(CstStmt {
                kind: CstStmtKind::Continue,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Return(None),
            });
        }
        if self.match_keyword(KeywordKind::Goto) {
            let label = if self.current.kind == TokenKind::Identifier {
                let l = self.current_text().to_string();
                self.advance();
                l
            } else { String::new() };
            self.consume(TokenKind::Semicolon, "expected ';' after goto");
            return Some(CstStmt {
                kind: CstStmtKind::Goto,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Goto(label),
            });
        }
        if self.match_keyword(KeywordKind::AtThrow) {
            let expr = if !self.check(TokenKind::Semicolon) {
                self.parse_expression()
            } else { None };
            self.consume(TokenKind::Semicolon, "expected ';' after @throw");
            return Some(CstStmt {
                kind: CstStmtKind::Throw,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Throw(expr.map(Box::new)),
            });
        }

        // Compound statement
        if self.check(TokenKind::LBrace) {
            return self.parse_compound_statement();
        }

        // If
        if self.match_keyword(KeywordKind::If) {
            self.consume(TokenKind::LParen, "expected '(' after if");
            let cond = self.parse_expression().unwrap_or_else(||
                CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) }
            );
            self.consume(TokenKind::RParen, "expected ')' after if condition");
            let then_branch = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            let else_branch = if self.match_keyword(KeywordKind::Else) {
                self.parse_statement().map(Box::new)
            } else { None };
            return Some(CstStmt {
                kind: CstStmtKind::If,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::If { cond: Box::new(cond), then_branch, else_branch },
            });
        }

        // While
        if self.match_keyword(KeywordKind::While) {
            self.consume(TokenKind::LParen, "expected '(' after while");
            let cond = self.parse_expression().unwrap_or_else(||
                CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) }
            );
            self.consume(TokenKind::RParen, "expected ')' after while condition");
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::While,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::While { cond: Box::new(cond), body },
            });
        }

        // Do-while
        if self.match_keyword(KeywordKind::Do) {
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            self.consume_keyword(KeywordKind::While, "expected 'while' after do body");
            self.consume(TokenKind::LParen, "expected '(' after while");
            let cond = self.parse_expression().unwrap_or_else(||
                CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) }
            );
            self.consume(TokenKind::RParen, "expected ')' after while condition");
            self.consume(TokenKind::Semicolon, "expected ';' after do-while");
            return Some(CstStmt {
                kind: CstStmtKind::Do,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Do { body, cond: Box::new(cond) },
            });
        }

        // For
        if self.match_keyword(KeywordKind::For) {
            self.consume(TokenKind::LParen, "expected '(' after for");
            let init = if !self.check(TokenKind::Semicolon) {
                // Check for decl: type name = expr;
                if self.current.kind == TokenKind::Keyword &&
                   matches!(self.current.keyword,
                    KeywordKind::Int | KeywordKind::Char | KeywordKind::Float |
                    KeywordKind::Double | KeywordKind::Long | KeywordKind::Short |
                    KeywordKind::Void | KeywordKind::Bool | KeywordKind::Id |
                    KeywordKind::Class | KeywordKind::Sel | KeywordKind::Instancetype |
                    KeywordKind::Const | KeywordKind::Struct | KeywordKind::Union |
                    KeywordKind::Enum | KeywordKind::Signed | KeywordKind::Unsigned)
                {
                    let decl = self.parse_declaration();
                    decl.map(|d| CstStmt {
                        kind: CstStmtKind::Decl,
                        line: d.line, column: d.column,
                        data: CstStmtData::Decl(d),
                    })
                } else {
                    self.parse_expression_statement()
                }
            } else {
                self.advance();
                None
            };
            let cond = if !self.check(TokenKind::Semicolon) {
                let e = self.parse_expression();
                self.consume(TokenKind::Semicolon, "expected ';' after for condition");
                e
            } else {
                self.advance();
                None
            };
            let incr = if !self.check(TokenKind::RParen) {
                let e = self.parse_expression();
                self.consume(TokenKind::RParen, "expected ')' after for incr");
                e
            } else {
                self.advance();
                None
            };
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::For,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::For {
                    init: init.map(Box::new),
                    cond: cond.map(Box::new),
                    incr: incr.map(Box::new),
                    body,
                },
            });
        }

        // For-in
        if self.match_keyword(KeywordKind::For) {
            // Already handled above, but for-in pattern: for (Type var in collection)
            // This is handled by detecting the 'in' keyword after the expression
            // Not fully implemented yet
        }

        // Switch
        if self.match_keyword(KeywordKind::Switch) {
            self.consume(TokenKind::LParen, "expected '(' after switch");
            let expr = self.parse_expression().map(Box::new).unwrap_or_else(||
                Box::new(CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) })
            );
            self.consume(TokenKind::RParen, "expected ')' after switch expr");
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::Switch,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Switch { expr, body },
            });
        }

        // Case
        if self.match_keyword(KeywordKind::Case) {
            let value = self.parse_expression().map(Box::new).unwrap_or_else(||
                Box::new(CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) })
            );
            self.consume(TokenKind::Colon, "expected ':' after case value");
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::Case,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Case { value, body },
            });
        }

        // Default
        if self.match_keyword(KeywordKind::Default) {
            self.consume(TokenKind::Colon, "expected ':' after default");
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::Default,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Default(body),
            });
        }

        // @try/@catch/@finally
        if self.match_keyword(KeywordKind::AtTry) {
            let try_block = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            let mut catches = Vec::new();
            while self.match_keyword(KeywordKind::AtCatch) {
                let mut param = CstParam {
                    par_type: None, name: None, external_name: None, next: None,
                };
                if self.match_token(TokenKind::LParen) {
                    param.par_type = self.parse_type_full().map(Box::new);
                    if self.current.kind == TokenKind::Identifier {
                        param.name = Some(self.current_text().to_string());
                        self.advance();
                    }
                    self.consume(TokenKind::RParen, "expected ')' after @catch param");
                }
                let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                    Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
                );
                catches.push(CstStmt {
                    kind: CstStmtKind::Catch,
                    line: self.previous.line, column: self.previous.column,
                    data: CstStmtData::Catch { param, body },
                });
            }
            let finally_block = if self.match_keyword(KeywordKind::AtFinally) {
                self.parse_statement().map(Box::new)
            } else { None };
            return Some(CstStmt {
                kind: CstStmtKind::Try,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Try { try_block, catches, finally_block },
            });
        }

        // @synchronized
        if self.match_keyword(KeywordKind::AtSynchronized) {
            self.consume(TokenKind::LParen, "expected '(' after @synchronized");
            let lock = self.parse_expression().map(Box::new).unwrap_or_else(||
                Box::new(CstExpr { kind: CstExprKind::Integer, expr_type: None, line: 0, col: 0, data: CstExprData::Integer(0) })
            );
            self.consume(TokenKind::RParen, "expected ')' after @synchronized lock");
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::Synchronized,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Synchronized { lock, body },
            });
        }

        // @autoreleasepool
        if self.match_keyword(KeywordKind::AtAutoreleasepool) {
            let body = self.parse_statement().map(Box::new).unwrap_or_else(||
                Box::new(CstStmt { kind: CstStmtKind::Compound, line: 0, column: 0, data: CstStmtData::Compound(Vec::new()) })
            );
            return Some(CstStmt {
                kind: CstStmtKind::Autoreleasepool,
                line: self.previous.line, column: self.previous.column,
                data: CstStmtData::Autoreleasepool(body),
            });
        }

        // Label: identifier : (not ::)
        if self.current.kind == TokenKind::Identifier && self.peek_next() == TokenKind::Colon {
            let name = self.current_text().to_string();
            // Check if next is :: (namespace) vs : (label)
            let saved = self.current.start;
            self.advance();
            if self.current.kind == TokenKind::Colon && self.peek_next() != TokenKind::Colon {
                self.advance(); // consume :
                return Some(CstStmt {
                    kind: CstStmtKind::Label,
                    line: self.previous.line, column: self.previous.column,
                    data: CstStmtData::Label(name),
                });
            }
            // Not a label, rewind
            self.current = Token {
                kind: TokenKind::Identifier,
                keyword: KeywordKind::None,
                start: saved, length: name.len() as usize,
                line: self.previous.line, column: self.previous.column,
                char_val: 0,
            };
        }

        // Declaration
        if self.is_declaration_start() {
            let decl = self.parse_declaration();
            if let Some(d) = decl {
                return Some(CstStmt {
                    kind: CstStmtKind::Decl,
                    line: d.line, column: d.column,
                    data: CstStmtData::Decl(d),
                });
            }
        }

        // Expression statement
        self.parse_expression_statement()
    }

    fn peek_next(&self) -> TokenKind {
        // This is a simplified peekahead — in the real parser we'd need to save/restore
        // For now, just return current
        self.current.kind
    }

    fn is_declaration_start(&self) -> bool {
        if self.current.kind == TokenKind::Keyword {
            match self.current.keyword {
                KeywordKind::Int | KeywordKind::Char | KeywordKind::Float |
                KeywordKind::Double | KeywordKind::Long | KeywordKind::Short |
                KeywordKind::Void | KeywordKind::Bool |
                KeywordKind::Const | KeywordKind::Static | KeywordKind::Extern |
                KeywordKind::Struct | KeywordKind::Union | KeywordKind::Enum |
                KeywordKind::Typedef | KeywordKind::Signed | KeywordKind::Unsigned |
                KeywordKind::Volatile | KeywordKind::Auto | KeywordKind::Register |
                KeywordKind::Inline | KeywordKind::Restrict |
                KeywordKind::Block | KeywordKind::Weak | KeywordKind::Strong |
                KeywordKind::Autoreleasing | KeywordKind::UnsafeUnretained => true,
                // id/Class/Sel/Instancetype are type keywords that can also be variable names
                // Peek at next non-space char to disambiguate:
                //   id obj = ... → next char is identifier → declaration
                //   id = ...     → next char is '=' → expression (variable assignment)
                KeywordKind::Id | KeywordKind::Class | KeywordKind::Sel | KeywordKind::Instancetype => {
                    let after = self.current.start + self.current.length;
                    let rest = &self.source[after..];
                    let next = rest.trim_start().chars().next().unwrap_or(';');
                    // If followed by =, ;, ), , → expression (variable usage)
                    // If followed by *, [, (, <, identifier → declaration
                    matches!(next, '*' | '[' | '(' | '<' | '_' | 'a'..='z' | 'A'..='Z')
                }
                _ => false,
            }
        } else if self.current.kind == TokenKind::Identifier {
            let tname = self.current_text().to_string();
            // Check if it's a known type name, a type parameter, or a namespace-qualified name
            self.is_type_name(&tname) || self.is_type_param(&tname) || {
                // Peek ahead to see if :: follows (namespace-qualified name)
                let after = self.current.start + self.current.length;
                let rest = &self.source[after..];
                rest.trim_start().starts_with("::")
            }
        } else {
            false
        }
    }

    // ─── Declaration parsing ────────────────────────────────────────────

    fn parse_qualified_name_with_keywords(&mut self) -> Option<String> {
        let saved = self.current.start;
        let mut name = String::new();
        if self.current.kind == TokenKind::Identifier {
            self.advance();
            name.push_str(self.previous_text());
        } else if self.current.kind == TokenKind::Keyword {
            let kw = self.current.keyword;
            if kw == KeywordKind::Id || kw == KeywordKind::Class ||
               kw == KeywordKind::Sel || kw == KeywordKind::Instancetype {
                self.advance();
                name.push_str(self.previous_text());
            } else {
                return None;
            }
        } else {
            return None;
        }
        while self.match_token(TokenKind::ColonColon) {
            name.push_str("::");
            if self.current.kind == TokenKind::Identifier {
                self.advance();
                name.push_str(self.previous_text());
            } else if self.current.kind == TokenKind::Keyword {
                let kw = self.current.keyword;
                if kw == KeywordKind::Id || kw == KeywordKind::Class ||
                   kw == KeywordKind::Sel || kw == KeywordKind::Instancetype {
                    self.advance();
                    name.push_str(self.previous_text());
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Some(name)
    }

    fn parse_function_decl_or_definition(&mut self, return_type: CstType, name: String) -> Option<CstDecl> {
        let mut params = Vec::new();
        let mut has_variadic = false;

        self.consume(TokenKind::LParen, "expected '(' after function name");
        while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
            if self.match_token(TokenKind::Ellipsis) {
                has_variadic = true;
                break;
            }
            let ptype = self.parse_type_full();
            if let Some(mut pt) = ptype {
                let pname = if self.is_name_token() {
                    let n = self.current_text().to_string();
                    self.advance();
                    // Array suffix after param name: name[]
                    if self.match_token(TokenKind::LBracket) {
                        let mut arr_type = CstType::new(TypePrim::Named);
                        arr_type.subtype = Some(Box::new(pt));
                        arr_type.is_array = true;
                        if self.current.kind == TokenKind::Integer {
                            arr_type.array_size = self.current_text().parse().unwrap_or(0);
                            self.advance();
                        } else if self.current.kind == TokenKind::Identifier {
                            arr_type.array_size_name = Some(self.current_text().to_string());
                            self.advance();
                        }
                        self.consume(TokenKind::RBracket, "expected ']' after array size");
                        pt = arr_type;
                    }
                    n
                } else { String::new() };
                params.push(CstParam {
                    par_type: Some(Box::new(pt)),
                    name: if pname.is_empty() { None } else { Some(pname) },
                    external_name: None,
                    next: None,
                });
            } else {
                self.advance();
            }
            if !self.match_token(TokenKind::Comma) { break; }
        }
        self.consume(TokenKind::RParen, "expected ')' after params");

        let body = if self.check(TokenKind::LBrace) {
            self.parse_compound_statement().map(Box::new)
        } else {
            self.consume(TokenKind::Semicolon, "expected ';' after function decl");
            None
        };

        // Link params via next
        let mut head = None;
        let mut tail = &mut head;
        for p in params {
            tail = &mut tail.insert(Box::new(p)).next;
        }

        Some(CstDecl {
            kind: CstDeclKind::Function,
            line: self.previous.line, column: self.previous.column,
            name: Some(name),
            next: None,
            data: CstDeclData::Function {
                return_type: Some(Box::new(return_type)),
                params: head,
                has_variadic,
                body,
            },
        })
    }

    fn parse_declaration(&mut self) -> Option<CstDecl> {
        // @interface / @implementation / @protocol / @class / @namespace / @using
        if self.current.kind == TokenKind::Keyword {
            match self.current.keyword {
                KeywordKind::AtInterface => return self.parse_class_interface(),
                KeywordKind::AtImplementation => return self.parse_class_implementation(),
                KeywordKind::AtProtocol => return self.parse_protocol(),
                KeywordKind::AtClass => return self.parse_forward_class(),
                KeywordKind::AtNamespace => return self.parse_namespace(),
                KeywordKind::AtUsing => return self.parse_using(),
                _ => {}
            }
        }

        // Typedef
        if self.match_keyword(KeywordKind::Typedef) {
            return self.parse_typedef();
        }

        // Struct/union/enum
        if self.match_keyword(KeywordKind::Struct) || self.match_keyword(KeywordKind::Union) {
            let is_union = self.previous.keyword == KeywordKind::Union;
            if self.current.kind == TokenKind::Identifier {
                let name = self.current_text().to_string();
                self.advance();
                if self.check(TokenKind::LBrace) {
                    let fields = self.parse_struct_body(is_union)?;
                    self.consume(TokenKind::Semicolon, "expected ';' after struct");
                    return Some(CstDecl {
                        kind: CstDeclKind::Struct,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(name),
                        next: None,
                        data: CstDeclData::Aggregate { fields, is_union },
                    });
                }
                if self.check(TokenKind::Semicolon) {
                    self.advance();
                    return Some(CstDecl {
                        kind: CstDeclKind::Struct,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(name),
                        next: None,
                        data: CstDeclData::Aggregate { fields: Vec::new(), is_union },
                    });
                }
                // struct Name var = ... — fall through to regular declaration parsing
                // Restore struct/name tokens so parse_type_full can handle it
                // We need to make the parser think we haven't consumed struct Foo yet
                // By reconstructing the type from what we've consumed
                let struct_type = CstType {
                    prim: TypePrim::Named, is_pointer: false, is_struct: true,
                    name: Some(name), subtype: None, next: None,
                    block_params: None, is_const: false, is_block: false,
                    is_array: false, array_size: 0, is_volatile: false,
                    is_block_qual: false, is_weak_qual: false, is_unsigned: false,
                    block_name: None, protocols: Vec::new(), type_args: Vec::new(),
                    array_size_name: None,
                };
                // Continue to regular declaration parsing with struct_type as the return type
                // (falls through to lines 1892+)
                let qualifiers = self.parse_decl_qualifiers();
                let name = self.parse_qualified_name_with_keywords()?;
                // Function or variable?
                if self.check(TokenKind::LParen) {
                    return self.parse_function_decl_or_definition(struct_type, name);
                } else {
                    // Variable declaration
                    let mut var = CstDecl {
                        kind: CstDeclKind::Variable,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(name.clone()),
                        next: None,
                        data: CstDeclData::Variable {
                            var_type: Some(Box::new(struct_type)),
                            initializer: None,
                            is_static: qualifiers.0,
                            is_extern: qualifiers.1,
                            is_const: qualifiers.2,
                            is_block_qual: qualifiers.3,
                            is_weak: qualifiers.4,
                        },
                    };
                    // Array suffix
                    if self.match_token(TokenKind::LBracket) {
                        let mut array_type = CstType::new(TypePrim::Named);
                        if let CstDeclData::Variable { ref mut var_type, .. } = var.data {
                            let base = var_type.take().map(|t| *t).unwrap_or_else(|| CstType::new(TypePrim::Int));
                            array_type.subtype = Some(Box::new(base));
                            array_type.is_array = true;
                            if self.current.kind == TokenKind::Integer {
                                array_type.array_size = self.current_text().parse().unwrap_or(0);
                                self.advance();
                            }
                            *var_type = Some(Box::new(array_type));
                        }
                        self.consume(TokenKind::RBracket, "expected ']' after array size");
                    }
                    // Initializer
                    if self.match_token(TokenKind::Assign) {
                        let init = self.parse_assignment();
                        if let CstDeclData::Variable { ref mut initializer, .. } = var.data {
                            *initializer = init.map(Box::new);
                        }
                    }
                    // Comma-separated declarations
                    if self.match_token(TokenKind::Comma) {
                        let mut head = Box::new(var);
                        let mut tail = &mut head;
                        loop {
                            if self.current.kind != TokenKind::Identifier { break; }
                            let n = self.current_text().to_string();
                            self.advance();
                            let mut next_var = CstDecl {
                                kind: CstDeclKind::Variable,
                                line: self.previous.line, column: self.previous.column,
                                name: Some(n),
                                next: None,
                                data: CstDeclData::Variable {
                                    var_type: None,
                                    initializer: None,
                                    is_static: qualifiers.0,
                                    is_extern: qualifiers.1,
                                    is_const: qualifiers.2,
                                    is_block_qual: qualifiers.3,
                                    is_weak: qualifiers.4,
                                },
                            };
                            if self.match_token(TokenKind::Assign) {
                                if let CstDeclData::Variable { ref mut initializer, .. } = next_var.data {
                                    *initializer = self.parse_assignment().map(Box::new);
                                }
                            }
                            tail.next = Some(Box::new(next_var));
                            tail = tail.next.as_mut().unwrap();
                            if !self.match_token(TokenKind::Comma) { break; }
                        }
                        self.consume(TokenKind::Semicolon, "expected ';' after declaration");
                        return Some(*head);
                    }
                    self.consume(TokenKind::Semicolon, "expected ';' after declaration");
                    return Some(var);
                }
            }
            if let Some(fields) = self.parse_struct_body(is_union) {
                self.consume(TokenKind::Semicolon, "expected ';' after struct");
                return Some(CstDecl {
                    kind: CstDeclKind::Struct,
                    line: self.previous.line, column: self.previous.column,
                    name: None,
                    next: None,
                    data: CstDeclData::Aggregate { fields, is_union },
                });
            }
            return None;
        }
        if self.match_keyword(KeywordKind::Enum) {
            return self.parse_enum();
        }

        // Regular declaration: type name = ...; or type name(params) { ... }
        let saved_pos = self.current.start;
        let qualifiers = self.parse_decl_qualifiers();
        let return_type = match self.parse_type_full() {
            Some(t) => t,
            None => {
                // Fallback: treat unknown identifier as type name (like C parser)
                if self.current.kind == TokenKind::Identifier {
                    let tname = self.current_text().to_string();
                    self.advance();
                    let mut t = CstType::new(TypePrim::Named);
                    t.name = Some(tname);
                    t
                } else {
                    return None;
                }
            }
        };
        let name = if return_type.block_name.is_some() {
            // Block type consumed the name as block_name (e.g., int (^name)(params))
            return_type.block_name.clone().unwrap()
        } else {
            self.parse_qualified_name_with_keywords()?
        };

        // Function or variable?
        if self.check(TokenKind::LParen) {
            self.parse_function_decl_or_definition(return_type, name)
        } else {
            // Variable declaration
            let mut var = CstDecl {
                kind: CstDeclKind::Variable,
                line: self.previous.line, column: self.previous.column,
                name: Some(name.clone()),
                next: None,
                data: CstDeclData::Variable {
                    var_type: Some(Box::new(return_type)),
                    initializer: None,
                    is_static: qualifiers.0,
                    is_extern: qualifiers.1,
                    is_const: qualifiers.2,
                    is_block_qual: qualifiers.3,
                    is_weak: qualifiers.4,
                },
            };

            // Array suffix: name[size]
            if self.match_token(TokenKind::LBracket) {
                let mut array_type = CstType::new(TypePrim::Named);
                if let CstDeclData::Variable { ref mut var_type, .. } = var.data {
                    let base = var_type.take().map(|t| *t).unwrap_or_else(|| CstType::new(TypePrim::Int));
                    array_type.subtype = Some(Box::new(base));
                    array_type.is_array = true;
                    if self.current.kind == TokenKind::Integer {
                        array_type.array_size = self.current_text().parse().unwrap_or(0);
                        self.advance();
                    } else if self.current.kind == TokenKind::Identifier {
                        array_type.array_size_name = Some(self.current_text().to_string());
                        self.advance();
                    }
                    *var_type = Some(Box::new(array_type));
                }
                self.consume(TokenKind::RBracket, "expected ']' after array size");
            }

            // Initializer
            if self.match_token(TokenKind::Assign) {
                let init = self.parse_assignment();
                if let CstDeclData::Variable { ref mut initializer, .. } = var.data {
                    *initializer = init.map(Box::new);
                }
            }

            // Comma-separated declarations
            if self.match_token(TokenKind::Comma) {
                let mut head = Box::new(var);
                let mut tail = &mut head;
                loop {
                    if self.current.kind != TokenKind::Identifier { break; }
                    let n = self.current_text().to_string();
                    self.advance();
                    let mut next_var = CstDecl {
                        kind: CstDeclKind::Variable,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(n),
                        next: None,
                        data: CstDeclData::Variable {
                            var_type: None, // inherits type from first
                            initializer: None,
                            is_static: qualifiers.0,
                            is_extern: qualifiers.1,
                            is_const: qualifiers.2,
                            is_block_qual: qualifiers.3,
                            is_weak: qualifiers.4,
                        },
                    };
                    if self.match_token(TokenKind::Assign) {
                        if let CstDeclData::Variable { ref mut initializer, .. } = next_var.data {
                            *initializer = self.parse_assignment().map(Box::new);
                        }
                    }
                    tail.next = Some(Box::new(next_var));
                    tail = tail.next.as_mut().unwrap();
                    if !self.match_token(TokenKind::Comma) { break; }
                }
                self.consume(TokenKind::Semicolon, "expected ';' after declaration");
                return Some(*head);
            }

            self.consume(TokenKind::Semicolon, "expected ';' after declaration");
            Some(var)
        }
    }

    fn parse_decl_qualifiers(&mut self) -> (bool, bool, bool, bool, bool) {
        let mut is_static = false;
        let mut is_extern = false;
        let mut is_const = false;
        let mut is_block = false;
        let mut is_weak = false;
        loop {
            if self.match_keyword(KeywordKind::Static) { is_static = true; }
            else if self.match_keyword(KeywordKind::Extern) { is_extern = true; }
            else if self.match_keyword(KeywordKind::Const) { is_const = true; }
            else if self.match_keyword(KeywordKind::Block) { is_block = true; }
            else if self.match_keyword(KeywordKind::Weak) { is_weak = true; }
            else { break; }
        }
        (is_static, is_extern, is_const, is_block, is_weak)
    }

    fn parse_typedef(&mut self) -> Option<CstDecl> {
        // `typedef enum {...} Alias;` — delegate to parse_enum (which handles
        // optional tag name + brace body + members), then take trailing alias.
        // The struct/union branch below only handles Struct/Union, so without
        // this Enum branch `typedef enum {...} GameState;` fell through to
        // parse_type_full and dropped all enum members.
        if self.current.kind == TokenKind::Keyword && self.current.keyword == KeywordKind::Enum {
            // Consume the `enum` keyword so parse_enum sees the tag name or `{`
            // (parse_enum expects current to be IDENT or LBrace, NOT the enum kw).
            self.advance();
            let mut enum_decl = self.parse_enum()?;
            if self.current.kind == TokenKind::Identifier {
                let alias = self.current_text().to_string();
                self.advance();
                self.consume(TokenKind::Semicolon, "expected ; after typedef");
                self.add_type_name(&alias);
                if let CstDeclData::Enum { .. } = enum_decl.data {
                    enum_decl.name = Some(alias);
                }
                return Some(enum_decl);
            }
            self.consume(TokenKind::Semicolon, "expected ; after typedef");
            return Some(enum_decl);
        }
        if self.current.kind == TokenKind::Keyword &&
           (self.current.keyword == KeywordKind::Struct ||
            self.current.keyword == KeywordKind::Union) {
            let is_union = self.current.keyword == KeywordKind::Union;
            self.advance();
            let mut anon = false;
            if self.current.kind == TokenKind::Identifier {
                // Named struct
                let name = self.current_text().to_string();
                self.advance();
                if self.check(TokenKind::LBrace) {
                    let struct_decl = self.parse_struct_body(is_union)?;
                    self.add_type_name(&name);
                    // Parse typedef name
                    if self.current.kind == TokenKind::Identifier {
                        let alias = self.current_text().to_string();
                        self.advance();
                        self.consume(TokenKind::Semicolon, "expected ; after typedef");
                        self.add_type_name(&alias);
                        return Some(CstDecl {
                            kind: CstDeclKind::Typedef,
                            line: self.previous.line, column: self.previous.column,
                            name: Some(alias),
                            next: None,
                            data: CstDeclData::Typedef {
                                alias_type: Some(Box::new(CstType {
                                    prim: TypePrim::Named, is_struct: true,
                                    name: Some(name),
                                    ..CstType::new(TypePrim::Named)
                                })),
                                struct_fields: struct_decl,
                            },
                        });
                    }
                    self.consume(TokenKind::Semicolon, "expected ; after struct");
                    return Some(CstDecl {
                        kind: CstDeclKind::Struct,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(name),
                        next: None,
                        data: CstDeclData::Aggregate { fields: struct_decl, is_union },
                    });
                }
                // Forward declaration
                self.consume(TokenKind::Semicolon, "expected ; after struct name");
                return Some(CstDecl {
                    kind: CstDeclKind::Struct,
                    line: self.previous.line, column: self.previous.column,
                    name: Some(name),
                    next: None,
                    data: CstDeclData::Aggregate { fields: Vec::new(), is_union },
                });
            }
            // Anonymous struct/union
            if self.check(TokenKind::LBrace) {
                let fields = self.parse_struct_body(is_union)?;
                if self.current.kind == TokenKind::Identifier {
                    let alias = self.current_text().to_string();
                    self.advance();
                    self.consume(TokenKind::Semicolon, "expected ; after typedef");
                    self.add_type_name(&alias);
                    return Some(CstDecl {
                        kind: CstDeclKind::Typedef,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(alias),
                        next: None,
                        data: CstDeclData::Typedef {
                            alias_type: None,
                            struct_fields: fields,
                        },
                    });
                }
                self.consume(TokenKind::Semicolon, "expected ; after struct");
                return Some(CstDecl {
                    kind: CstDeclKind::Struct,
                    line: self.previous.line, column: self.previous.column,
                    name: None,
                    next: None,
                    data: CstDeclData::Aggregate { fields, is_union },
                });
            }
            self.consume(TokenKind::Semicolon, "expected ; after struct");
            return None;
        }

        // typedef type name;
        // Support `typedef enum { ... } Name;` — parse the enum body inline,
        // then take the trailing identifier as the alias name. Return an Enum
        // decl (not Typedef) so elaborator/codegen emit `enum Name {...};` +
        // `typedef enum Name Name;` and the members are preserved. Returning
        // Typedef with `alias_type=None` previously dropped all enum members.
        if self.check_keyword(KeywordKind::Enum) {
            let mut enum_decl = self.parse_enum()?;
            // Optional trailing alias identifier: `} AliasName;`
            if self.current.kind == TokenKind::Identifier {
                let alias = self.current_text().to_string();
                self.advance();
                self.consume(TokenKind::Semicolon, "expected ; after typedef");
                self.add_type_name(&alias);
                // Override the enum decl's name to the trailing alias so codegen
                // emits `typedef enum Alias Alias;` matching the source form.
                if let CstDeclData::Enum { .. } = enum_decl.data {
                    enum_decl.name = Some(alias);
                }
                return Some(enum_decl);
            }
            self.consume(TokenKind::Semicolon, "expected ; after typedef");
            return Some(enum_decl);
        }

        // typedef type name;
        let alias_type = self.parse_type_full();
        // Check if the type has a block_name (e.g. typedef int (^name)(params) → name is the alias)
        if let Some(ref at) = alias_type {
            if let Some(ref block_name) = at.block_name {
                let name = block_name.clone();
                self.consume(TokenKind::Semicolon, "expected ; after typedef");
                self.add_type_name(&name);
                return Some(CstDecl {
                    kind: CstDeclKind::Typedef,
                    line: self.previous.line, column: self.previous.column,
                    name: Some(name),
                    next: None,
                    data: CstDeclData::Typedef {
                        alias_type: Some(Box::new(at.clone())),
                        struct_fields: Vec::new(),
                    },
                });
            }
        }
        if self.current.kind == TokenKind::Identifier {
            let name = self.current_text().to_string();
            self.advance();
            self.consume(TokenKind::Semicolon, "expected ; after typedef");
            self.add_type_name(&name);
            return Some(CstDecl {
                kind: CstDeclKind::Typedef,
                line: self.previous.line, column: self.previous.column,
                name: Some(name),
                next: None,
                data: CstDeclData::Typedef {
                    alias_type: alias_type.map(Box::new),
                    struct_fields: Vec::new(),
                },
            });
        }
        self.consume(TokenKind::Semicolon, "expected ; after typedef");
        None
    }

    fn parse_struct_body(&mut self, is_union: bool) -> Option<Vec<CstDecl>> {
        self.advance(); // consume {
        let mut fields = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
            if let Some(ftype) = self.parse_type_full() {
                if self.current.kind == TokenKind::Identifier {
                    let fname = self.current_text().to_string();
                    self.advance();
                    // Array field: name[size]
                    let mut field_type = ftype;
                    if self.match_token(TokenKind::LBracket) {
                        let mut array_type = CstType::new(TypePrim::Named);
                        array_type.subtype = Some(Box::new(field_type));
                        array_type.is_array = true;
                        if self.current.kind == TokenKind::Integer {
                            array_type.array_size = self.current_text().parse().unwrap_or(0);
                            self.advance();
                        }
                        field_type = array_type;
                        self.consume(TokenKind::RBracket, "expected ']'");
                    }
                    fields.push(CstDecl {
                        kind: CstDeclKind::Ivar,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(fname),
                        next: None,
                        data: CstDeclData::Ivar {
                            ivar_type: Some(Box::new(field_type)),
                            iboutlet: false,
                        },
                    });
                }
                // Handle comma-separated fields
                while self.match_token(TokenKind::Comma) {
                    if self.current.kind == TokenKind::Identifier {
                        let fname = self.current_text().to_string();
                        self.advance();
                        fields.push(CstDecl {
                            kind: CstDeclKind::Ivar,
                            line: self.previous.line, column: self.previous.column,
                            name: Some(fname),
                            next: None,
                            data: CstDeclData::Ivar {
                                ivar_type: None,
                                iboutlet: false,
                            },
                        });
                    }
                }
            } else if !self.check(TokenKind::Semicolon) && !self.check(TokenKind::RBrace) {
                break;
            }
            if self.check(TokenKind::Semicolon) {
                self.advance();
            } else {
                self.consume(TokenKind::Semicolon, "expected ';' after field");
            }
            while self.match_keyword(KeywordKind::Static) || self.match_keyword(KeywordKind::Const) ||
                  self.match_keyword(KeywordKind::Volatile) || self.match_keyword(KeywordKind::Inline) {
                // skip
            }
        }
        self.consume(TokenKind::RBrace, "expected '}' after struct");
        Some(fields)
    }

    fn parse_enum(&mut self) -> Option<CstDecl> {
        let mut members = Vec::new();
        let mut values = Vec::new();
        // Optional tag name: `enum Foo { ... }` vs anonymous `enum { ... }`
        // (the latter arises from `typedef enum { ... } Alias;` where `enum`
        // is immediately followed by `{`). The previous code only handled the
        // tagged form and returned None for anonymous enums, dropping the
        // entire decl from the output (cf. old C parser line ~2336 which
        // proceeds into the brace body when tag is NULL).
        let mut name: Option<String> = None;
        if self.current.kind == TokenKind::Identifier {
            name = Some(self.current_text().to_string());
            self.advance();
            // For a tagged enum that is NOT followed by `{`, treat as forward
            // declaration: `enum Foo;` — register the tag and return an empty
            // enum decl so the name resolves.
            if !self.check(TokenKind::LBrace) {
                if let Some(ref n) = name { self.add_type_name(n); }
                self.consume(TokenKind::Semicolon, "expected ';' after enum name");
                return Some(CstDecl {
                    kind: CstDeclKind::Enum,
                    line: self.previous.line, column: self.previous.column,
                    name,
                    next: None,
                    data: CstDeclData::Enum { members, values },
                });
            }
        }
        // Body: `{ member = value, ... }` — tagged or anonymous
        if self.check(TokenKind::LBrace) {
            self.advance();
            let mut next_val: i64 = 0;
            while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
                if self.current.kind == TokenKind::Identifier {
                    let mname = self.current_text().to_string();
                    self.advance();
                    members.push(mname);
                    if self.match_token(TokenKind::Assign) {
                        if self.current.kind == TokenKind::Integer {
                            let val = self.current_text().parse().unwrap_or(next_val);
                            self.advance();
                            next_val = val;
                        }
                    }
                    values.push(CstExpr {
                        kind: CstExprKind::Integer, expr_type: None,
                        line: self.previous.line, col: self.previous.column,
                        data: CstExprData::Integer(next_val),
                    });
                    next_val += 1;
                    if !self.match_token(TokenKind::Comma) { break; }
                } else {
                    break;
                }
            }
            self.consume(TokenKind::RBrace, "expected '}' after enum");
            // Do NOT consume `;` here — `typedef enum {...} Alias;` flow needs
            // to read the trailing alias identifier first (handled by caller
            // `parse_typedef`). For a bare `enum Foo {...};` decl, the caller
            // (`parse_declaration`) consumes the `;`.
            return Some(CstDecl {
                kind: CstDeclKind::Enum,
                line: self.previous.line, column: self.previous.column,
                name,
                next: None,
                data: CstDeclData::Enum { members, values },
            });
        }
        None
    }

    // ─── @interface / @implementation / @protocol parsing ──────────────

    fn parse_class_interface(&mut self) -> Option<CstDecl> {
        self.advance(); // consume @interface

        if !self.match_name() { self.error("expected class name after @interface"); return None; }
        let mut name = self.previous_text().to_string();
        // Check for qualified name (Namespace::ClassName)
        while self.match_token(TokenKind::ColonColon) {
            name.push_str("::");
            if self.match_name() {
                name.push_str(self.previous_text());
            } else {
                self.error("expected identifier after '::'");
                return None;
            }
        }
        self.add_type_name(&name);

        // Check for category: @interface ClassName (CategoryName)
        let mut category_name = None;
        if self.match_token(TokenKind::LParen) {
            // Category name — may contain non-ASCII characters, consume until )
            let mut cat = String::new();
            while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                if self.current.kind == TokenKind::Identifier ||
                   self.current.kind == TokenKind::Keyword {
                    cat.push_str(self.current_text());
                } else {
                    // Non-ASCII chars: just consume the raw text
                    let start = self.current.start;
                    let end = if self.current.length > 0 { start + self.current.length } else { start };
                    if end > start {
                        if let Some(s) = self.source.get(start..end) {
                            cat.push_str(s);
                        }
                    }
                }
                self.advance();
            }
            self.consume(TokenKind::RParen, "expected ')' after category name");
            category_name = if cat.is_empty() { None } else { Some(cat) };
        }

        // Generic type params: <T>
        let mut type_params = Vec::new();
        if self.match_token(TokenKind::Less) {
            while self.current.kind == TokenKind::Identifier ||
                  (self.current.kind == TokenKind::Keyword &&
                   matches!(self.current.keyword, KeywordKind::Id | KeywordKind::Class |
                    KeywordKind::Sel | KeywordKind::Instancetype)) {
                let tp = self.current_text().to_string();
                self.advance();
                self.add_type_param(&tp);
                type_params.push(tp);
                if !self.match_token(TokenKind::Comma) { break; }
            }
            self.consume(TokenKind::Greater, "expected '>' after type params");
        }

        // Register class as generic if it has type params
        if !type_params.is_empty() && !self.generic_class_names.contains(&name) {
            self.generic_class_names.push(name.clone());
        }

        // Superclass: : SuperClassName (may be a qualified name like Engine::Graphics::RenderNode)
        let mut superclass = None;
        if self.match_token(TokenKind::Colon) {
            if self.current.kind == TokenKind::Identifier {
                superclass = self.parse_qualified_name();
            }
        }

        // Protocols: <Proto1, Proto2>
        let mut protocols = Vec::new();
        if self.match_token(TokenKind::Less) {
            while self.current.kind == TokenKind::Identifier {
                let p = self.current_text().to_string();
                self.advance();
                protocols.push(p);
                if !self.match_token(TokenKind::Comma) { break; }
            }
            // Check for > or >> (>> is parsed as two > tokens)
            if self.current.kind == TokenKind::Greater {
                self.advance();
            } else if self.current.kind == TokenKind::RShift {
                // >> is two > tokens
                self.advance();
            }
        }

        // Ivars: { ... }
        let mut ivars = Vec::new();
        if self.match_token(TokenKind::LBrace) {
            while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
                // ivar qualifiers: @public, @protected, @private, @package
                if self.match_keyword(KeywordKind::AtPublic) ||
                   self.match_keyword(KeywordKind::AtProtected) ||
                   self.match_keyword(KeywordKind::AtPrivate) ||
                   self.match_keyword(KeywordKind::AtPackage) {
                    continue;
                }
                // ivar attribute parens: `@public (weak) DirectoryNode *_parent;`
                // Consume the parenthesized qualifier list so the following type
                // declaration parses cleanly. Previously the `(` landed at the
                // method-declaration path (`parse_declaration` saw `(` as a method
                // return-type group) and errored with
                // `expected ';' after ivar (got '(')`.
                if self.match_token(TokenKind::LParen) {
                    while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                        self.advance();
                    }
                    self.consume(TokenKind::RParen, "expected ')' after ivar qualifier");
                }
                // IBOutlet qualifier
                let iboutlet = false;
                if let Some(ivar_type) = self.parse_type_full() {
                    while self.match_name() {
                        let ivar_name = self.previous_text().to_string();
                        let mut final_type = ivar_type.clone();
                        // Check for array suffix: name[size]
                        if self.match_token(TokenKind::LBracket) {
                            let mut arr_type = CstType::new(TypePrim::Named);
                            arr_type.subtype = Some(Box::new(final_type));
                            arr_type.is_array = true;
                            // Array size may be an integer literal OR a macro/enum
                            // constant identifier (e.g. `FSNode *_children[MAX_CHILDREN];`).
                            // The previous code only accepted Integer tokens, leaving the
                            // identifier in place so `consume(RBracket)` failed with
                            // `expected ']' after array size (got identifier)`.
                            if self.current.kind == TokenKind::Integer {
                                arr_type.array_size = self.current_text().parse().unwrap_or(0);
                                self.advance();
                            } else if self.current.kind == TokenKind::Identifier {
                                // Symbolic size — record the identifier name so codegen
                                // can emit `T[MAX_CHILDREN]` instead of `T[]` (flexible
                                // array member, which C forbids for non-trailing fields).
                                arr_type.array_size = 0;
                                arr_type.array_size_name = Some(self.current_text().to_string());
                                self.advance();
                            }
                            self.consume(TokenKind::RBracket, "expected ']' after array size");
                            final_type = arr_type;
                        }
                        ivars.push(CstDecl {
                            kind: CstDeclKind::Ivar,
                            line: self.previous.line, column: self.previous.column,
                            name: Some(ivar_name),
                            next: None,
                            data: CstDeclData::Ivar {
                                ivar_type: Some(Box::new(final_type)),
                                iboutlet,
                            },
                        });
                        if !self.match_token(TokenKind::Comma) { break; }
                    }
                }
                self.consume(TokenKind::Semicolon, "expected ';' after ivar");
            }
            self.consume(TokenKind::RBrace, "expected '}' after ivar list");
        }

        // Properties and methods
        let mut properties = Vec::new();
        let mut methods = Vec::new();
        while !self.match_keyword(KeywordKind::AtEnd) && !self.check(TokenKind::Eof) {
            if self.match_keyword(KeywordKind::AtProperty) {
                if let Some(prop) = self.parse_property() {
                    properties.push(prop);
                }
            } else if self.current.kind == TokenKind::Keyword &&
                      (self.current.keyword == KeywordKind::AtSynthesize ||
                       self.current.keyword == KeywordKind::AtDynamic) {
                let is_dynamic = self.current.keyword == KeywordKind::AtDynamic;
                self.advance();
            while self.current.kind == TokenKind::Identifier ||
                  (self.current.kind == TokenKind::Keyword && !matches!(self.current.keyword,
                      KeywordKind::AtInterface | KeywordKind::AtImplementation | KeywordKind::AtEnd |
                      KeywordKind::AtProperty | KeywordKind::AtSynthesize | KeywordKind::AtDynamic |
                      KeywordKind::AtSelector | KeywordKind::AtEncode | KeywordKind::AtProtocol |
                      KeywordKind::AtOptional | KeywordKind::AtRequired | KeywordKind::AtClass |
                      KeywordKind::AtTry | KeywordKind::AtCatch | KeywordKind::AtFinally |
                      KeywordKind::AtThrow | KeywordKind::AtSynchronized | KeywordKind::AtAutoreleasepool |
                      KeywordKind::AtPublic | KeywordKind::AtPackage | KeywordKind::AtProtected |
                      KeywordKind::AtPrivate | KeywordKind::AtDefs | KeywordKind::AtNamespace |
                      KeywordKind::AtUsing | KeywordKind::Self_ | KeywordKind::Super |
                      KeywordKind::Return | KeywordKind::If | KeywordKind::Else |
                      KeywordKind::Switch | KeywordKind::Case | KeywordKind::Default |
                      KeywordKind::While | KeywordKind::Do | KeywordKind::For |
                      KeywordKind::Break | KeywordKind::Continue | KeywordKind::Goto |
                      KeywordKind::Sizeof | KeywordKind::Typeof | KeywordKind::Typedef |
                      KeywordKind::Struct | KeywordKind::Union | KeywordKind::Enum |
                      KeywordKind::Const | KeywordKind::Volatile | KeywordKind::Extern |
                      KeywordKind::Static | KeywordKind::Auto | KeywordKind::Register |
                      KeywordKind::Inline | KeywordKind::Restrict |
                      KeywordKind::Imp | KeywordKind::NpZone |
                      KeywordKind::Import | KeywordKind::Include | KeywordKind::Define |
                      KeywordKind::Ifdef | KeywordKind::Ifndef | KeywordKind::Endif |
                      KeywordKind::Pragma | KeywordKind::Elif | KeywordKind::Undef
                  )) {
                    let prop_name = self.current_text().to_string();
                    self.advance();
                    if self.match_token(TokenKind::Assign) {
                        if self.current.kind == TokenKind::Identifier {
                            self.advance();
                        }
                    }
                    if !self.match_token(TokenKind::Comma) { break; }
                }
                self.consume(TokenKind::Semicolon, "expected ';' after @synthesize/@dynamic");
            } else if self.current.kind == TokenKind::Keyword &&
                      (self.current.keyword == KeywordKind::AtPublic ||
                       self.current.keyword == KeywordKind::AtProtected ||
                       self.current.keyword == KeywordKind::AtPrivate ||
                       self.current.keyword == KeywordKind::AtPackage) {
                self.advance();
            } else {
                // Method (+/-)
                if self.current.kind == TokenKind::Keyword &&
                   (self.current.keyword == KeywordKind::Block ||
                    self.current.keyword == KeywordKind::Weak ||
                    self.current.keyword == KeywordKind::Strong) {
                    // This might be a __block/__weak declaration inside @interface — skip
                    // Actually, these appear inside @implementation for local variables
                    self.advance();
                    continue;
                }
                if let Some(method) = self.parse_method() {
                    methods.push(method);
                } else {
                    self.advance();
                }
            }
        }

        // Register type param names
        for tp in &type_params {
            self.add_type_name(tp);
        }
        self.add_type_name(&name);

        let kind = if category_name.is_some() {
            CstDeclKind::CategoryInterface
        } else {
            CstDeclKind::ClassInterface
        };

        Some(CstDecl {
            kind,
            line: self.previous.line, column: self.previous.column,
            name: Some(name),
            next: None,
            data: CstDeclData::Class {
                superclass,
                category_name,
                protocols,
                type_params,
                ivars,
                properties,
                methods,
                impl_vars: Vec::new(),
            },
        })
    }

    fn parse_class_implementation(&mut self) -> Option<CstDecl> {
        self.advance(); // consume @implementation
        // Class name may be namespace-qualified: `@implementation Game::Player`.
        let name = match self.parse_qualified_name_with_keywords() {
            Some(n) if !n.is_empty() => n,
            _ => { self.error("expected class name after @implementation"); return None; }
        };

        // Check for category: @implementation ClassName (CategoryName)
        let mut category_name = None;
        if self.match_token(TokenKind::LParen) {
            let mut cat = String::new();
            while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                if self.current.kind == TokenKind::Identifier ||
                   self.current.kind == TokenKind::Keyword {
                    cat.push_str(self.current_text());
                } else {
                    let start = self.current.start;
                    let end = if self.current.length > 0 { start + self.current.length } else { start };
                    if end > start {
                        if let Some(s) = self.source.get(start..end) {
                            cat.push_str(s);
                        }
                    }
                }
                self.advance();
            }
            self.consume(TokenKind::RParen, "expected ')' after category name");
            category_name = if cat.is_empty() { None } else { Some(cat) };
        }

        let mut methods = Vec::new();
        let mut impl_vars = Vec::new();
        while !self.match_keyword(KeywordKind::AtEnd) && !self.check(TokenKind::Eof) {
            if self.current.kind == TokenKind::Keyword &&
               (self.current.keyword == KeywordKind::AtProperty ||
                self.current.keyword == KeywordKind::AtSynthesize ||
                self.current.keyword == KeywordKind::AtDynamic) {
                self.advance();
                // Skip property/synthesize/dynamic in @implementation
                if self.current.kind == TokenKind::LParen {
                    // Skip property attributes
                    let mut depth = 1;
                    while depth > 0 && !self.check(TokenKind::Eof) {
                        if self.current.kind == TokenKind::LParen { depth += 1; }
                        if self.current.kind == TokenKind::RParen { depth -= 1; }
                        self.advance();
                    }
                }
                // Skip to ;
                while !self.check(TokenKind::Semicolon) && !self.check(TokenKind::Eof) {
                    self.advance();
                }
                if self.current.kind == TokenKind::Semicolon { self.advance(); }
                continue;
            }
            // Check for variable declarations inside @implementation
            // (e.g., static globals used by categories).
            // But if the current token is + or - (method type indicators),
            // try parse_method first to avoid confusing method return types
            // (like int, void, BOOL) with declaration starts.
            if self.current.kind == TokenKind::Plus || self.current.kind == TokenKind::Minus {
                if let Some(method) = self.parse_method() {
                    methods.push(method);
                    continue;
                }
            } else if self.is_declaration_start() {
                if let Some(decl) = self.parse_declaration() {
                    impl_vars.push(decl);
                    continue;
                }
            }
            if let Some(method) = self.parse_method() {
                methods.push(method);
            } else {
                // Skip to next @end or method
                if self.current.kind == TokenKind::Eof { break; }
                self.advance();
            }
        }

        let kind = if category_name.is_some() {
            CstDeclKind::CategoryImplementation
        } else {
            CstDeclKind::ClassImplementation
        };

        Some(CstDecl {
            kind,
            line: self.previous.line, column: self.previous.column,
            name: Some(name),
            next: None,
            data: CstDeclData::Class {
                superclass: None,
                category_name,
                protocols: Vec::new(),
                type_params: Vec::new(),
                ivars: Vec::new(),
                properties: Vec::new(),
                methods,
                impl_vars,
            },
        })
    }

    fn parse_property(&mut self) -> Option<CstDecl> {
        // Attributes: (readonly, weak, nonatomic, getter=xxx, setter=xxx:)
        let mut is_readonly = false;
        let mut is_weak = false;
        let mut is_assign = false;
        let mut is_retain = false;
        let mut is_copy = false;
        let mut is_nonatomic = false;
        let mut getter = None;
        let mut setter = None;

        if self.match_token(TokenKind::LParen) {
            while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                if self.match_keyword(KeywordKind::AtReadonly) { is_readonly = true; }
                else if self.match_keyword(KeywordKind::AtWeak) { is_weak = true; }
                else if self.match_keyword(KeywordKind::AtAssign) { is_assign = true; }
                else if self.match_keyword(KeywordKind::AtRetain) { is_retain = true; }
                else if self.match_keyword(KeywordKind::AtCopy) { is_copy = true; }
                else if self.match_keyword(KeywordKind::AtNonatomic) { is_nonatomic = true; }
                else if self.match_keyword(KeywordKind::AtGetter) {
                    self.consume(TokenKind::Assign, "expected '=' after getter");
                    if self.current.kind == TokenKind::Identifier {
                        getter = Some(self.current_text().to_string());
                        self.advance();
                    }
                }
                else if self.match_keyword(KeywordKind::AtSetter) {
                    self.consume(TokenKind::Assign, "expected '=' after setter");
                    if self.current.kind == TokenKind::Identifier {
                        let mut s = self.current_text().to_string();
                        self.advance();
                        if self.match_token(TokenKind::Colon) {
                            // setter name includes colon
                        }
                        setter = Some(s);
                    }
                }
                else {
                    self.advance();
                }
                if !self.match_token(TokenKind::Comma) { break; }
            }
            self.consume(TokenKind::RParen, "expected ')' after property attributes");
        }

        let mut prop_type = self.parse_type_full();
        // Check for array suffix: name[size]
        if self.match_name() {
            let name = self.previous_text().to_string();
            // Array suffix
            if self.match_token(TokenKind::LBracket) {
                let mut arr_type = CstType::new(TypePrim::Named);
                if let Some(ref pt) = prop_type {
                    arr_type.subtype = Some(Box::new(pt.clone()));
                }
                arr_type.is_array = true;
                if self.current.kind == TokenKind::Integer {
                    arr_type.array_size = self.current_text().parse().unwrap_or(0);
                    self.advance();
                }
                self.consume(TokenKind::RBracket, "expected ']' after array size");
                prop_type = Some(arr_type);
            }
            // Check for comma-separated names
            if self.match_token(TokenKind::Comma) {
                // Parse additional names with the same type
                let mut head = CstDecl {
                    kind: CstDeclKind::Property,
                    line: self.previous.line, column: self.previous.column,
                    name: Some(name),
                    next: None,
                    data: CstDeclData::Property {
                        prop_type: prop_type.clone().map(Box::new),
                        getter: getter.clone(),
                        setter: setter.clone(),
                        is_readonly,
                        is_weak,
                        is_assign,
                        is_retain,
                        is_copy,
                        is_nonatomic,
                        is_dynamic: false,
                    },
                };
                let mut tail = &mut head;
                while self.match_name() {
                    let nname = self.previous_text().to_string();
                    tail.next = Some(Box::new(CstDecl {
                        kind: CstDeclKind::Property,
                        line: self.previous.line, column: self.previous.column,
                        name: Some(nname),
                        next: None,
                        data: CstDeclData::Property {
                            prop_type: prop_type.clone().map(Box::new),
                            getter: getter.clone(),
                            setter: setter.clone(),
                            is_readonly,
                            is_weak,
                            is_assign,
                            is_retain,
                            is_copy,
                            is_nonatomic,
                            is_dynamic: false,
                        },
                    }));
                    tail = tail.next.as_mut().unwrap();
                    if !self.match_token(TokenKind::Comma) { break; }
                }
                self.consume(TokenKind::Semicolon, "expected ';' after @property");
                return Some(head);
            }
            self.consume(TokenKind::Semicolon, "expected ';' after @property");
            return Some(CstDecl {
                kind: CstDeclKind::Property,
                line: self.previous.line, column: self.previous.column,
                name: Some(name),
                next: None,
                data: CstDeclData::Property {
                    prop_type: prop_type.map(Box::new),
                    getter,
                    setter,
                    is_readonly,
                    is_weak,
                    is_assign,
                    is_retain,
                    is_copy,
                    is_nonatomic,
                    is_dynamic: false,
                },
            });
        }
        self.consume(TokenKind::Semicolon, "expected ';' after @property");
        None
    }

    fn parse_method(&mut self) -> Option<CstDecl> {
        let is_class_method = if self.match_token(TokenKind::Plus) { true }
        else if self.match_token(TokenKind::Minus) { false }
        else { return None; };

        // Parse return type: (type) or plain type
        let return_type = if self.match_token(TokenKind::LParen) {
            let rt = self.parse_type_full();
            self.consume(TokenKind::RParen, "expected ')' after method return type");
            rt
        } else {
            self.parse_type_full()
        };

        // Parse method selector and params
        let mut params: Option<Box<CstParam>> = None;
        let mut tail: &mut Option<Box<CstParam>> = &mut params;
        let mut has_keyword = false;
        let mut method_name = String::new();

        // First keyword/param
        if self.current.kind == TokenKind::Identifier ||
           (self.current.kind == TokenKind::Keyword &&
            !matches!(self.current.keyword, KeywordKind::Return | KeywordKind::If |
                KeywordKind::While | KeywordKind::For | KeywordKind::Do |
                KeywordKind::Switch | KeywordKind::Case | KeywordKind::Default |
                KeywordKind::Break | KeywordKind::Continue | KeywordKind::Goto |
                KeywordKind::Sizeof | KeywordKind::Const | KeywordKind::Static |
                KeywordKind::Extern | KeywordKind::Volatile | KeywordKind::Struct |
                KeywordKind::Union | KeywordKind::Enum | KeywordKind::Typedef |
                KeywordKind::Void | KeywordKind::Int | KeywordKind::Char |
                KeywordKind::Short | KeywordKind::Long | KeywordKind::Float |
                KeywordKind::Double | KeywordKind::Bool | KeywordKind::Signed |
                KeywordKind::Unsigned | KeywordKind::Id | KeywordKind::Class |
                KeywordKind::Sel | KeywordKind::Instancetype |
                KeywordKind::Block | KeywordKind::Weak | KeywordKind::Strong |
                KeywordKind::Autoreleasing | KeywordKind::UnsafeUnretained)) {
            let sel_part = self.current_text().to_string();
            self.advance();

            if self.match_token(TokenKind::Colon) {
                has_keyword = true;
                method_name.push_str(&sel_part);
                method_name.push(':');
                let mut p = CstParam {
                    par_type: None,
                    name: None,
                    external_name: Some(sel_part),
                    next: None,
                };
                // Type name (optional)
                if self.match_token(TokenKind::LParen) {
                    p.par_type = self.parse_type_full().map(Box::new);
                    self.consume(TokenKind::RParen, "expected ')' after param type");
                } else {
                    p.par_type = self.parse_type_full().map(Box::new);
                }
                if self.is_name_token() &&
                   !self.check(TokenKind::Colon) && !self.check(TokenKind::Semicolon) &&
                   !self.check(TokenKind::RBrace) {
                    p.name = Some(self.current_text().to_string());
                    self.advance();
                }
                tail = &mut tail.insert(Box::new(p)).next;

                // More keyword:param pairs
                loop {
                    if self.current.kind == TokenKind::Identifier ||
                       (self.current.kind == TokenKind::Keyword &&
                        !matches!(self.current.keyword, KeywordKind::Return | KeywordKind::If |
                            KeywordKind::While | KeywordKind::For | KeywordKind::Do |
                            KeywordKind::Switch | KeywordKind::Break | KeywordKind::Continue |
                            KeywordKind::Goto | KeywordKind::Sizeof | KeywordKind::Const |
                            KeywordKind::Static | KeywordKind::Extern | KeywordKind::Struct |
                            KeywordKind::Union | KeywordKind::Enum | KeywordKind::Typedef |
                            KeywordKind::Void | KeywordKind::Int | KeywordKind::Char |
                            KeywordKind::Short | KeywordKind::Long | KeywordKind::Float |
                            KeywordKind::Double | KeywordKind::Bool | KeywordKind::Signed |
                            KeywordKind::Unsigned | KeywordKind::Id | KeywordKind::Class |
                            KeywordKind::Sel | KeywordKind::Instancetype |
                            KeywordKind::Block | KeywordKind::Weak | KeywordKind::Strong)) {
                        let next_part = self.current_text().to_string();
                        self.advance();
                        if self.match_token(TokenKind::Colon) {
                            has_keyword = true;
                            method_name.push_str(&next_part);
                            method_name.push(':');
                            let mut next_p = CstParam {
                                par_type: None,
                                name: None,
                                external_name: Some(next_part),
                                next: None,
                            };
                            if self.match_token(TokenKind::LParen) {
                                next_p.par_type = self.parse_type_full().map(Box::new);
                                self.consume(TokenKind::RParen, "expected ')' after param type");
                            } else {
                                next_p.par_type = self.parse_type_full().map(Box::new);
                            }
                             if self.is_name_token() &&
                                !self.check(TokenKind::Colon) && !self.check(TokenKind::Semicolon) &&
                                !self.check(TokenKind::RBrace) {
                                next_p.name = Some(self.current_text().to_string());
                                self.advance();
                            }
                            tail = &mut tail.insert(Box::new(next_p)).next;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            } else {
                method_name = sel_part;
            }
        }

        // If no keyword params, check for C-style params: (type name, ...)
        if !has_keyword && self.match_token(TokenKind::LParen) {
            while !self.check(TokenKind::RParen) && !self.check(TokenKind::Eof) {
                if let Some(ptype) = self.parse_type_full() {
                    let pname = if self.is_name_token() {
                        let n = self.current_text().to_string();
                        self.advance();
                        n
                    } else { String::new() };
                    let mut p = CstParam {
                        par_type: Some(Box::new(ptype)),
                        name: if pname.is_empty() { None } else { Some(pname) },
                        external_name: None,
                        next: None,
                    };
                    tail = &mut tail.insert(Box::new(p)).next;
                }
                if !self.match_token(TokenKind::Comma) { break; }
            }
            self.consume(TokenKind::RParen, "expected ')' after params");
        }

        // Body
        let body = if self.check(TokenKind::LBrace) {
            self.parse_compound_statement().map(Box::new)
        } else {
            self.consume(TokenKind::Semicolon, "expected ';' after method declaration");
            None
        };

        // Build method selector name from params
        let method_name = method_name; // make it non-mut

        Some(CstDecl {
            kind: CstDeclKind::Method,
            line: self.previous.line, column: self.previous.column,
            name: if method_name.is_empty() { None } else { Some(method_name) },
            next: None,
            data: CstDeclData::Method {
                is_class_method,
                return_type: return_type.map(Box::new),
                params,
                body,
            },
        })
    }

    fn parse_protocol(&mut self) -> Option<CstDecl> {
        self.advance(); // consume @protocol
        if !self.match_name() { self.error("expected protocol name"); return None; }
        let name = self.previous_text().to_string();

        // Protocol inheritance: <Proto1, Proto2>
        let mut protocols = Vec::new();
        if self.match_token(TokenKind::Less) {
            while self.current.kind == TokenKind::Identifier {
                let p = self.current_text().to_string();
                self.advance();
                protocols.push(p);
                if !self.match_token(TokenKind::Comma) { break; }
            }
            if self.current.kind == TokenKind::Greater { self.advance(); }
            else if self.current.kind == TokenKind::RShift { self.advance(); }
        }

        let mut methods = Vec::new();
        let mut is_optional = false;
        while !self.match_keyword(KeywordKind::AtEnd) && !self.check(TokenKind::Eof) {
            if self.match_keyword(KeywordKind::AtOptional) {
                is_optional = true;
                continue;
            }
            if self.match_keyword(KeywordKind::AtRequired) {
                is_optional = false;
                continue;
            }
            if let Some(method) = self.parse_method() {
                methods.push(method);
            } else {
                self.advance();
            }
        }

        self.add_type_name(&name);
        Some(CstDecl {
            kind: CstDeclKind::Protocol,
            line: self.previous.line, column: self.previous.column,
            name: Some(name),
            next: None,
            data: CstDeclData::ProtocolData { protocols, methods, is_optional },
        })
    }

    fn parse_forward_class(&mut self) -> Option<CstDecl> {
        self.advance(); // consume @class
        let mut names = Vec::new();
        while self.current.kind == TokenKind::Identifier {
            names.push(self.current_text().to_string());
            self.advance();
            if !self.match_token(TokenKind::Comma) { break; }
        }
        self.consume(TokenKind::Semicolon, "expected ';' after @class");
        for n in &names {
            self.add_type_name(n);
        }
        Some(CstDecl {
            kind: CstDeclKind::ForwardClass,
            line: self.previous.line, column: self.previous.column,
            name: None,
            next: None,
            data: CstDeclData::Forward(names),
        })
    }

    fn parse_namespace(&mut self) -> Option<CstDecl> {
        self.advance(); // consume @namespace
        // Namespace name may be qualified: `@namespace Core::Swarm { ... }`
        let name = self.parse_qualified_name_with_keywords();
        let name = match name {
            Some(n) if !n.is_empty() => n,
            _ => { self.error("expected namespace name"); return None; }
        };
        self.consume(TokenKind::LBrace, "expected '{' after @namespace name");
        let mut decls = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.check(TokenKind::Eof) {
            if let Some(d) = self.parse_declaration() {
                decls.push(d);
            } else {
                self.advance();
            }
        }
        self.consume(TokenKind::RBrace, "expected '}' after @namespace");
        Some(CstDecl {
            kind: CstDeclKind::Namespace,
            line: self.previous.line, column: self.previous.column,
            name: Some(name),
            next: None,
            data: CstDeclData::Namespace(decls),
        })
    }

    fn parse_using(&mut self) -> Option<CstDecl> {
        self.advance(); // consume @using

        // @using namespace Name;
        let is_ns = self.match_keyword(KeywordKind::AtNamespace) ||
            (self.current.kind == TokenKind::Identifier && self.current_text() == "namespace" && { self.advance(); true });
        if is_ns {
            // Namespace may be qualified: `@using namespace Network::Extensions;`
            if let Some(fqn) = self.parse_qualified_name_with_keywords() {
                if !fqn.is_empty() {
                    self.consume(TokenKind::Semicolon, "expected ';' after @using namespace");
                    return Some(CstDecl {
                        kind: CstDeclKind::Using,
                        line: self.previous.line, column: self.previous.column,
                        name: None,
                        next: None,
                        data: CstDeclData::Using { fqn, alias: None },
                    });
                }
            }
            self.consume(TokenKind::Semicolon, "expected ';' after @using namespace");
            return None;
        }

        // @using Alias = FQN; or @using FQN;
        let mut alias = None;
        let mut fqn = String::new();

        if self.current.kind == TokenKind::Identifier {
            let first = self.current_text().to_string();
            self.advance();

            // Check for Alias = FQN pattern
            if self.match_token(TokenKind::Assign) {
                alias = Some(first.clone());
                // Use parse_type_full (not parse_qualified_name) so that the
                // FQN may contain protocol qualifiers (`id<P>`) and generic
                // type arguments (`VectorBuffer<T*>`). Render the parsed type
                // back to a source-level fqn string for the symbol table.
                if let Some(t) = self.parse_type_full() {
                    fqn = Self::type_to_fqn(&t);
                }
                // Register the alias as a type name so that subsequent
                // `Alias *var = ...` declarations are recognized as declarations
                // (is_declaration_start relies on type_names membership).
                self.add_type_name(&first);
            } else {
                fqn = first;
                while self.match_token(TokenKind::ColonColon) {
                    fqn.push_str("::");
                    if self.current.kind == TokenKind::Identifier {
                        fqn.push_str(self.current_text());
                        self.advance();
                    } else {
                        break;
                    }
                }
                // @using Namespace::Class; — register the short (last) name
                // as a type name so that `Class *var = ...` parses as a declaration.
                if let Some(short) = fqn.rsplit("::").next() {
                    if !short.is_empty() && short != fqn.as_str() {
                        self.add_type_name(short);
                    }
                }
            }
        }

        self.consume(TokenKind::Semicolon, "expected ';' after @using");
        Some(CstDecl {
            kind: CstDeclKind::Using,
            line: self.previous.line, column: self.previous.column,
            name: None,
            next: None,
            data: CstDeclData::Using { fqn, alias },
        })
    }

    // ─── Top-level ──────────────────────────────────────────────────────

    pub fn parse_translation_unit(&mut self) -> Option<TranslationUnit> {
        let mut decls = Vec::new();
        while self.current.kind != TokenKind::Eof {
            if let Some(d) = self.parse_declaration() {
                decls.push(d);
            } else {
                self.advance();
            }
        }
        Some(TranslationUnit {
            decls,
            filename: String::new(),
        })
    }

    pub fn has_error(&self) -> bool {
        self.has_error
    }

    pub fn last_error(&self) -> &str {
        &self.err_msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let mut p = Parser::new("");
        let unit = p.parse_translation_unit();
        assert!(unit.is_some());
        assert_eq!(unit.unwrap().decls.len(), 0);
    }

    #[test]
    fn test_parse_integer_var() {
        let mut p = Parser::new("int x = 42;");
        let unit = p.parse_translation_unit().unwrap();
        assert_eq!(unit.decls.len(), 1);
        let decl = &unit.decls[0];
        assert_eq!(decl.kind, CstDeclKind::Variable);
        if let CstDeclData::Variable { ref var_type, .. } = decl.data {
            assert!(var_type.is_some());
        }
    }

    #[test]
    fn test_parse_function() {
        let mut p = Parser::new("int foo() { return 0; }");
        let unit = p.parse_translation_unit().unwrap();
        assert_eq!(unit.decls.len(), 1, "expected 1 decl, got {}, err={}", unit.decls.len(), p.last_error());
        let decl = &unit.decls[0];
        assert_eq!(decl.kind, CstDeclKind::Function);
    }

    #[test]
    fn test_parse_function_with_param() {
        let mut p = Parser::new("int foo(int val) { return val; }");
        let unit = p.parse_translation_unit().unwrap();
        assert_eq!(unit.decls.len(), 1, "expected 1 decl, got {}, err={}", unit.decls.len(), p.last_error());
        let decl = &unit.decls[0];
        assert_eq!(decl.kind, CstDeclKind::Function);
    }

    #[test]
    fn test_parse_failed() {
        let mut p = Parser::new("struct { int x; }");
        let unit = p.parse_translation_unit();
        assert!(unit.is_some() || p.has_error());
    }

    #[test]
    fn test_debug_nupa_alloc() {
        let source = r#"id nupa_alloc(struct NPClass *cls);"#;
        let mut p = Parser::new(source);
        let unit = p.parse_translation_unit().unwrap();
        println!("decls: {}", unit.decls.len());
        for d in &unit.decls {
            println!("kind: {:?}", d.kind);
            println!("name: {:?}", d.name);
            if let nupa_cst::CstDeclData::Function { ref return_type, ref params, .. } = d.data {
                println!("return_type: {:?}", return_type);
                if let Some(ref p) = params {
                    let mut q: Option<&Box<nupa_cst::CstParam>> = Some(p);
                    while let Some(param) = q {
                        println!("  param name: {:?}", param.name);
                        println!("  param type: {:?}", param.par_type);
                        if let Some(ref t) = param.par_type {
                            println!("    prim: {:?}", t.prim);
                            println!("    name: {:?}", t.name);
                            println!("    is_struct: {}", t.is_struct);
                            println!("    is_pointer: {}", t.is_pointer);
                        }
                        q = param.next.as_ref();
                    }
                }
            }
        }
        assert_eq!(unit.decls.len(), 1);
    }
}