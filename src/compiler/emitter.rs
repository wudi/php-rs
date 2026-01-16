use crate::compiler::chunk::{CatchEntry, CodeChunk, FuncParam, ReturnType, UserFunc};
use crate::core::interner::Interner;
use crate::core::value::{Symbol, Val, Visibility};
use crate::parser::ast::{
    AssignOp, AttributeGroup, BinaryOp, CastKind, ClassMember, Expr, IncludeKind, MagicConstKind,
    Name, Stmt, StmtId, TraitAdaptation, Type, UnaryOp, UseKind,
};
use crate::parser::lexer::token::{Token, TokenKind};
use crate::parser::span::Span;
use crate::vm::opcode::OpCode;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

/// Unescape a double-quoted string, processing escape sequences like \n, \r, \t, etc.
fn unescape_string(s: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' && i + 1 < s.len() {
            match s[i + 1] {
                b'n' => result.push(b'\n'),
                b'r' => result.push(b'\r'),
                b't' => result.push(b'\t'),
                b'\\' => result.push(b'\\'),
                b'$' => result.push(b'$'),
                b'"' => result.push(b'"'),
                b'\'' => result.push(b'\''),
                b'v' => result.push(b'\x0B'), // vertical tab
                b'e' => result.push(b'\x1B'), // escape
                b'f' => result.push(b'\x0C'), // form feed
                b'0' => result.push(b'\0'),   // null byte
                // Hexadecimal: \xHH
                b'x' if i + 3 < s.len() => {
                    if let (Some(h1), Some(h2)) = (
                        char::from(s[i + 2]).to_digit(16),
                        char::from(s[i + 3]).to_digit(16),
                    ) {
                        result.push((h1 * 16 + h2) as u8);
                        i += 2; // Skip the two hex digits
                    } else {
                        result.push(b'\\');
                        result.push(s[i + 1]);
                    }
                }
                // Octal: \nnn (up to 3 digits)
                b'0'..=b'7' => {
                    let mut octal_val = s[i + 1] - b'0';
                    let mut consumed = 1;
                    if i + 2 < s.len() && (b'0'..=b'7').contains(&s[i + 2]) {
                        octal_val = octal_val * 8 + (s[i + 2] - b'0');
                        consumed = 2;
                        if i + 3 < s.len() && (b'0'..=b'7').contains(&s[i + 3]) {
                            octal_val = octal_val * 8 + (s[i + 3] - b'0');
                            consumed = 3;
                        }
                    }
                    result.push(octal_val);
                    i += consumed;
                }
                _ => {
                    // Unknown escape, keep both characters
                    result.push(b'\\');
                    result.push(s[i + 1]);
                }
            }
            i += 2;
        } else {
            result.push(s[i]);
            i += 1;
        }
    }
    result
}

struct LoopInfo {
    break_jumps: Vec<usize>,
    continue_jumps: Vec<usize>,
}

#[derive(Clone)]
struct TryFinallyInfo {
    /// Index in catch_table for the finally-only entry
    catch_table_idx: usize,
    /// Start of the finally block code
    finally_start: u32,
    /// End of the finally block code (exclusive)
    finally_end: u32,
}

pub struct Emitter<'src> {
    chunk: CodeChunk,
    source: &'src [u8],
    interner: &'src mut Interner,
    loop_stack: Vec<LoopInfo>,
    try_finally_stack: Vec<TryFinallyInfo>,
    is_generator: bool,
    current_line: u32,
    // Context for magic constants
    file_path: Option<String>,
    current_class: Option<Symbol>,
    current_trait: Option<Symbol>,
    current_function: Option<Symbol>,
    current_namespace: Option<Symbol>,
    use_aliases: HashMap<Vec<u8>, Vec<u8>>,
    // For eval(): inherit strict_types from parent scope if not explicitly declared
    inherited_strict_types: Option<bool>,
    // Counter for anonymous classes
    anonymous_class_counter: usize,
}

impl<'src> Emitter<'src> {
    pub fn new(source: &'src [u8], interner: &'src mut Interner) -> Self {
        Self {
            chunk: CodeChunk::default(),
            source,
            interner,
            loop_stack: Vec::new(),
            try_finally_stack: Vec::new(),
            is_generator: false,
            current_line: 1,
            file_path: None,
            current_class: None,
            current_trait: None,
            current_function: None,
            current_namespace: None,
            use_aliases: HashMap::new(),
            inherited_strict_types: None,
            anonymous_class_counter: 0,
        }
    }

    /// Create an emitter with a file path for accurate __FILE__ and __DIR__
    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Set inherited strict_types for eval() - applies only if no explicit declare
    pub fn with_inherited_strict_types(mut self, strict: bool) -> Self {
        self.inherited_strict_types = Some(strict);
        self
    }

    fn get_visibility(&self, modifiers: &[Token]) -> Visibility {
        for token in modifiers {
            match token.kind {
                TokenKind::Public => return Visibility::Public,
                TokenKind::Protected => return Visibility::Protected,
                TokenKind::Private => return Visibility::Private,
                _ => {}
            }
        }
        Visibility::Public // Default
    }

    /// Generate a unique name for an anonymous class
    fn generate_anonymous_class_name(&mut self, parent_name: Option<&[u8]>, span: &Span) -> String {
        let base_name = parent_name
            .map(|p| format!("{}@anonymous", String::from_utf8_lossy(p)))
            .unwrap_or_else(|| "class@anonymous".to_string());

        let suffix = self
            .file_path
            .as_ref()
            .map(|path| format!("\0{}:{}", path, span.start))
            .unwrap_or_else(|| format!("\0{}", self.anonymous_class_counter));

        self.anonymous_class_counter += 1;
        format!("{}{}", base_name, suffix)
    }

    /// Emit common class definition opcodes (attributes, modifiers, interfaces)
    fn emit_class_metadata(
        &mut self,
        class_sym: Symbol,
        attributes: &[AttributeGroup],
        modifiers: &[Token],
        implements: &[Name],
    ) {
        // Handle attributes if any
        if !attributes.is_empty() {
            let attr_val = self.build_attribute_list(attributes);
            let idx = self.add_constant(attr_val);
            self.chunk
                .code
                .push(OpCode::SetClassAttributes(class_sym, idx as u16));
        }

        // Handle modifiers (abstract, final, readonly)
        if modifiers.iter().any(|m| m.kind == TokenKind::Abstract) {
            self.push_op(OpCode::MarkAbstract(class_sym));
        }

        if modifiers.iter().any(|m| m.kind == TokenKind::Final) {
            self.push_op(OpCode::MarkFinal(class_sym));
        }

        if modifiers.iter().any(|m| m.kind == TokenKind::Readonly) {
            self.push_op(OpCode::MarkReadonly(class_sym));
        }

        // Handle interfaces
        for interface_name in implements {
            let interface_sym = self.resolve_class_sym_from_name(interface_name);
            self.chunk
                .code
                .push(OpCode::AddInterface(class_sym, interface_sym));
        }
    }

    fn name_bytes(&self, name: &Name) -> Vec<u8> {
        self.get_text(name.span).to_vec()
    }

    fn strip_leading_backslash(name: &[u8]) -> &[u8] {
        if name.first() == Some(&b'\\') {
            &name[1..]
        } else {
            name
        }
    }

    fn qualify_declaration_name(&self, name: &[u8]) -> Vec<u8> {
        let trimmed = Self::strip_leading_backslash(name);
        if let Some(ns_sym) = self.current_namespace {
            if let Some(ns_bytes) = self.interner.lookup(ns_sym) {
                if !ns_bytes.is_empty() {
                    let mut full = Vec::with_capacity(ns_bytes.len() + 1 + trimmed.len());
                    full.extend_from_slice(ns_bytes);
                    full.push(b'\\');
                    full.extend_from_slice(trimmed);
                    return full;
                }
            }
        }
        trimmed.to_vec()
    }

    fn resolve_class_name(&self, name: &[u8]) -> Vec<u8> {
        if name.eq_ignore_ascii_case(b"self")
            || name.eq_ignore_ascii_case(b"static")
            || name.eq_ignore_ascii_case(b"parent")
        {
            return name.to_vec();
        }

        let trimmed = Self::strip_leading_backslash(name);
        let mut split = trimmed.splitn(2, |b| *b == b'\\');
        let first = split.next().unwrap_or(&[]);
        let rest = split.next();
        let key = first.to_ascii_lowercase();

        if let Some(alias) = self.use_aliases.get(&key) {
            if let Some(rest) = rest {
                let mut full = Vec::with_capacity(alias.len() + 1 + rest.len());
                full.extend_from_slice(alias);
                full.push(b'\\');
                full.extend_from_slice(rest);
                return full;
            }
            return alias.clone();
        }

        if name.first() != Some(&b'\\') {
            if let Some(ns_sym) = self.current_namespace {
                if let Some(ns_bytes) = self.interner.lookup(ns_sym) {
                    if !ns_bytes.is_empty() {
                        let mut full = Vec::with_capacity(ns_bytes.len() + 1 + trimmed.len());
                        full.extend_from_slice(ns_bytes);
                        full.push(b'\\');
                        full.extend_from_slice(trimmed);
                        return full;
                    }
                }
            }
        }

        trimmed.to_vec()
    }

    fn resolve_class_sym_from_span(&mut self, span: Span) -> Symbol {
        let name = self.get_text(span);
        let resolved = self.resolve_class_name(name);
        self.interner.intern(&resolved)
    }

    fn resolve_class_sym_from_name(&mut self, name: &Name) -> Symbol {
        let resolved = self.resolve_class_name(&self.name_bytes(name));
        self.interner.intern(&resolved)
    }

    fn declare_class_sym_from_span(&mut self, span: Span) -> Symbol {
        let name = self.get_text(span);
        let qualified = self.qualify_declaration_name(name);
        self.interner.intern(&qualified)
    }

    fn emit_toplevel_decls(&mut self, stmts: &[StmtId]) {
        for stmt in stmts {
            match stmt {
                Stmt::Namespace { name, body, .. } => {
                    let ns_sym = name.as_ref().map(|ns| {
                        let ns_bytes = self.name_bytes(ns);
                        let trimmed = Self::strip_leading_backslash(&ns_bytes);
                        self.interner.intern(trimmed)
                    });

                    if let Some(body) = body {
                        let prev_namespace = self.current_namespace;
                        let prev_aliases = self.use_aliases.clone();
                        self.current_namespace = ns_sym;
                        self.use_aliases.clear();
                        self.emit_toplevel_decls(body);
                        self.current_namespace = prev_namespace;
                        self.use_aliases = prev_aliases;
                    } else {
                        self.current_namespace = ns_sym;
                        self.use_aliases.clear();
                    }
                }
                Stmt::Use { uses, kind, .. } => {
                    if *kind != UseKind::Normal {
                        continue;
                    }

                    for item in *uses {
                        let name_bytes = self.name_bytes(&item.name);
                        let full_name = Self::strip_leading_backslash(&name_bytes);
                        let alias = if let Some(alias) = item.alias {
                            self.get_text(alias.span).to_vec()
                        } else {
                            full_name
                                .rsplit(|b| *b == b'\\')
                                .next()
                                .unwrap_or(full_name)
                                .to_vec()
                        };
                        self.use_aliases
                            .insert(alias.to_ascii_lowercase(), full_name.to_vec());
                    }
                }
                Stmt::Function { .. } => {
                    self.emit_stmt(stmt);
                }
                _ => {}
            }
        }
    }

    /// Parse an integer literal from bytes, handling '_' separators.
    /// Returns the parsed u64 value.
    fn parse_integer_literal(&self, value: &[u8]) -> u64 {
        let mut num: u64 = 0;
        for b in value {
            if *b == b'_' {
                continue;
            }
            if !b.is_ascii_digit() {
                return 0;
            }
            num = num.saturating_mul(10).saturating_add((b - b'0') as u64);
        }
        num
    }

    /// Process declare(strict_types=...) statements early, before function compilation.
    /// Returns true if an explicit strict_types declaration was found.
    fn process_strict_types_early(&mut self, stmts: &[StmtId]) -> bool {
        for stmt in stmts {
            if let Stmt::Declare { declares, .. } = stmt {
                for item in *declares {
                    let key = self.get_text(item.key.span);
                    if key.eq_ignore_ascii_case(b"strict_types") {
                        if let Expr::Integer { value, .. } = item.value {
                            let num = self.parse_integer_literal(value);
                            self.chunk.strict_types = num == 1;
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    fn emit_toplevel_stmts(&mut self, stmts: &[StmtId]) {
        for stmt in stmts {
            match stmt {
                Stmt::Namespace { name, body, .. } => {
                    let ns_sym = name.as_ref().map(|ns| {
                        let ns_bytes = self.name_bytes(ns);
                        let trimmed = Self::strip_leading_backslash(&ns_bytes);
                        self.interner.intern(trimmed)
                    });

                    if let Some(body) = body {
                        let prev_namespace = self.current_namespace;
                        let prev_aliases = self.use_aliases.clone();
                        self.current_namespace = ns_sym;
                        self.use_aliases.clear();
                        self.emit_toplevel_stmts(body);
                        self.current_namespace = prev_namespace;
                        self.use_aliases = prev_aliases;
                    } else {
                        self.current_namespace = ns_sym;
                        self.use_aliases.clear();
                    }
                }
                Stmt::Use { uses, kind, .. } => {
                    if *kind != UseKind::Normal {
                        continue;
                    }

                    for item in *uses {
                        let name_bytes = self.name_bytes(&item.name);
                        let full_name = Self::strip_leading_backslash(&name_bytes);
                        let alias = if let Some(alias) = item.alias {
                            self.get_text(alias.span).to_vec()
                        } else {
                            full_name
                                .rsplit(|b| *b == b'\\')
                                .next()
                                .unwrap_or(full_name)
                                .to_vec()
                        };
                        self.use_aliases
                            .insert(alias.to_ascii_lowercase(), full_name.to_vec());
                    }
                }
                Stmt::Function { .. } => {
                    continue;
                }
                _ => self.emit_stmt(stmt),
            }
        }
    }

    fn set_current_line(&mut self, span: Span) {
        if let Some(info) = span.line_info(self.source) {
            self.current_line = info.line as u32;
        }
    }

    fn push_op(&mut self, op: OpCode) {
        self.chunk.code.push(op);
        self.chunk.lines.push(self.current_line);
    }

    pub fn compile(mut self, stmts: &[StmtId]) -> (CodeChunk, bool) {
        // Process declare(strict_types=...) FIRST before anything else
        // This ensures functions inherit the correct strict_types setting
        let has_explicit_strict_types = self.process_strict_types_early(stmts);

        // Apply inherited strictness only if no explicit declare
        if !has_explicit_strict_types {
            if let Some(inherited) = self.inherited_strict_types {
                self.chunk.strict_types = inherited;
            }
        }

        if self.current_function.is_none() {
            self.emit_toplevel_decls(stmts);
            self.emit_toplevel_stmts(stmts);
        } else {
            for stmt in stmts {
                self.emit_stmt(stmt);
            }
        }

        // Implicit return:
        // - Functions/methods: return null if no explicit return
        // - Top-level scripts: NO implicit return (PHP returns 1 for include, or the last statement result)
        if self.current_function.is_some() {
            // Inside a function - add implicit return null
            let null_idx = self.add_constant(Val::Null);
            self.push_op(OpCode::Const(null_idx as u16));
            self.push_op(OpCode::Return);
        }
        // Note: Top-level scripts don't get implicit return null

        let chunk_name = if let Some(func_sym) = self.current_function {
            func_sym
        } else if let Some(path) = &self.file_path {
            self.interner.intern(path.as_bytes())
        } else {
            self.interner.intern(b"(unknown)")
        };
        self.chunk.name = chunk_name;
        self.chunk.file_path = self.file_path.clone();

        (self.chunk, self.is_generator)
    }

    fn emit_members(&mut self, class_sym: Symbol, members: &[ClassMember]) {
        for member in members {
            match member {
                ClassMember::Method {
                    attributes,
                    name,
                    body,
                    params,
                    modifiers,
                    return_type,
                    span,
                    close_brace_span,
                    ..
                } => {
                    let method_name_str = self.get_text(name.span);
                    let method_sym = self.interner.intern(method_name_str);
                    let visibility = self.get_visibility(modifiers);
                    let is_static = modifiers.iter().any(|t| t.kind == TokenKind::Static);
                    let is_abstract = modifiers.iter().any(|t| t.kind == TokenKind::Abstract);
                    let is_final = modifiers.iter().any(|t| t.kind == TokenKind::Final);

                    // 1. Collect param info
                    struct ParamInfo<'a> {
                        name_span: crate::parser::span::Span,
                        by_ref: bool,
                        ty: Option<&'a Type<'a>>,
                        default: Option<&'a Expr<'a>>,
                        variadic: bool,
                    }

                    let mut param_infos = Vec::new();
                    for param in *params {
                        param_infos.push(ParamInfo {
                            name_span: param.name.span,
                            by_ref: param.by_ref,
                            ty: param.ty,
                            default: param.default.as_ref().map(|e| *e),
                            variadic: param.variadic,
                        });
                    }

                    // 2. Create emitter with inherited context
                    let mut method_emitter = Emitter::new(self.source, self.interner);
                    method_emitter.file_path = self.file_path.clone();
                    method_emitter.current_class = Some(class_sym);
                    method_emitter.current_namespace = self.current_namespace;
                    method_emitter.use_aliases = self.use_aliases.clone();
                    method_emitter.chunk.strict_types = self.chunk.strict_types;

                    // Build method name after creating method_emitter to avoid borrow issues
                    let method_name_full = {
                        let class_name = method_emitter.interner.lookup(class_sym).unwrap_or(b"");
                        let mut full = class_name.to_vec();
                        full.extend_from_slice(b"::");
                        full.extend_from_slice(method_name_str);
                        method_emitter.interner.intern(&full)
                    };
                    method_emitter.current_function = Some(method_name_full);

                    // 3. Process params
                    let mut param_syms = Vec::new();
                    for (i, info) in param_infos.iter().enumerate() {
                        let p_name = method_emitter.get_text(info.name_span);
                        if p_name.starts_with(b"$") {
                            let sym = method_emitter.interner.intern(&p_name[1..]);
                            let param_type = info.ty.and_then(|ty| method_emitter.convert_type(ty));
                        let default_value = if info.variadic {
                            None
                        } else {
                            info.default
                                .map(|expr| method_emitter.eval_constant_expr(expr))
                        };

                            param_syms.push(FuncParam {
                                name: sym,
                                by_ref: info.by_ref,
                                param_type,
                                is_variadic: info.variadic,
                                default_value,
                            });

                        if info.variadic {
                            method_emitter
                                .chunk
                                .code
                                .push(OpCode::RecvVariadic(i as u32));
                        } else if let Some(default_expr) = info.default {
                            let val = method_emitter.eval_constant_expr(default_expr);
                            let idx = method_emitter.add_constant(val);
                            method_emitter
                                .chunk
                                .code
                                .push(OpCode::RecvInit(i as u32, idx as u16));
                        } else {
                            method_emitter.push_op(OpCode::Recv(i as u32));
                        }
                        }
                    }

                    let (method_chunk, is_generator) = method_emitter.compile(body);

                    let start_line = span.line_info(self.source).map(|li| li.line as u32);
                    let end_line = close_brace_span
                        .and_then(|s| s.line_info(self.source).map(|li| li.line as u32));

                    // Convert return type
                    let ret_type = return_type.and_then(|rt| self.convert_type(rt));

                    let user_func = UserFunc {
                        params: param_syms,
                        uses: Vec::new(),
                        chunk: Rc::new(method_chunk),
                        is_static,
                        is_generator,
                        statics: Rc::new(RefCell::new(HashMap::new())),
                        return_type: ret_type,
                        start_line,
                        end_line,
                    };

                    // Store in constants
                    let func_res = Val::Resource(Rc::new(user_func));
                    let const_idx = self.add_constant(func_res);

                    self.push_op(OpCode::DefMethod(
                        class_sym,
                        method_sym,
                        const_idx as u32,
                        visibility,
                        is_static,
                        is_abstract,
                        is_final,
                    ));

                    if !attributes.is_empty() {
                        let attr_val = self.build_attribute_list(attributes);
                        let idx = self.add_constant(attr_val);
                        self.push_op(OpCode::SetMethodAttributes(
                            class_sym, method_sym, idx as u16,
                        ));
                    }
                }
                ClassMember::Property {
                    attributes,
                    entries,
                    modifiers,
                    ty,
                    doc_comment,
                    ..
                } => {
                    let visibility = self.get_visibility(modifiers);
                    let is_static = modifiers.iter().any(|t| t.kind == TokenKind::Static);
                    let is_readonly = modifiers.iter().any(|t| t.kind == TokenKind::Readonly);
                    let doc_comment_idx = doc_comment.map(|doc_comment| {
                        let comment = self.source[doc_comment.start..doc_comment.end].to_vec();
                        self.add_constant(Val::String(Rc::new(comment)))
                    });
                    let attr_idx = if !attributes.is_empty() {
                        let attr_val = self.build_attribute_list(attributes);
                        Some(self.add_constant(attr_val))
                    } else {
                        None
                    };

                    // Extract type hint once for all properties in this declaration
                    let type_hint_opt = ty.and_then(|t| self.convert_type(t));
                    let type_hint_idx = if let Some(ref th) = type_hint_opt {
                        self.add_constant(Val::Resource(Rc::new(th.clone())))
                    } else {
                        self.add_constant(Val::Null)
                    };

                    for entry in *entries {
                        let prop_name_str = self.get_text(entry.name.span);
                        let prop_name = if prop_name_str.starts_with(b"$") {
                            &prop_name_str[1..]
                        } else {
                            prop_name_str
                        };
                        let prop_sym = self.interner.intern(prop_name);

                        let default_idx = if let Some(default_expr) = entry.default {
                            let val = self.eval_constant_expr(default_expr);
                            self.add_constant(val)
                        } else {
                            if type_hint_opt.is_some() || is_readonly {
                                self.add_constant(Val::Uninitialized)
                            } else {
                                self.add_constant(Val::Null)
                            }
                        };

                        if is_static {
                            self.push_op(OpCode::DefStaticProp(
                                class_sym,
                                prop_sym,
                                default_idx as u16,
                                visibility,
                                type_hint_idx as u32,
                            ));
                        } else {
                            self.push_op(OpCode::DefProp(
                                class_sym,
                                prop_sym,
                                default_idx as u16,
                                visibility,
                                type_hint_idx as u32,
                                is_readonly,
                            ));
                        }

                        if let Some(doc_comment_idx) = doc_comment_idx {
                            self.push_op(OpCode::SetPropertyDocComment(
                                class_sym,
                                prop_sym,
                                doc_comment_idx as u16,
                            ));
                        }

                        if let Some(attr_idx) = attr_idx {
                            self.push_op(OpCode::SetPropertyAttributes(
                                class_sym,
                                prop_sym,
                                attr_idx as u16,
                            ));
                        }
                    }
                }
                ClassMember::Const {
                    attributes,
                    consts,
                    modifiers,
                    doc_comment,
                    ..
                } => {
                    let visibility = self.get_visibility(modifiers);
                    let doc_comment_idx = doc_comment.map(|doc_comment| {
                        let comment = self.source[doc_comment.start..doc_comment.end].to_vec();
                        self.add_constant(Val::String(Rc::new(comment)))
                    });
                    let attr_idx = if !attributes.is_empty() {
                        let attr_val = self.build_attribute_list(attributes);
                        Some(self.add_constant(attr_val))
                    } else {
                        None
                    };
                    for entry in *consts {
                        let const_name_str = self.get_text(entry.name.span);
                        let const_sym = self.interner.intern(const_name_str);

                        let val = self
                            .get_literal_value(entry.value)
                            .unwrap_or_else(|| Val::Null);
                        let val_idx = self.add_constant(val);
                        self.push_op(OpCode::DefClassConst(
                            class_sym,
                            const_sym,
                            val_idx as u16,
                            visibility,
                        ));

                        if let Some(doc_comment_idx) = doc_comment_idx {
                            self.push_op(OpCode::SetClassConstDocComment(
                                class_sym,
                                const_sym,
                                doc_comment_idx as u16,
                            ));
                        }

                        if let Some(attr_idx) = attr_idx {
                            self.push_op(OpCode::SetClassConstAttributes(
                                class_sym,
                                const_sym,
                                attr_idx as u16,
                            ));
                        }
                    }
                }
                ClassMember::TraitUse {
                    traits,
                    adaptations,
                    ..
                } => {
                    for trait_name in *traits {
                        let trait_str = self.get_text(trait_name.span);
                        let trait_sym = self.interner.intern(trait_str);
                        self.push_op(OpCode::UseTrait(class_sym, trait_sym));
                    }
                    for adaptation in *adaptations {
                        if let TraitAdaptation::Alias {
                            method,
                            alias,
                            visibility,
                            ..
                        } = adaptation
                        {
                            let Some(alias) = alias else {
                                continue;
                            };
                            let alias_name = self.get_text(alias.span);
                            let alias_sym = self.interner.intern(alias_name);

                            let method_name = self.get_text(method.method.span);
                            let method_sym = self.interner.intern(method_name);

                            let trait_sym = method.trait_name.map(|name| {
                                let trait_name = self.get_text(name.span);
                                self.interner.intern(trait_name)
                            });

                            let vis = visibility.and_then(|token| match token.kind {
                                TokenKind::Public => Some(Visibility::Public),
                                TokenKind::Protected => Some(Visibility::Protected),
                                TokenKind::Private => Some(Visibility::Private),
                                _ => None,
                            });

                            self.push_op(OpCode::SetTraitAlias(
                                class_sym, alias_sym, trait_sym, method_sym, vis,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn emit_stmt(&mut self, stmt: &Stmt) {
        self.set_current_line(stmt.span());
        match stmt {
            Stmt::Declare { declares, body, .. } => {
                // PHP: declare(strict_types=1) is per-file and affects calls made from this file.
                // The parser already validates strict_types is an integer literal 0/1.
                for item in *declares {
                    let key = self.get_text(item.key.span);
                    if key.eq_ignore_ascii_case(b"strict_types") {
                        if let Expr::Integer { value, .. } = item.value {
                            let num = self.parse_integer_literal(value);
                            self.chunk.strict_types = num == 1;
                        }
                    }
                }

                for s in *body {
                    self.emit_stmt(s);
                }
            }
            Stmt::Namespace { name, body, .. } => {
                let ns_sym = name.as_ref().map(|ns| {
                    let ns_bytes = self.name_bytes(ns);
                    let trimmed = Self::strip_leading_backslash(&ns_bytes);
                    self.interner.intern(trimmed)
                });

                if let Some(body) = body {
                    let prev_namespace = self.current_namespace;
                    let prev_aliases = self.use_aliases.clone();
                    self.current_namespace = ns_sym;
                    self.use_aliases.clear();
                    for s in *body {
                        self.emit_stmt(s);
                    }
                    self.current_namespace = prev_namespace;
                    self.use_aliases = prev_aliases;
                } else {
                    self.current_namespace = ns_sym;
                    self.use_aliases.clear();
                }
            }
            Stmt::Use { uses, kind, .. } => {
                if *kind != UseKind::Normal {
                    return;
                }

                for item in *uses {
                    let name_bytes = self.name_bytes(&item.name);
                    let full_name = Self::strip_leading_backslash(&name_bytes);
                    let alias = if let Some(alias) = item.alias {
                        self.get_text(alias.span).to_vec()
                    } else {
                        full_name
                            .rsplit(|b| *b == b'\\')
                            .next()
                            .unwrap_or(full_name)
                            .to_vec()
                    };
                    self.use_aliases
                        .insert(alias.to_ascii_lowercase(), full_name.to_vec());
                }
            }
            Stmt::Echo { exprs, .. } => {
                for expr in *exprs {
                    self.emit_expr(expr);
                    self.push_op(OpCode::Echo);
                }
            }
            Stmt::InlineHtml { value, .. } => {
                // Output inline HTML/text that appears outside PHP tags
                // Convert the raw bytes to a string constant and echo it
                let idx = self.add_constant(Val::String(value.to_vec().into()));
                self.push_op(OpCode::Const(idx as u16));
                self.push_op(OpCode::Echo);
            }
            Stmt::Expression { expr, .. } => {
                self.emit_expr(expr);
                self.push_op(OpCode::Pop);
            }
            Stmt::Return { expr, .. } => {
                if let Some(e) = expr {
                    self.emit_expr(e);
                } else {
                    let idx = self.add_constant(Val::Null);
                    self.push_op(OpCode::Const(idx as u16));
                }
                // Return type checking is now done in the Return handler
                self.push_op(OpCode::Return);
            }
            Stmt::Const { consts, .. } => {
                for c in *consts {
                    let name_str = self.get_text(c.name.span);
                    let sym = self.interner.intern(name_str);

                    // Value must be constant expression.
                    // For now, we only support literals or simple expressions we can evaluate at compile time?
                    // Or we can emit code to evaluate it and then define it?
                    // PHP `const` requires constant expression.
                    // If we emit code, we can use `DefGlobalConst` which takes a value index?
                    // No, `DefGlobalConst` takes `val_idx` which implies it's in the constant table.
                    // So we must evaluate it at compile time.

                    let val = self.get_literal_value(c.value).unwrap_or_else(|| Val::Null);

                    let val_idx = self.add_constant(val);
                    self.chunk
                        .code
                        .push(OpCode::DefGlobalConst(sym, val_idx as u16));
                }
            }
            Stmt::Global { vars, .. } => {
                for var in *vars {
                    if let Expr::Variable { span, .. } = var {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let var_name = &name[1..];
                            let sym = self.interner.intern(var_name);
                            self.push_op(OpCode::BindGlobal(sym));
                        }
                    }
                }
            }
            Stmt::Static { vars, .. } => {
                for var in *vars {
                    // Check if var.var is Assign
                    let (target_var, default_expr) = if let Expr::Assign {
                        var: assign_var,
                        expr: assign_expr,
                        ..
                    } = var.var
                    {
                        (*assign_var, Some(*assign_expr))
                    } else {
                        (var.var, var.default)
                    };

                    let name = if let Expr::Variable { span, .. } = target_var {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            self.interner.intern(&name[1..])
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    let val = if let Some(expr) = default_expr {
                        self.eval_constant_expr(expr)
                    } else {
                        Val::Null
                    };

                    let idx = self.add_constant(val);
                    self.push_op(OpCode::BindStatic(name, idx as u16));
                }
            }
            Stmt::Unset { vars, .. } => {
                for var in *vars {
                    match var {
                        Expr::Variable { span, .. } => {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                let sym = self.interner.intern(&name[1..]);
                                self.push_op(OpCode::UnsetVar(sym));
                            }
                        }
                        Expr::IndirectVariable { name, .. } => {
                            self.emit_expr(name);
                            self.push_op(OpCode::UnsetVarDynamic);
                        }
                        Expr::ArrayDimFetch { array, dim, .. } => {
                            if let Expr::Variable { span, .. } = array {
                                let name = self.get_text(*span);
                                if name.starts_with(b"$") {
                                    let sym = self.interner.intern(&name[1..]);
                                    self.push_op(OpCode::LoadVar(sym));
                                    self.push_op(OpCode::Dup);

                                    if let Some(d) = dim {
                                        self.emit_expr(d);
                                    } else {
                                        let idx = self.add_constant(Val::Null);
                                        self.push_op(OpCode::Const(idx as u16));
                                    }

                                    self.push_op(OpCode::UnsetDim);
                                    self.push_op(OpCode::StoreVar(sym));
                                }
                            } else {
                                // Check if this is a property fetch (possibly nested): $obj->prop['key']['key2']...
                                // Use flatten_dim_fetch to get all keys
                                let (base, keys) = Self::flatten_dim_fetch(var);

                                // Check if the base is a PropertyFetch
                                if let Expr::PropertyFetch {
                                    target, property, ..
                                } = base
                                {
                                    // Ensure we have at least one key
                                    if keys.is_empty() {
                                        return; // Shouldn't happen for ArrayDimFetch
                                    }

                                    // Get property name symbol
                                    let prop_sym = if let Expr::Variable { span, .. } = property {
                                        let name = self.get_text(*span);
                                        self.interner.intern(name)
                                    } else {
                                        return; // Can't handle dynamic property names in unset yet
                                    };

                                    // Emit target (obj)
                                    self.emit_expr(target); // [obj]
                                    self.push_op(OpCode::Dup); // [obj, obj]

                                    // Fetch the property
                                    self.push_op(OpCode::FetchProp(prop_sym)); // [obj, array]

                                    // Emit all keys
                                    for key in &keys {
                                        if let Some(k) = key {
                                            self.emit_expr(k);
                                        } else {
                                            let idx = self.add_constant(Val::Null);
                                            self.push_op(OpCode::Const(idx as u16));
                                        }
                                    }

                                    // Unset nested dimension
                                    self.chunk
                                        .code
                                        .push(OpCode::UnsetNestedDim(keys.len() as u8)); // [obj, modified_array]

                                    // Assign back to property
                                    self.push_op(OpCode::AssignProp(prop_sym)); // []
                                    self.push_op(OpCode::Pop); // discard result
                                }
                            }
                        }
                        Expr::PropertyFetch {
                            target, property, ..
                        } => {
                            self.emit_expr(target);
                            if let Expr::Variable { span, .. } = property {
                                let name = self.get_text(*span);
                                let idx = self.add_constant(Val::String(name.to_vec().into()));
                                self.push_op(OpCode::Const(idx as u16));
                                self.push_op(OpCode::UnsetObj);
                            }
                        }
                        Expr::ClassConstFetch {
                            class, constant, ..
                        } => {
                            let is_static_prop = if let Expr::Variable { span, .. } = constant {
                                let name = self.get_text(*span);
                                name.starts_with(b"$")
                            } else {
                                false
                            };

                            if is_static_prop {
                                if let Expr::Variable { span, .. } = class {
                                    let name = self.get_text(*span);
                                    if !name.starts_with(b"$") {
                                        let idx =
                                            self.add_constant(Val::String(name.to_vec().into()));
                                        self.push_op(OpCode::Const(idx as u16));
                                    } else {
                                        let sym = self.interner.intern(&name[1..]);
                                        self.push_op(OpCode::LoadVar(sym));
                                    }

                                    if let Expr::Variable {
                                        span: prop_span, ..
                                    } = constant
                                    {
                                        let prop_name = self.get_text(*prop_span);
                                        let idx = self.add_constant(Val::String(
                                            prop_name[1..].to_vec().into(),
                                        ));
                                        self.push_op(OpCode::Const(idx as u16));
                                        self.push_op(OpCode::UnsetStaticProp);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Stmt::Break { level, .. } => {
                // Determine the break level (default 1)
                let break_level = if let Some(level_expr) = level {
                    // Try to evaluate as a constant integer
                    self.get_literal_value(level_expr)
                        .and_then(|v| match v {
                            Val::Int(i) if i > 0 => Some(i as usize),
                            _ => None,
                        })
                        .unwrap_or(1)
                } else {
                    1
                };

                // Find the target loop (counting from innermost)
                let loop_depth = self.loop_stack.len();
                if break_level > 0 && break_level <= loop_depth {
                    // Calculate which loop to target (from the end of the stack)
                    let target_loop_idx = loop_depth - break_level;

                    let idx = self.chunk.code.len();
                    // Check if we're inside try-finally blocks
                    if !self.try_finally_stack.is_empty() {
                        // Use JmpFinally which will execute finally blocks at runtime
                        self.push_op(OpCode::JmpFinally(0)); // Patch later
                    } else {
                        // Normal jump
                        self.push_op(OpCode::Jmp(0)); // Patch later
                    }

                    // Register the jump with the target loop
                    self.loop_stack[target_loop_idx].break_jumps.push(idx);
                }
            }
            Stmt::Continue { level, .. } => {
                // Determine the continue level (default 1)
                let continue_level = if let Some(level_expr) = level {
                    // Try to evaluate as a constant integer
                    self.get_literal_value(level_expr)
                        .and_then(|v| match v {
                            Val::Int(i) if i > 0 => Some(i as usize),
                            _ => None,
                        })
                        .unwrap_or(1)
                } else {
                    1
                };

                // Find the target loop (counting from innermost)
                let loop_depth = self.loop_stack.len();
                if continue_level > 0 && continue_level <= loop_depth {
                    // Calculate which loop to target (from the end of the stack)
                    let target_loop_idx = loop_depth - continue_level;

                    let idx = self.chunk.code.len();
                    // Check if we're inside try-finally blocks
                    if !self.try_finally_stack.is_empty() {
                        // Use JmpFinally which will execute finally blocks at runtime
                        self.push_op(OpCode::JmpFinally(0)); // Patch later
                    } else {
                        // Normal jump
                        self.push_op(OpCode::Jmp(0)); // Patch later
                    }

                    // Register the jump with the target loop
                    self.loop_stack[target_loop_idx].continue_jumps.push(idx);
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                self.emit_expr(condition);

                let jump_false_idx = self.chunk.code.len();
                self.push_op(OpCode::JmpIfFalse(0));

                for stmt in *then_block {
                    self.emit_stmt(stmt);
                }

                let jump_end_idx = self.chunk.code.len();
                self.push_op(OpCode::Jmp(0));

                let else_label = self.chunk.code.len();
                self.patch_jump(jump_false_idx, else_label);

                if let Some(else_stmts) = else_block {
                    for stmt in *else_stmts {
                        self.emit_stmt(stmt);
                    }
                }

                let end_label = self.chunk.code.len();
                self.patch_jump(jump_end_idx, end_label);
            }
            Stmt::Function {
                attributes,
                name,
                params,
                body,
                by_ref,
                return_type,
                span,
                close_brace_span,
                ..
            } => {
                let func_name_str = self.get_text(name.span);
                let func_name = self.qualify_declaration_name(func_name_str);
                let func_sym = self.interner.intern(&func_name);

                // 1. Collect param info to avoid borrow issues
                struct ParamInfo<'a> {
                    name_span: crate::parser::span::Span,
                    by_ref: bool,
                    ty: Option<&'a Type<'a>>,
                    default: Option<&'a Expr<'a>>,
                    variadic: bool,
                }

                let mut param_infos = Vec::new();
                for param in *params {
                    param_infos.push(ParamInfo {
                        name_span: param.name.span,
                        by_ref: param.by_ref,
                        ty: param.ty,
                        default: param.default.as_ref().map(|e| *e),
                        variadic: param.variadic,
                    });
                }

                // 2. Create emitter with inherited context
                let mut func_emitter = Emitter::new(self.source, self.interner);
                func_emitter.file_path = self.file_path.clone();
                func_emitter.current_function = Some(func_sym);
                func_emitter.current_namespace = self.current_namespace;
                func_emitter.use_aliases = self.use_aliases.clone();
                func_emitter.chunk.strict_types = self.chunk.strict_types;

                // 3. Process params using func_emitter
                let mut param_syms = Vec::new();
                for (i, info) in param_infos.iter().enumerate() {
                    let p_name = func_emitter.get_text(info.name_span);
                    if p_name.starts_with(b"$") {
                        let sym = func_emitter.interner.intern(&p_name[1..]);
                        let param_type = info.ty.and_then(|ty| func_emitter.convert_type(ty));
                        let default_value = if info.variadic {
                            None
                        } else {
                            info.default
                                .map(|expr| func_emitter.eval_constant_expr(expr))
                        };

                        param_syms.push(FuncParam {
                            name: sym,
                            by_ref: info.by_ref,
                            param_type,
                            is_variadic: info.variadic,
                            default_value,
                        });

                        if info.variadic {
                            func_emitter
                                .chunk
                                .code
                                .push(OpCode::RecvVariadic(i as u32));
                        } else if let Some(default_expr) = info.default {
                            let val = func_emitter.eval_constant_expr(default_expr);
                            let idx = func_emitter.add_constant(val);
                            func_emitter
                                .chunk
                                .code
                                .push(OpCode::RecvInit(i as u32, idx as u16));
                        } else {
                            func_emitter.push_op(OpCode::Recv(i as u32));
                        }
                    }
                }

                let (mut func_chunk, is_generator) = func_emitter.compile(body);
                func_chunk.returns_ref = *by_ref;

                // Convert return type
                let ret_type = return_type.and_then(|rt| self.convert_type(rt));

                let start_line = span.line_info(self.source).map(|li| li.line as u32);
                let end_line = close_brace_span
                    .and_then(|s| s.line_info(self.source).map(|li| li.line as u32));

                let user_func = UserFunc {
                    params: param_syms,
                    uses: Vec::new(),
                    chunk: Rc::new(func_chunk),
                    is_static: false,
                    is_generator,
                    statics: Rc::new(RefCell::new(HashMap::new())),
                    return_type: ret_type,
                    start_line,
                    end_line,
                };

                let func_res = Val::Resource(Rc::new(user_func));
                let const_idx = self.add_constant(func_res);

                self.chunk
                    .code
                    .push(OpCode::DefFunc(func_sym, const_idx as u32));

                if !attributes.is_empty() {
                    let attr_val = self.build_attribute_list(attributes);
                    let idx = self.add_constant(attr_val);
                    self.chunk
                        .code
                        .push(OpCode::SetFunctionAttributes(func_sym, idx as u16));
                }
            }
            Stmt::Class {
                name,
                members,
                extends,
                implements,
                attributes,
                modifiers,
                doc_comment,
                close_brace_span,
                ..
            } => {
                let class_sym = self.declare_class_sym_from_span(name.span);

                let parent_sym = if let Some(parent_name) = extends {
                    Some(self.resolve_class_sym_from_name(parent_name))
                } else {
                    None
                };

                self.chunk
                    .code
                    .push(OpCode::DefClass(class_sym, parent_sym));

                let start_line = name
                    .span
                    .line_info(self.source)
                    .map(|info| info.line as u32);
                let end_line = close_brace_span
                    .and_then(|span| span.line_info(self.source).map(|info| info.line as u32));
                self.chunk
                    .code
                    .push(OpCode::SetClassLines(class_sym, start_line, end_line));

                if let Some(doc_comment) = doc_comment {
                    let comment = self.source[doc_comment.start..doc_comment.end].to_vec();
                    let idx = self.add_constant(Val::String(Rc::new(comment)));
                    self.chunk
                        .code
                        .push(OpCode::SetClassDocComment(class_sym, idx as u16));
                }

                // Emit class metadata (attributes, modifiers, interfaces)
                self.emit_class_metadata(class_sym, attributes, modifiers, implements);

                // Check for #[AllowDynamicProperties] attribute
                for attr_group in *attributes {
                    for attr in attr_group.attributes {
                        let attr_name = self.get_text(attr.name.span);
                        // Check for both fully qualified and simple name
                        if attr_name == b"AllowDynamicProperties"
                            || attr_name.ends_with(b"\\AllowDynamicProperties")
                        {
                            self.chunk
                                .code
                                .push(OpCode::AllowDynamicProperties(class_sym));
                            break;
                        }
                    }
                }

                // Track class context while emitting members
                let prev_class = self.current_class;
                self.current_class = Some(class_sym);
                self.emit_members(class_sym, members);
                self.current_class = prev_class;

                // Finalize class: validate interfaces, abstract methods, etc.
                self.push_op(OpCode::FinalizeClass(class_sym));
            }
            Stmt::Interface {
                name,
                members,
                extends,
                doc_comment,
                close_brace_span,
                ..
            } => {
                let sym = self.declare_class_sym_from_span(name.span);

                self.push_op(OpCode::DefInterface(sym));

                let start_line = name
                    .span
                    .line_info(self.source)
                    .map(|info| info.line as u32);
                let end_line = close_brace_span
                    .and_then(|span| span.line_info(self.source).map(|info| info.line as u32));
                self.chunk
                    .code
                    .push(OpCode::SetClassLines(sym, start_line, end_line));

                if let Some(doc_comment) = doc_comment {
                    let comment = self.source[doc_comment.start..doc_comment.end].to_vec();
                    let idx = self.add_constant(Val::String(Rc::new(comment)));
                    self.chunk
                        .code
                        .push(OpCode::SetClassDocComment(sym, idx as u16));
                }

                for interface in *extends {
                    let interface_sym = self.resolve_class_sym_from_name(interface);
                    self.chunk
                        .code
                        .push(OpCode::AddInterface(sym, interface_sym));
                }

                let prev_class = self.current_class;
                self.current_class = Some(sym);
                self.emit_members(sym, members);
                self.current_class = prev_class;
            }
            Stmt::Trait {
                name,
                members,
                doc_comment,
                close_brace_span,
                ..
            } => {
                let sym = self.declare_class_sym_from_span(name.span);

                self.push_op(OpCode::DefTrait(sym));

                let start_line = name
                    .span
                    .line_info(self.source)
                    .map(|info| info.line as u32);
                let end_line = close_brace_span
                    .and_then(|span| span.line_info(self.source).map(|info| info.line as u32));
                self.chunk
                    .code
                    .push(OpCode::SetClassLines(sym, start_line, end_line));

                if let Some(doc_comment) = doc_comment {
                    let comment = self.source[doc_comment.start..doc_comment.end].to_vec();
                    let idx = self.add_constant(Val::String(Rc::new(comment)));
                    self.chunk
                        .code
                        .push(OpCode::SetClassDocComment(sym, idx as u16));
                }

                let prev_trait = self.current_trait;
                self.current_trait = Some(sym);
                self.emit_members(sym, members);
                self.current_trait = prev_trait;
            }

            Stmt::While {
                condition, body, ..
            } => {
                let start_label = self.chunk.code.len();

                self.emit_expr(condition);

                let end_jump = self.chunk.code.len();
                self.push_op(OpCode::JmpIfFalse(0)); // Patch later

                self.loop_stack.push(LoopInfo {
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                for stmt in *body {
                    self.emit_stmt(stmt);
                }

                self.push_op(OpCode::Jmp(start_label as u32));

                let end_label = self.chunk.code.len();
                self.chunk.code[end_jump] = OpCode::JmpIfFalse(end_label as u32);

                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_jumps {
                    self.patch_jump(idx, end_label);
                }
                for idx in loop_info.continue_jumps {
                    self.patch_jump(idx, start_label);
                }
            }
            Stmt::DoWhile {
                body, condition, ..
            } => {
                let start_label = self.chunk.code.len();

                self.loop_stack.push(LoopInfo {
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                for stmt in *body {
                    self.emit_stmt(stmt);
                }

                let continue_label = self.chunk.code.len();
                self.emit_expr(condition);
                self.push_op(OpCode::JmpIfTrue(start_label as u32));

                let end_label = self.chunk.code.len();

                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_jumps {
                    self.patch_jump(idx, end_label);
                }
                for idx in loop_info.continue_jumps {
                    self.patch_jump(idx, continue_label);
                }
            }
            Stmt::For {
                init,
                condition,
                loop_expr,
                body,
                ..
            } => {
                for expr in *init {
                    self.emit_expr(expr);
                    self.push_op(OpCode::Pop); // Discard result
                }

                let start_label = self.chunk.code.len();

                let mut end_jump = None;
                if !condition.is_empty() {
                    for (i, expr) in condition.iter().enumerate() {
                        self.emit_expr(expr);
                        if i < condition.len() - 1 {
                            self.push_op(OpCode::Pop);
                        }
                    }
                    end_jump = Some(self.chunk.code.len());
                    self.push_op(OpCode::JmpIfFalse(0)); // Patch later
                }

                self.loop_stack.push(LoopInfo {
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                for stmt in *body {
                    self.emit_stmt(stmt);
                }

                let continue_label = self.chunk.code.len();
                for expr in *loop_expr {
                    self.emit_expr(expr);
                    self.push_op(OpCode::Pop);
                }

                self.push_op(OpCode::Jmp(start_label as u32));

                let end_label = self.chunk.code.len();
                if let Some(idx) = end_jump {
                    self.chunk.code[idx] = OpCode::JmpIfFalse(end_label as u32);
                }

                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_jumps {
                    self.patch_jump(idx, end_label);
                }
                for idx in loop_info.continue_jumps {
                    self.patch_jump(idx, continue_label);
                }
            }
            Stmt::Foreach {
                expr,
                key_var,
                value_var,
                body,
                ..
            } => {
                // Check if by-ref
                let is_by_ref = matches!(
                    value_var,
                    Expr::Unary {
                        op: UnaryOp::Reference,
                        ..
                    }
                );

                if is_by_ref {
                    if let Expr::Variable { span, .. } = expr {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::MakeVarRef(sym));
                        } else {
                            self.emit_expr(expr);
                        }
                    } else {
                        self.emit_expr(expr);
                    }
                } else {
                    self.emit_expr(expr);
                }

                // IterInit(End)
                let init_idx = self.chunk.code.len();
                self.push_op(OpCode::IterInit(0)); // Patch later

                let start_label = self.chunk.code.len();

                // IterValid(End)
                let valid_idx = self.chunk.code.len();
                self.push_op(OpCode::IterValid(0)); // Patch later

                // IterGetVal
                if let Expr::Variable { span, .. } = value_var {
                    let name = self.get_text(*span);
                    if name.starts_with(b"$") {
                        let sym = self.interner.intern(&name[1..]);
                        self.push_op(OpCode::IterGetVal(sym));
                    }
                } else if let Expr::Unary {
                    op: UnaryOp::Reference,
                    expr,
                    ..
                } = value_var
                {
                    if let Expr::Variable { span, .. } = expr {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::IterGetValRef(sym));
                        }
                    }
                }

                // IterGetKey
                if let Some(k) = key_var {
                    if let Expr::Variable { span, .. } = k {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::IterGetKey(sym));
                        }
                    }
                }

                self.loop_stack.push(LoopInfo {
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                // Body
                for stmt in *body {
                    self.emit_stmt(stmt);
                }

                let continue_label = self.chunk.code.len();
                // IterNext
                self.push_op(OpCode::IterNext);

                // Jump back to start
                self.push_op(OpCode::Jmp(start_label as u32));

                let end_label = self.chunk.code.len();

                // Patch jumps
                self.patch_jump(init_idx, end_label);
                self.patch_jump(valid_idx, end_label);

                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_jumps {
                    self.patch_jump(idx, end_label);
                }
                for idx in loop_info.continue_jumps {
                    self.patch_jump(idx, continue_label);
                }
            }
            Stmt::Throw { expr, .. } => {
                self.emit_expr(expr);
                self.push_op(OpCode::Throw);
            }
            Stmt::Switch {
                condition, cases, ..
            } => {
                self.emit_expr(condition);

                let dispatch_jump = self.chunk.code.len();
                self.push_op(OpCode::Jmp(0)); // Patch later

                let mut case_labels = Vec::new();
                let mut default_label = None;

                self.loop_stack.push(LoopInfo {
                    break_jumps: Vec::new(),
                    continue_jumps: Vec::new(),
                });

                for case in *cases {
                    let label = self.chunk.code.len();
                    case_labels.push(label);

                    if case.condition.is_none() {
                        default_label = Some(label);
                    }

                    for stmt in case.body {
                        self.emit_stmt(stmt);
                    }
                }

                let jump_over_dispatch = self.chunk.code.len();
                self.push_op(OpCode::Jmp(0)); // Patch to end_label

                let dispatch_start = self.chunk.code.len();
                self.patch_jump(dispatch_jump, dispatch_start);

                // Dispatch logic
                for (i, case) in cases.iter().enumerate() {
                    if let Some(cond) = case.condition {
                        self.push_op(OpCode::Dup); // Dup switch cond
                        self.emit_expr(cond);
                        self.push_op(OpCode::IsEqual); // Loose comparison
                        self.chunk
                            .code
                            .push(OpCode::JmpIfTrue(case_labels[i] as u32));
                    }
                }

                // Pop switch cond
                self.push_op(OpCode::Pop);

                if let Some(def_lbl) = default_label {
                    self.push_op(OpCode::Jmp(def_lbl as u32));
                } else {
                    // No default, jump to end
                    self.push_op(OpCode::Jmp(jump_over_dispatch as u32));
                    // Will be patched to end_label
                }

                let end_label = self.chunk.code.len();
                self.patch_jump(jump_over_dispatch, end_label);

                let loop_info = self.loop_stack.pop().unwrap();
                for idx in loop_info.break_jumps {
                    self.patch_jump(idx, end_label);
                }
                // Continue in switch acts like break
                for idx in loop_info.continue_jumps {
                    self.patch_jump(idx, end_label);
                }
            }
            Stmt::Try {
                body,
                catches,
                finally,
                ..
            } => {
                let try_start = self.chunk.code.len() as u32;

                // If there's a finally block, we need to track it BEFORE emitting the try body
                // so that break/continue inside the try body know they're inside a final context
                let has_finally = finally.is_some();
                let try_finally_placeholder_idx = if has_finally {
                    // Reserve space in try_finally_stack with placeholder values
                    // We'll update them after we emit the finally block
                    self.try_finally_stack.push(TryFinallyInfo {
                        catch_table_idx: 0, // Will be updated
                        finally_start: 0,   // Will be updated
                        finally_end: 0,     // Will be updated
                    });
                    Some(self.try_finally_stack.len() - 1)
                } else {
                    None
                };

                for stmt in *body {
                    self.emit_stmt(stmt);
                }
                let try_end = self.chunk.code.len() as u32;

                // Jump from successful try to finally (or end if no finally)
                let jump_from_try = self.chunk.code.len();
                self.push_op(OpCode::Jmp(0)); // Will patch to finally or end

                let mut catch_jumps = Vec::new();
                let mut catch_ranges = Vec::new(); // Track catch block ranges for finally encoding

                for catch in *catches {
                    let catch_start = self.chunk.code.len() as u32;

                    for ty in catch.types {
                        let type_name = self.get_text(ty.span);
                        let type_sym = self.interner.intern(type_name);

                        self.chunk.catch_table.push(CatchEntry {
                            start: try_start,
                            end: try_end,
                            target: catch_start,
                            catch_type: Some(type_sym),
                            finally_target: None, // Will be set below if finally exists
                            finally_end: None,
                        });
                    }

                    if let Some(var) = catch.var {
                        let name = self.get_text(var.span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::StoreVar(sym));
                        }
                    } else {
                        self.push_op(OpCode::Pop);
                    }

                    for stmt in catch.body {
                        self.emit_stmt(stmt);
                    }

                    let catch_end = self.chunk.code.len() as u32;
                    catch_ranges.push((catch_start, catch_end));

                    // Jump from catch to finally (or end if no finally)
                    catch_jumps.push(self.chunk.code.len());
                    self.push_op(OpCode::Jmp(0)); // Will patch to finally or end
                }

                // Emit finally block if present
                if let Some(finally_body) = finally {
                    let finally_start = self.chunk.code.len() as u32;
                    let catch_table_idx = self.chunk.catch_table.len();

                    // Update the placeholder in try_finally_stack
                    if let Some(idx) = try_finally_placeholder_idx {
                        self.try_finally_stack[idx].catch_table_idx = catch_table_idx;
                        self.try_finally_stack[idx].finally_start = finally_start;
                        // finally_end will be set after emitting
                    }

                    // Patch jump from try to finally
                    self.patch_jump(jump_from_try, finally_start as usize);

                    // Patch all catch block jumps to finally
                    for idx in &catch_jumps {
                        self.patch_jump(*idx, finally_start as usize);
                    }

                    // Emit the finally block statements
                    for stmt in *finally_body {
                        self.emit_stmt(stmt);
                    }
                    let finally_end = self.chunk.code.len() as u32;

                    // Update the finally_end in try_finally_stack
                    if let Some(idx) = try_finally_placeholder_idx {
                        self.try_finally_stack[idx].finally_end = finally_end;
                    }

                    // Update all existing catch entries to include finally_target and finally_end
                    // This enables unwinding through finally when exception is caught
                    for entry in self.chunk.catch_table.iter_mut() {
                        if entry.start == try_start && entry.end == try_end {
                            entry.finally_target = Some(finally_start);
                            entry.finally_end = Some(finally_end);
                        }
                    }

                    // Add a finally-only entry for the try block
                    // This ensures finally executes even on uncaught exceptions
                    self.chunk.catch_table.push(CatchEntry {
                        start: try_start,
                        end: try_end,
                        target: finally_start,
                        catch_type: None, // No specific catch type - this is for finally
                        finally_target: None,
                        finally_end: Some(finally_end),
                    });

                    // Also add entries for catch blocks to ensure finally runs during their unwinding
                    for (catch_start, catch_end) in catch_ranges {
                        self.chunk.catch_table.push(CatchEntry {
                            start: catch_start,
                            end: catch_end,
                            target: finally_start,
                            catch_type: None, // No specific catch type - this is for finally
                            finally_target: None,
                            finally_end: Some(finally_end),
                        });
                    }

                    // Pop from try_finally_stack after emitting
                    self.try_finally_stack.pop();

                    // Finally falls through to end
                } else {
                    // No finally - patch jumps directly to end
                    let after_catches = self.chunk.code.len();
                    self.patch_jump(jump_from_try, after_catches);
                    for idx in &catch_jumps {
                        self.patch_jump(*idx, after_catches);
                    }
                }
            }
            _ => {}
        }
    }

    fn patch_jump(&mut self, idx: usize, target: usize) {
        let op = self.chunk.code[idx];
        let new_op = match op {
            OpCode::Jmp(_) => OpCode::Jmp(target as u32),
            OpCode::JmpIfFalse(_) => OpCode::JmpIfFalse(target as u32),
            OpCode::JmpIfTrue(_) => OpCode::JmpIfTrue(target as u32),
            OpCode::JmpZEx(_) => OpCode::JmpZEx(target as u32),
            OpCode::JmpNzEx(_) => OpCode::JmpNzEx(target as u32),
            OpCode::Coalesce(_) => OpCode::Coalesce(target as u32),
            OpCode::IterInit(_) => OpCode::IterInit(target as u32),
            OpCode::IterValid(_) => OpCode::IterValid(target as u32),
            OpCode::JmpFinally(_) => OpCode::JmpFinally(target as u32),
            _ => panic!("Cannot patch non-jump opcode: {:?}", op),
        };
        self.chunk.code[idx] = new_op;
    }

    fn get_literal_value(&self, expr: &Expr) -> Option<Val> {
        match expr {
            Expr::Integer { value, .. } => {
                let s = std::str::from_utf8(value).ok()?;
                let i: i64 = s.parse().ok()?;
                Some(Val::Int(i))
            }
            Expr::String { value, .. } => {
                let s = if value.len() >= 2 {
                    let first = value[0];
                    let last = value[value.len() - 1];
                    if first == b'"' && last == b'"' {
                        let inner = &value[1..value.len() - 1];
                        unescape_string(inner)
                    } else if first == b'\'' && last == b'\'' {
                        let inner = &value[1..value.len() - 1];
                        let mut result = Vec::new();
                        let mut i = 0;
                        while i < inner.len() {
                            if inner[i] == b'\\' && i + 1 < inner.len() {
                                if inner[i + 1] == b'\'' || inner[i + 1] == b'\\' {
                                    result.push(inner[i + 1]);
                                    i += 2;
                                } else {
                                    result.push(inner[i]);
                                    i += 1;
                                }
                            } else {
                                result.push(inner[i]);
                                i += 1;
                            }
                        }
                        result
                    } else {
                        // No quotes - this is from string interpolation (EncapsedAndWhitespace)
                        // These strings need unescaping too
                        unescape_string(value)
                    }
                } else if !value.is_empty() {
                    // Short string without quotes - also from interpolation
                    unescape_string(value)
                } else {
                    value.to_vec()
                };
                Some(Val::String(s.into()))
            }
            Expr::Boolean { value, .. } => Some(Val::Bool(*value)),
            Expr::Null { .. } => Some(Val::Null),
            Expr::Array { items, .. } => {
                use crate::core::value::ConstArrayKey;
                use indexmap::IndexMap;

                let mut const_array = IndexMap::new();
                let mut next_index = 0i64;

                for item in *items {
                    if item.unpack {
                        continue;
                    }

                    let val = self.get_literal_value(item.value).unwrap_or(Val::Null);
                    if let Some(key_expr) = item.key {
                        let key_val = self.get_literal_value(key_expr);
                        let key = match key_val {
                            Some(Val::Int(i)) => {
                                if i >= next_index {
                                    next_index = i + 1;
                                }
                                ConstArrayKey::Int(i)
                            }
                            Some(Val::String(s)) => ConstArrayKey::Str(s),
                            Some(Val::Float(f)) => {
                                let i = f as i64;
                                if i >= next_index {
                                    next_index = i + 1;
                                }
                                ConstArrayKey::Int(i)
                            }
                            Some(Val::Bool(b)) => {
                                let i = if b { 1 } else { 0 };
                                if i >= next_index {
                                    next_index = i + 1;
                                }
                                ConstArrayKey::Int(i)
                            }
                            _ => {
                                let key = ConstArrayKey::Int(next_index);
                                next_index += 1;
                                key
                            }
                        };
                        const_array.insert(key, val);
                    } else {
                        const_array.insert(ConstArrayKey::Int(next_index), val);
                        next_index += 1;
                    }
                }

                Some(Val::ConstArray(Rc::new(const_array)))
            }
            Expr::ClassConstFetch { class, constant, .. } => {
                if let Expr::Variable {
                    span: const_span, ..
                } = constant
                {
                    let const_name = self.get_text(*const_span);
                    if const_name.eq_ignore_ascii_case(b"class") {
                        if let Expr::Variable { span, .. } = class {
                            let class_name = self.get_text(*span);
                            if !class_name.starts_with(b"$") {
                                if (class_name.eq_ignore_ascii_case(b"self")
                                    || class_name.eq_ignore_ascii_case(b"static"))
                                    && self.current_class.is_some()
                                {
                                    let class_sym = self.current_class.unwrap();
                                    let name_bytes = self.interner.lookup(class_sym)?;
                                    return Some(Val::String(name_bytes.to_vec().into()));
                                }

                                let resolved = self.resolve_class_name(class_name);
                                return Some(Val::String(resolved.into()));
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn emit_expr(&mut self, expr: &Expr) {
        self.set_current_line(expr.span());
        match expr {
            Expr::Integer { value, .. } => {
                let s = std::str::from_utf8(value).unwrap_or("0");
                let i: i64 = s.parse().unwrap_or(0);
                let idx = self.add_constant(Val::Int(i));
                self.push_op(OpCode::Const(idx as u16));
            }
            Expr::Float { value, .. } => {
                let s = std::str::from_utf8(value).unwrap_or("0.0");
                let f: f64 = s.parse().unwrap_or(0.0);
                let idx = self.add_constant(Val::Float(f));
                self.push_op(OpCode::Const(idx as u16));
            }
            Expr::String { value, .. } => {
                let s = if value.len() >= 2 {
                    let first = value[0];
                    let last = value[value.len() - 1];
                    if first == b'"' && last == b'"' {
                        // Double-quoted string: unescape escape sequences
                        let inner = &value[1..value.len() - 1];
                        unescape_string(inner)
                    } else if first == b'\'' && last == b'\'' {
                        // Single-quoted string: no escape processing (except \' and \\)
                        let inner = &value[1..value.len() - 1];
                        let mut result = Vec::new();
                        let mut i = 0;
                        while i < inner.len() {
                            if inner[i] == b'\\' && i + 1 < inner.len() {
                                if inner[i + 1] == b'\'' || inner[i + 1] == b'\\' {
                                    result.push(inner[i + 1]);
                                    i += 2;
                                } else {
                                    result.push(inner[i]);
                                    i += 1;
                                }
                            } else {
                                result.push(inner[i]);
                                i += 1;
                            }
                        }
                        result
                    } else {
                        // No quotes - this is from string interpolation (EncapsedAndWhitespace)
                        // These strings need unescaping too
                        unescape_string(value)
                    }
                } else if !value.is_empty() {
                    // Short string without quotes - also from interpolation
                    unescape_string(value)
                } else {
                    value.to_vec()
                };
                let idx = self.add_constant(Val::String(s.into()));
                self.push_op(OpCode::Const(idx as u16));
            }
            Expr::InterpolatedString { parts, .. } => {
                if parts.is_empty() {
                    let idx = self.add_constant(Val::String(Vec::<u8>::new().into()));
                    self.push_op(OpCode::Const(idx as u16));
                } else {
                    for (i, part) in parts.iter().enumerate() {
                        self.emit_expr(*part);
                        if i > 0 {
                            self.push_op(OpCode::Concat);
                        }
                    }
                }
            }
            Expr::Boolean { value, .. } => {
                let idx = self.add_constant(Val::Bool(*value));
                self.push_op(OpCode::Const(idx as u16));
            }
            Expr::Null { .. } => {
                let idx = self.add_constant(Val::Null);
                self.push_op(OpCode::Const(idx as u16));
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                match op {
                    BinaryOp::And | BinaryOp::LogicalAnd => {
                        self.emit_expr(left);
                        let end_jump = self.chunk.code.len();
                        self.push_op(OpCode::JmpZEx(0));
                        self.emit_expr(right);
                        let end_label = self.chunk.code.len();
                        self.chunk.code[end_jump] = OpCode::JmpZEx(end_label as u32);
                        self.push_op(OpCode::Cast(1)); // Bool
                    }
                    BinaryOp::Or | BinaryOp::LogicalOr => {
                        self.emit_expr(left);
                        let end_jump = self.chunk.code.len();
                        self.push_op(OpCode::JmpNzEx(0));
                        self.emit_expr(right);
                        let end_label = self.chunk.code.len();
                        self.chunk.code[end_jump] = OpCode::JmpNzEx(end_label as u32);
                        self.push_op(OpCode::Cast(1)); // Bool
                    }
                    BinaryOp::Coalesce => {
                        self.emit_expr(left);
                        let end_jump = self.chunk.code.len();
                        self.push_op(OpCode::Coalesce(0));
                        self.emit_expr(right);
                        let end_label = self.chunk.code.len();
                        self.chunk.code[end_jump] = OpCode::Coalesce(end_label as u32);
                    }
                    BinaryOp::Instanceof => {
                        // For instanceof, the class name should be treated as a literal string,
                        // not a constant lookup. PHP allows bare identifiers like "instanceof Foo".
                        self.emit_expr(left);

                        // Special handling for bare class names
                        match right {
                            Expr::Variable { span, .. } => {
                                // Bare identifier - treat as class name string
                                let name = self.get_text(*span);
                                let class_name_str = if name.starts_with(b"$") {
                                    // It's actually a variable, evaluate it normally
                                    self.emit_expr(right);
                                    return;
                                } else {
                                    // Bare class name - push as string constant
                                    let resolved = self.resolve_class_name(name);
                                    Val::String(resolved.into())
                                };
                                let const_idx = self.add_constant(class_name_str) as u16;
                                self.push_op(OpCode::Const(const_idx));
                            }
                            _ => {
                                // Complex expression - evaluate normally
                                self.emit_expr(right);
                            }
                        }

                        self.push_op(OpCode::InstanceOf);
                    }
                    _ => {
                        self.emit_expr(left);
                        self.emit_expr(right);
                        match op {
                            BinaryOp::Plus => self.push_op(OpCode::Add),
                            BinaryOp::Minus => self.push_op(OpCode::Sub),
                            BinaryOp::Mul => self.push_op(OpCode::Mul),
                            BinaryOp::Div => self.push_op(OpCode::Div),
                            BinaryOp::Mod => self.push_op(OpCode::Mod),
                            BinaryOp::Concat => self.push_op(OpCode::Concat),
                            BinaryOp::Pow => self.push_op(OpCode::Pow),
                            BinaryOp::BitAnd => self.push_op(OpCode::BitwiseAnd),
                            BinaryOp::BitOr => self.push_op(OpCode::BitwiseOr),
                            BinaryOp::BitXor => self.push_op(OpCode::BitwiseXor),
                            BinaryOp::ShiftLeft => self.push_op(OpCode::ShiftLeft),
                            BinaryOp::ShiftRight => self.push_op(OpCode::ShiftRight),
                            BinaryOp::EqEq => self.push_op(OpCode::IsEqual),
                            BinaryOp::EqEqEq => self.push_op(OpCode::IsIdentical),
                            BinaryOp::NotEq => self.push_op(OpCode::IsNotEqual),
                            BinaryOp::NotEqEq => self.push_op(OpCode::IsNotIdentical),
                            BinaryOp::Gt => self.push_op(OpCode::IsGreater),
                            BinaryOp::Lt => self.push_op(OpCode::IsLess),
                            BinaryOp::GtEq => self.push_op(OpCode::IsGreaterOrEqual),
                            BinaryOp::LtEq => self.push_op(OpCode::IsLessOrEqual),
                            BinaryOp::Spaceship => self.push_op(OpCode::Spaceship),
                            BinaryOp::LogicalXor => self.push_op(OpCode::BoolXor),
                            // Instanceof is handled above
                            BinaryOp::Instanceof => {}
                            _ => {}
                        }
                    }
                }
            }
            Expr::Match {
                condition, arms, ..
            } => {
                self.emit_expr(condition);

                let mut end_jumps = Vec::new();

                for arm in *arms {
                    if let Some(conds) = arm.conditions {
                        let mut body_jump_indices = Vec::new();

                        for cond in conds {
                            self.push_op(OpCode::Dup);
                            self.emit_expr(cond);
                            self.push_op(OpCode::IsIdentical); // Strict

                            let jump_idx = self.chunk.code.len();
                            self.push_op(OpCode::JmpIfTrue(0)); // Jump to body
                            body_jump_indices.push(jump_idx);
                        }

                        // If we are here, none matched. Jump to next arm.
                        let skip_body_idx = self.chunk.code.len();
                        self.push_op(OpCode::Jmp(0));

                        // Body start
                        let body_start = self.chunk.code.len();
                        for idx in body_jump_indices {
                            self.patch_jump(idx, body_start);
                        }

                        // Pop condition before body
                        self.push_op(OpCode::Pop);
                        self.emit_expr(arm.body);

                        // Jump to end
                        end_jumps.push(self.chunk.code.len());
                        self.push_op(OpCode::Jmp(0));

                        // Patch skip_body_idx to here (next arm)
                        self.patch_jump(skip_body_idx, self.chunk.code.len());
                    } else {
                        // Default arm
                        self.push_op(OpCode::Pop); // Pop condition
                        self.emit_expr(arm.body);
                        end_jumps.push(self.chunk.code.len());
                        self.push_op(OpCode::Jmp(0));
                    }
                }

                // No match found
                self.push_op(OpCode::MatchError);

                let end_label = self.chunk.code.len();
                for idx in end_jumps {
                    self.patch_jump(idx, end_label);
                }
            }
            Expr::Print { expr, .. } => {
                self.emit_expr(expr);
                self.push_op(OpCode::Echo);
                let idx = self.add_constant(Val::Int(1));
                self.push_op(OpCode::Const(idx as u16));
            }
            Expr::Include { expr, kind, .. } => {
                self.emit_expr(expr);
                let include_type = match kind {
                    IncludeKind::Include => 2,
                    IncludeKind::IncludeOnce => 3,
                    IncludeKind::Require => 4,
                    IncludeKind::RequireOnce => 5,
                };
                let idx = self.add_constant(Val::Int(include_type));
                self.push_op(OpCode::Const(idx as u16));
                self.push_op(OpCode::IncludeOrEval);
            }
            Expr::Unary { op, expr, .. } => {
                match op {
                    UnaryOp::Reference => {
                        // Handle &$var
                        if let Expr::Variable { span, .. } = expr {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                let var_name = &name[1..];
                                let sym = self.interner.intern(var_name);
                                self.push_op(OpCode::MakeVarRef(sym));
                            }
                        } else {
                            // Reference to something else?
                            self.emit_expr(expr);
                            self.push_op(OpCode::MakeRef);
                        }
                    }
                    UnaryOp::Minus => {
                        // 0 - expr
                        let idx = self.add_constant(Val::Int(0));
                        self.push_op(OpCode::Const(idx as u16));
                        self.emit_expr(expr);
                        self.push_op(OpCode::Sub);
                    }
                    UnaryOp::Not => {
                        self.emit_expr(expr);
                        self.push_op(OpCode::BoolNot);
                    }
                    UnaryOp::BitNot => {
                        self.emit_expr(expr);
                        self.push_op(OpCode::BitwiseNot);
                    }
                    UnaryOp::PreInc => {
                        match expr {
                            Expr::Variable { span, .. } => {
                                let name = self.get_text(*span);
                                if name.starts_with(b"$") {
                                    let sym = self.interner.intern(&name[1..]);
                                    self.push_op(OpCode::MakeVarRef(sym));
                                    self.push_op(OpCode::PreInc);
                                }
                            }
                            Expr::PropertyFetch {
                                target, property, ..
                            } => {
                                // ++$obj->prop
                                self.emit_expr(target);
                                // Property name (could be identifier or expression)
                                let prop_name = self.get_text(property.span());
                                let const_idx =
                                    self.add_constant(Val::String(Rc::new(prop_name.to_vec())));
                                self.push_op(OpCode::Const(const_idx as u16));
                                self.push_op(OpCode::PreIncObj);
                            }
                            Expr::ClassConstFetch {
                                class, constant, ..
                            } => {
                                // ++Class::$property
                                if self.emit_static_property_access(class, constant) {
                                    self.push_op(OpCode::PreIncStaticProp);
                                } else {
                                    self.emit_expr(expr);
                                }
                            }
                            _ => {
                                self.emit_expr(expr);
                            }
                        }
                    }
                    UnaryOp::PreDec => {
                        match expr {
                            Expr::Variable { span, .. } => {
                                let name = self.get_text(*span);
                                if name.starts_with(b"$") {
                                    let sym = self.interner.intern(&name[1..]);
                                    self.push_op(OpCode::MakeVarRef(sym));
                                    self.push_op(OpCode::PreDec);
                                }
                            }
                            Expr::PropertyFetch {
                                target, property, ..
                            } => {
                                // --$obj->prop
                                self.emit_expr(target);
                                let prop_name = self.get_text(property.span());
                                let const_idx =
                                    self.add_constant(Val::String(Rc::new(prop_name.to_vec())));
                                self.push_op(OpCode::Const(const_idx as u16));
                                self.push_op(OpCode::PreDecObj);
                            }
                            Expr::ClassConstFetch {
                                class, constant, ..
                            } => {
                                // --Class::$property
                                if self.emit_static_property_access(class, constant) {
                                    self.push_op(OpCode::PreDecStaticProp);
                                } else {
                                    self.emit_expr(expr);
                                }
                            }
                            _ => {
                                self.emit_expr(expr);
                            }
                        }
                    }
                    UnaryOp::ErrorSuppress => {
                        // @ operator: suppress errors for the expression
                        self.push_op(OpCode::BeginSilence);
                        self.emit_expr(expr);
                        self.push_op(OpCode::EndSilence);
                    }
                    _ => {
                        self.emit_expr(expr);
                    }
                }
            }
            Expr::PostInc { var, .. } => {
                match var {
                    Expr::Variable { span, .. } => {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::MakeVarRef(sym));
                            self.push_op(OpCode::PostInc);
                        }
                    }
                    Expr::PropertyFetch {
                        target, property, ..
                    } => {
                        // $obj->prop++
                        self.emit_expr(target);
                        let prop_name = self.get_text(property.span());
                        let const_idx = self.add_constant(Val::String(Rc::new(prop_name.to_vec())));
                        self.push_op(OpCode::Const(const_idx as u16));
                        self.push_op(OpCode::PostIncObj);
                    }
                    Expr::ClassConstFetch {
                        class, constant, ..
                    } => {
                        // Class::$property++
                        if self.emit_static_property_access(class, constant) {
                            self.push_op(OpCode::PostIncStaticProp);
                        } else {
                            self.emit_expr(var);
                        }
                    }
                    _ => {
                        // Unsupported post-increment target
                        self.emit_expr(var);
                    }
                }
            }
            Expr::PostDec { var, .. } => {
                match var {
                    Expr::Variable { span, .. } => {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::MakeVarRef(sym));
                            self.push_op(OpCode::PostDec);
                        }
                    }
                    Expr::PropertyFetch {
                        target, property, ..
                    } => {
                        // $obj->prop--
                        self.emit_expr(target);
                        let prop_name = self.get_text(property.span());
                        let const_idx = self.add_constant(Val::String(Rc::new(prop_name.to_vec())));
                        self.push_op(OpCode::Const(const_idx as u16));
                        self.push_op(OpCode::PostDecObj);
                    }
                    Expr::ClassConstFetch {
                        class, constant, ..
                    } => {
                        // Class::$property--
                        if self.emit_static_property_access(class, constant) {
                            self.push_op(OpCode::PostDecStaticProp);
                        } else {
                            self.emit_expr(var);
                        }
                    }
                    _ => {
                        // Unsupported post-decrement target
                        self.emit_expr(var);
                    }
                }
            }
            Expr::Ternary {
                condition,
                if_true,
                if_false,
                ..
            } => {
                self.emit_expr(condition);
                if let Some(true_expr) = if_true {
                    // cond ? true : false
                    let else_jump = self.chunk.code.len();
                    self.push_op(OpCode::JmpIfFalse(0)); // Placeholder

                    self.emit_expr(true_expr);
                    let end_jump = self.chunk.code.len();
                    self.push_op(OpCode::Jmp(0)); // Placeholder

                    let else_label = self.chunk.code.len();
                    self.chunk.code[else_jump] = OpCode::JmpIfFalse(else_label as u32);

                    self.emit_expr(if_false);
                    let end_label = self.chunk.code.len();
                    self.chunk.code[end_jump] = OpCode::Jmp(end_label as u32);
                } else {
                    // cond ?: false (Elvis)
                    let end_jump = self.chunk.code.len();
                    self.push_op(OpCode::JmpNzEx(0)); // Placeholder
                    self.emit_expr(if_false);

                    let end_label = self.chunk.code.len();
                    self.chunk.code[end_jump] = OpCode::JmpNzEx(end_label as u32);
                }
            }
            Expr::Cast { kind, expr, .. } => {
                self.emit_expr(expr);
                // Map CastKind to OpCode::Cast(u8)
                // 0=Int, 1=Bool, 2=Float, 3=String, 4=Array, 5=Object, 6=Unset
                let cast_op = match kind {
                    CastKind::Int => 0,
                    CastKind::Bool => 1,
                    CastKind::Float => 2,
                    CastKind::String => 3,
                    CastKind::Array => 4,
                    CastKind::Object => 5,
                    CastKind::Unset => 6,
                    _ => 0, // TODO
                };
                self.push_op(OpCode::Cast(cast_op));
            }
            Expr::Clone { expr, .. } => {
                self.emit_expr(expr);
                self.push_op(OpCode::Clone);
            }
            Expr::Exit { expr, .. } | Expr::Die { expr, .. } => {
                if let Some(e) = expr {
                    self.emit_expr(e);
                } else {
                    let idx = self.add_constant(Val::Null);
                    self.push_op(OpCode::Const(idx as u16));
                }
                self.push_op(OpCode::Exit);
            }
            Expr::Isset { vars, .. } => {
                if vars.is_empty() {
                    let idx = self.add_constant(Val::Bool(false));
                    self.push_op(OpCode::Const(idx as u16));
                } else {
                    let mut end_jumps = Vec::new();

                    for (i, var) in vars.iter().enumerate() {
                        match var {
                            Expr::Variable { span, .. } => {
                                let name = self.get_text(*span);
                                if name.starts_with(b"$") {
                                    let sym = self.interner.intern(&name[1..]);
                                    self.push_op(OpCode::IssetVar(sym));
                                } else {
                                    let idx = self.add_constant(Val::Bool(false));
                                    self.push_op(OpCode::Const(idx as u16));
                                }
                            }
                            Expr::IndirectVariable { name, .. } => {
                                self.emit_expr(name);
                                self.push_op(OpCode::IssetVarDynamic);
                            }
                            Expr::ArrayDimFetch { array, dim, .. } => {
                                self.emit_expr(array);
                                if let Some(d) = dim {
                                    self.emit_expr(d);
                                    self.push_op(OpCode::IssetDim);
                                } else {
                                    let idx = self.add_constant(Val::Bool(false));
                                    self.push_op(OpCode::Const(idx as u16));
                                }
                            }
                            Expr::PropertyFetch {
                                target, property, ..
                            } => {
                                self.emit_expr(target);
                                if let Expr::Variable { span, .. } = property {
                                    let name = self.get_text(*span);
                                    let sym = self.interner.intern(name);
                                    self.push_op(OpCode::IssetProp(sym));
                                } else {
                                    self.push_op(OpCode::Pop);
                                    let idx = self.add_constant(Val::Bool(false));
                                    self.push_op(OpCode::Const(idx as u16));
                                }
                            }
                            Expr::ClassConstFetch {
                                class, constant, ..
                            } => {
                                let is_static_prop = if let Expr::Variable { span, .. } = constant {
                                    let name = self.get_text(*span);
                                    name.starts_with(b"$")
                                } else {
                                    false
                                };

                                if is_static_prop {
                                    if let Expr::Variable { span, .. } = class {
                                        let name = self.get_text(*span);
                                        if !name.starts_with(b"$") {
                                            let idx = self
                                                .add_constant(Val::String(name.to_vec().into()));
                                            self.push_op(OpCode::Const(idx as u16));
                                        } else {
                                            let sym = self.interner.intern(&name[1..]);
                                            self.push_op(OpCode::LoadVar(sym));
                                        }

                                        if let Expr::Variable {
                                            span: prop_span, ..
                                        } = constant
                                        {
                                            let prop_name = self.get_text(*prop_span);
                                            let prop_sym = self.interner.intern(&prop_name[1..]);
                                            self.push_op(OpCode::IssetStaticProp(prop_sym));
                                        }
                                    } else {
                                        let idx = self.add_constant(Val::Bool(false));
                                        self.push_op(OpCode::Const(idx as u16));
                                    }
                                } else {
                                    let idx = self.add_constant(Val::Bool(false));
                                    self.push_op(OpCode::Const(idx as u16));
                                }
                            }
                            _ => {
                                let idx = self.add_constant(Val::Bool(false));
                                self.push_op(OpCode::Const(idx as u16));
                            }
                        }

                        if i < vars.len() - 1 {
                            self.push_op(OpCode::Dup);
                            let jump_idx = self.chunk.code.len();
                            self.push_op(OpCode::JmpIfFalse(0));
                            self.push_op(OpCode::Pop);
                            end_jumps.push(jump_idx);
                        }
                    }

                    let end_label = self.chunk.code.len();
                    for idx in end_jumps {
                        self.patch_jump(idx, end_label);
                    }
                }
            }
            Expr::Empty { expr, .. } => {
                match expr {
                    Expr::Variable { span, .. } => {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let sym = self.interner.intern(&name[1..]);
                            self.push_op(OpCode::IssetVar(sym));
                        } else {
                            let idx = self.add_constant(Val::Bool(false));
                            self.push_op(OpCode::Const(idx as u16));
                        }
                    }
                    Expr::ArrayDimFetch { array, dim, .. } => {
                        self.emit_expr(array);
                        if let Some(d) = dim {
                            self.emit_expr(d);
                            self.push_op(OpCode::IssetDim);
                        } else {
                            let idx = self.add_constant(Val::Bool(false));
                            self.push_op(OpCode::Const(idx as u16));
                        }
                    }
                    Expr::PropertyFetch {
                        target, property, ..
                    } => {
                        self.emit_expr(target);
                        if let Expr::Variable { span, .. } = property {
                            let name = self.get_text(*span);
                            let sym = self.interner.intern(name);
                            self.push_op(OpCode::IssetProp(sym));
                        } else {
                            self.push_op(OpCode::Pop);
                            let idx = self.add_constant(Val::Bool(false));
                            self.push_op(OpCode::Const(idx as u16));
                        }
                    }
                    Expr::ClassConstFetch {
                        class, constant, ..
                    } => {
                        let is_static_prop = if let Expr::Variable { span, .. } = constant {
                            let name = self.get_text(*span);
                            name.starts_with(b"$")
                        } else {
                            false
                        };

                        if is_static_prop {
                            if let Expr::Variable { span, .. } = class {
                                let name = self.get_text(*span);
                                if !name.starts_with(b"$") {
                                    let idx = self.add_constant(Val::String(name.to_vec().into()));
                                    self.push_op(OpCode::Const(idx as u16));
                                } else {
                                    let sym = self.interner.intern(&name[1..]);
                                    self.push_op(OpCode::LoadVar(sym));
                                }

                                if let Expr::Variable {
                                    span: prop_span, ..
                                } = constant
                                {
                                    let prop_name = self.get_text(*prop_span);
                                    let prop_sym = self.interner.intern(&prop_name[1..]);
                                    self.push_op(OpCode::IssetStaticProp(prop_sym));
                                }
                            } else {
                                let idx = self.add_constant(Val::Bool(false));
                                self.push_op(OpCode::Const(idx as u16));
                            }
                        } else {
                            let idx = self.add_constant(Val::Bool(false));
                            self.push_op(OpCode::Const(idx as u16));
                        }
                    }
                    _ => {
                        self.emit_expr(expr);
                        self.push_op(OpCode::BoolNot);
                        return;
                    }
                }

                let jump_if_not_set = self.chunk.code.len();
                self.push_op(OpCode::JmpIfFalse(0));

                self.emit_expr(expr);
                self.push_op(OpCode::BoolNot);

                let jump_end = self.chunk.code.len();
                self.push_op(OpCode::Jmp(0));

                let label_true = self.chunk.code.len();
                self.patch_jump(jump_if_not_set, label_true);

                let idx = self.add_constant(Val::Bool(true));
                self.push_op(OpCode::Const(idx as u16));

                let label_end = self.chunk.code.len();
                self.patch_jump(jump_end, label_end);
            }
            Expr::Eval { expr, .. } => {
                self.emit_expr(expr);
                // Emit ZEND_EVAL (type=1) for eval()
                let idx = self.add_constant(Val::Int(1));
                self.push_op(OpCode::Const(idx as u16));
                self.push_op(OpCode::IncludeOrEval);
            }
            Expr::Yield {
                key, value, from, ..
            } => {
                self.is_generator = true;
                if *from {
                    if let Some(v) = value {
                        self.emit_expr(v);
                    } else {
                        let idx = self.add_constant(Val::Null);
                        self.push_op(OpCode::Const(idx as u16));
                    }
                    self.push_op(OpCode::YieldFrom);
                } else {
                    let has_key = key.is_some();
                    if let Some(k) = key {
                        self.emit_expr(k);
                    }

                    if let Some(v) = value {
                        self.emit_expr(v);
                    } else {
                        let idx = self.add_constant(Val::Null);
                        self.push_op(OpCode::Const(idx as u16));
                    }
                    self.push_op(OpCode::Yield(has_key));
                    self.push_op(OpCode::GetSentValue);
                }
            }
            Expr::Closure {
                attributes: _,
                params,
                uses,
                body,
                by_ref,
                is_static,
                return_type,
                span,
                close_brace_span,
                ..
            } => {
                // 1. Collect param info
                struct ParamInfo<'a> {
                    name_span: crate::parser::span::Span,
                    by_ref: bool,
                    ty: Option<&'a Type<'a>>,
                    default: Option<&'a Expr<'a>>,
                    variadic: bool,
                }

                let mut param_infos = Vec::new();
                for param in *params {
                    param_infos.push(ParamInfo {
                        name_span: param.name.span,
                        by_ref: param.by_ref,
                        ty: param.ty,
                        default: param.default.as_ref().map(|e| *e),
                        variadic: param.variadic,
                    });
                }

                // 2. Create emitter with inherited context (closures inherit context)
                let closure_sym = self.interner.intern(b"{closure}");
                let mut func_emitter = Emitter::new(self.source, self.interner);
                func_emitter.file_path = self.file_path.clone();
                func_emitter.current_class = self.current_class;
                func_emitter.current_function = Some(closure_sym);
                func_emitter.current_namespace = self.current_namespace;
                func_emitter.use_aliases = self.use_aliases.clone();
                func_emitter.chunk.strict_types = self.chunk.strict_types;

                // 3. Process params
                let mut param_syms = Vec::new();
                for (i, info) in param_infos.iter().enumerate() {
                    let p_name = func_emitter.get_text(info.name_span);
                    if p_name.starts_with(b"$") {
                        let sym = func_emitter.interner.intern(&p_name[1..]);
                        let param_type = info.ty.and_then(|ty| func_emitter.convert_type(ty));
                        let default_value = if info.variadic {
                            None
                        } else {
                            info.default
                                .map(|expr| func_emitter.eval_constant_expr(expr))
                        };

                        param_syms.push(FuncParam {
                            name: sym,
                            by_ref: info.by_ref,
                            param_type,
                            is_variadic: info.variadic,
                            default_value,
                        });

                        if info.variadic {
                            func_emitter
                                .chunk
                                .code
                                .push(OpCode::RecvVariadic(i as u32));
                        } else if let Some(default_expr) = info.default {
                            let val = func_emitter.eval_constant_expr(default_expr);
                            let idx = func_emitter.add_constant(val);
                            func_emitter
                                .chunk
                                .code
                                .push(OpCode::RecvInit(i as u32, idx as u16));
                        } else {
                            func_emitter.push_op(OpCode::Recv(i as u32));
                        }
                    }
                }

                let (mut func_chunk, is_generator) = func_emitter.compile(body);
                func_chunk.returns_ref = *by_ref;

                // Extract uses
                let mut use_syms = Vec::new();
                for use_var in *uses {
                    let u_name = self.get_text(use_var.var.span);
                    if u_name.starts_with(b"$") {
                        let sym = self.interner.intern(&u_name[1..]);
                        use_syms.push(sym);

                        if use_var.by_ref {
                            self.push_op(OpCode::LoadRef(sym));
                        } else {
                            // Emit code to push the captured variable onto the stack
                            self.push_op(OpCode::LoadVar(sym));
                            self.push_op(OpCode::Copy);
                        }
                    }
                }

                // Convert return type
                let ret_type = return_type.and_then(|rt| self.convert_type(rt));

                let start_line = span.line_info(self.source).map(|li| li.line as u32);
                let end_line = close_brace_span
                    .and_then(|s| s.line_info(self.source).map(|li| li.line as u32));

                let user_func = UserFunc {
                    params: param_syms,
                    uses: use_syms.clone(),
                    chunk: Rc::new(func_chunk),
                    is_static: *is_static,
                    is_generator,
                    statics: Rc::new(RefCell::new(HashMap::new())),
                    return_type: ret_type,
                    start_line,
                    end_line,
                };

                let func_res = Val::Resource(Rc::new(user_func));
                let const_idx = self.add_constant(func_res);

                self.chunk
                    .code
                    .push(OpCode::Closure(const_idx as u32, use_syms.len() as u32));
            }
            Expr::Call { func, args, .. } => {
                let has_unpack = args.iter().any(|arg| arg.unpack);

                match func {
                    Expr::Variable { span, .. } => {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            self.emit_expr(func);
                        } else {
                            let idx = self.add_constant(Val::String(name.to_vec().into()));
                            self.push_op(OpCode::Const(idx as u16));
                        }
                    }
                    _ => self.emit_expr(func),
                }

                if has_unpack {
                    self.push_op(OpCode::InitDynamicCall);
                    for arg in *args {
                        self.emit_expr(&arg.value);
                        if arg.unpack {
                            self.push_op(OpCode::SendUnpack);
                        } else {
                            self.push_op(OpCode::SendValEx);
                        }
                    }
                    self.push_op(OpCode::DoFcall);
                } else {
                    for arg in *args {
                        self.emit_expr(&arg.value);
                    }

                    self.push_op(OpCode::Call(args.len() as u8));
                }
            }
            Expr::Variable { span, .. } => {
                let name = self.get_text(*span);
                if name.starts_with(b"$") {
                    let var_name = &name[1..];
                    let sym = self.interner.intern(var_name);
                    self.push_op(OpCode::LoadVar(sym));
                } else {
                    // Constant fetch
                    let sym = self.interner.intern(name);
                    self.push_op(OpCode::FetchGlobalConst(sym));
                }
            }
            Expr::IndirectVariable { name, .. } => {
                self.emit_expr(name);
                self.push_op(OpCode::LoadVarDynamic);
            }
            Expr::Array { items, .. } => {
                self.push_op(OpCode::InitArray(items.len() as u32));
                for item in *items {
                    if item.unpack {
                        self.emit_expr(item.value);
                        self.push_op(OpCode::AddArrayUnpack);
                        continue;
                    }
                    if let Some(key) = item.key {
                        self.emit_expr(key);
                        self.emit_expr(item.value);
                        self.push_op(OpCode::AssignDim);
                    } else {
                        self.emit_expr(item.value);
                        self.push_op(OpCode::AppendArray);
                    }
                }
            }
            Expr::ArrayDimFetch { array, dim, .. } => {
                self.emit_expr(array);
                if let Some(d) = dim {
                    self.emit_expr(d);
                    self.push_op(OpCode::FetchDim);
                }
            }
            Expr::New { class, args, .. } => {
                // Check if this is an anonymous class
                if let Expr::AnonymousClass {
                    attributes,
                    modifiers,
                    args: _ctor_args,
                    extends,
                    implements,
                    members,
                    span,
                } = class
                {
                    // Extract parent information once
                    let (parent_name, parent_sym) = if let Some(parent_ref) = extends {
                        let parent_bytes = self.resolve_class_name(self.get_text(parent_ref.span));
                        let parent_sym = self.interner.intern(&parent_bytes);
                        (Some(parent_bytes), Some(parent_sym))
                    } else {
                        (None, None)
                    };

                    // Generate unique name and create class symbol
                    let anon_name =
                        self.generate_anonymous_class_name(parent_name.as_deref(), span);
                    let class_sym = self.interner.intern(anon_name.as_bytes());

                    // Define class with parent
                    self.chunk
                        .code
                        .push(OpCode::DefClass(class_sym, parent_sym));

                    // Set class line information
                    let start_line = span.line_info(self.source).map(|info| info.line as u32);
                    let end_line = span.line_info(self.source).map(|info| info.line as u32);
                    self.chunk
                        .code
                        .push(OpCode::SetClassLines(class_sym, start_line, end_line));

                    // Emit class metadata (attributes, modifiers, interfaces)
                    self.emit_class_metadata(class_sym, attributes, modifiers, implements);

                    // Emit class members with proper context
                    let prev_class = self.current_class;
                    self.current_class = Some(class_sym);
                    self.emit_members(class_sym, members);
                    self.current_class = prev_class;

                    // Finalize and instantiate
                    self.push_op(OpCode::FinalizeClass(class_sym));

                    // Emit constructor arguments and instantiate
                    for arg in *args {
                        self.emit_expr(arg.value);
                    }

                    self.chunk
                        .code
                        .push(OpCode::New(class_sym, args.len() as u8));
                } else if let Expr::Variable { span, .. } = class {
                    let name = self.get_text(*span);
                    if !name.starts_with(b"$") {
                        let class_sym = self.resolve_class_sym_from_span(*span);

                        for arg in *args {
                            self.emit_expr(arg.value);
                        }

                        self.chunk
                            .code
                            .push(OpCode::New(class_sym, args.len() as u8));
                    } else {
                        // Dynamic new $var()
                        // Emit expression to get class name (string)
                        self.emit_expr(class);

                        for arg in *args {
                            self.emit_expr(arg.value);
                        }

                        self.push_op(OpCode::NewDynamic(args.len() as u8));
                    }
                } else {
                    // Complex expression for class name
                    self.emit_expr(class);

                    for arg in *args {
                        self.emit_expr(arg.value);
                    }

                    self.push_op(OpCode::NewDynamic(args.len() as u8));
                }
            }
            Expr::PropertyFetch {
                target, property, ..
            } => {
                self.emit_expr(target);
                if let Expr::Variable { span, .. } = property {
                    let name = self.get_text(*span);
                    if !name.starts_with(b"$") {
                        let sym = self.interner.intern(name);
                        self.push_op(OpCode::FetchProp(sym));
                    } else {
                        // Dynamic property fetch $this->$prop
                        self.emit_expr(property);
                        self.push_op(OpCode::FetchPropDynamic);
                    }
                } else {
                    // Handle dynamic property fetch with expression: $this->{$expr}
                    self.emit_expr(property);
                    self.push_op(OpCode::FetchPropDynamic);
                }
            }
            Expr::MethodCall {
                target,
                method,
                args,
                ..
            } => {
                self.emit_expr(target);
                if let Expr::Variable { span, .. } = method {
                    let name = self.get_text(*span);
                    if !name.starts_with(b"$") {
                        for arg in *args {
                            self.emit_expr(arg.value);
                        }
                        let sym = self.interner.intern(name);
                        self.chunk
                            .code
                            .push(OpCode::CallMethod(sym, args.len() as u8));
                    } else {
                        // Dynamic method call: $obj->$method()
                        self.emit_expr(method);
                        for arg in *args {
                            self.emit_expr(arg.value);
                        }
                        self.chunk
                            .code
                            .push(OpCode::CallMethodDynamic(args.len() as u8));
                    }
                } else {
                    // Dynamic method call with expression: $obj->{$expr}()
                    self.emit_expr(method);
                    for arg in *args {
                        self.emit_expr(arg.value);
                    }
                    self.chunk
                        .code
                        .push(OpCode::CallMethodDynamic(args.len() as u8));
                }
            }
            Expr::StaticCall {
                class,
                method,
                args,
                ..
            } => {
                let mut class_emitted = false;
                if let Expr::Variable { span, .. } = class {
                    let class_name = self.get_text(*span);
                    if !class_name.starts_with(b"$") {
                        let class_sym = self.resolve_class_sym_from_span(*span);

                        if let Expr::Variable {
                            span: method_span, ..
                        } = method
                        {
                            let method_name = self.get_text(*method_span);
                            if !method_name.starts_with(b"$") {
                                for arg in *args {
                                    self.emit_expr(arg.value);
                                }
                                let method_sym = self.interner.intern(method_name);
                                self.push_op(OpCode::CallStaticMethod(
                                    class_sym,
                                    method_sym,
                                    args.len() as u8,
                                ));
                                class_emitted = true;
                            }
                        }

                        if !class_emitted {
                            // Class is static, but method is dynamic: Class::$method()
                            let resolved_name = self.resolve_class_name(class_name);
                            let idx = self.add_constant(Val::String(resolved_name.into()));
                            self.push_op(OpCode::Const(idx as u16));
                            self.emit_expr(method);
                            for arg in *args {
                                self.emit_expr(arg.value);
                            }
                            self.chunk
                                .code
                                .push(OpCode::CallStaticMethodDynamic(args.len() as u8));
                            class_emitted = true;
                        }
                    }
                }

                if !class_emitted {
                    // Dynamic static call: $class::$method()
                    self.emit_expr(class);
                    if let Expr::Variable { span, .. } = method {
                        let name = self.get_text(*span);
                        if !name.starts_with(b"$") {
                            let idx = self.add_constant(Val::String(name.to_vec().into()));
                            self.push_op(OpCode::Const(idx as u16));
                        } else {
                            self.emit_expr(method);
                        }
                    } else {
                        self.emit_expr(method);
                    }
                    for arg in *args {
                        self.emit_expr(arg.value);
                    }
                    self.chunk
                        .code
                        .push(OpCode::CallStaticMethodDynamic(args.len() as u8));
                }
            }
            Expr::ClassConstFetch {
                class, constant, ..
            } => {
                let mut is_class_keyword = false;
                if let Expr::Variable {
                    span: const_span, ..
                } = constant
                {
                    let const_name = self.get_text(*const_span);
                    if const_name.eq_ignore_ascii_case(b"class") {
                        is_class_keyword = true;
                    }
                }

                if let Expr::Variable { span, .. } = class {
                    let class_name = self.get_text(*span);
                    if !class_name.starts_with(b"$") {
                        if is_class_keyword {
                            if (class_name.eq_ignore_ascii_case(b"self")
                                || class_name.eq_ignore_ascii_case(b"static"))
                                && self.current_class.is_some()
                            {
                                let class_sym = self.current_class.unwrap();
                                let name_bytes =
                                    self.interner.lookup(class_sym).unwrap_or(b"");
                                let idx =
                                    self.add_constant(Val::String(name_bytes.to_vec().into()));
                                self.push_op(OpCode::Const(idx as u16));
                                return;
                            }

                            let resolved = self.resolve_class_name(class_name);
                            let idx = self.add_constant(Val::String(resolved.into()));
                            self.push_op(OpCode::Const(idx as u16));
                            return;
                        }

                        let class_sym = self.resolve_class_sym_from_span(*span);

                        if let Expr::Variable {
                            span: const_span, ..
                        } = constant
                        {
                            let const_name = self.get_text(*const_span);
                            if const_name.starts_with(b"$") {
                                let prop_name = &const_name[1..];
                                let prop_sym = self.interner.intern(prop_name);
                                self.chunk
                                    .code
                                    .push(OpCode::FetchStaticProp(class_sym, prop_sym));
                            } else {
                                let const_sym = self.interner.intern(const_name);
                                self.chunk
                                    .code
                                    .push(OpCode::FetchClassConst(class_sym, const_sym));
                            }
                        }
                        return;
                    }
                }

                // Dynamic class/object access
                self.emit_expr(class);
                if is_class_keyword {
                    self.push_op(OpCode::GetClass);
                } else {
                    if let Expr::Variable {
                        span: const_span, ..
                    } = constant
                    {
                        let const_name = self.get_text(*const_span);
                        if const_name.starts_with(b"$") {
                            // TODO: Dynamic class, static property: $obj::$prop
                            self.push_op(OpCode::Pop);
                            let idx = self.add_constant(Val::Null);
                            self.push_op(OpCode::Const(idx as u16));
                        } else {
                            let const_sym = self.interner.intern(const_name);
                            self.chunk
                                .code
                                .push(OpCode::FetchClassConstDynamic(const_sym));
                        }
                    } else {
                        self.push_op(OpCode::Pop);
                        let idx = self.add_constant(Val::Null);
                        self.push_op(OpCode::Const(idx as u16));
                    }
                }
            }
            Expr::Assign { var, expr, .. } => match var {
                Expr::Variable { span, .. } => {
                    self.emit_expr(expr);
                    let name = self.get_text(*span);
                    if name.starts_with(b"$") {
                        let var_name = &name[1..];
                        let sym = self.interner.intern(var_name);
                        self.push_op(OpCode::StoreVar(sym));
                        self.push_op(OpCode::LoadVar(sym));
                    }
                }
                Expr::IndirectVariable { name, .. } => {
                    self.emit_expr(name);
                    self.emit_expr(expr);
                    self.push_op(OpCode::StoreVarDynamic);
                }
                Expr::PropertyFetch {
                    target, property, ..
                } => {
                    self.emit_expr(target);
                    match property {
                        Expr::Variable { span, .. } => {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                self.emit_expr(property);
                                self.emit_expr(expr);
                                self.push_op(OpCode::AssignPropDynamic);
                            } else {
                                let sym = self.interner.intern(name);
                                self.emit_expr(expr);
                                self.push_op(OpCode::AssignProp(sym));
                            }
                        }
                        _ => {
                            self.emit_expr(property);
                            self.emit_expr(expr);
                            self.push_op(OpCode::AssignPropDynamic);
                        }
                    }
                }
                Expr::ClassConstFetch {
                    class, constant, ..
                } => {
                    self.emit_expr(expr);
                    if let Expr::Variable { span, .. } = class {
                        let class_name = self.get_text(*span);
                        if !class_name.starts_with(b"$") {
                            let class_sym = self.resolve_class_sym_from_span(*span);

                            if let Expr::Variable {
                                span: const_span, ..
                            } = constant
                            {
                                let const_name = self.get_text(*const_span);
                                if const_name.starts_with(b"$") {
                                    let prop_name = &const_name[1..];
                                    let prop_sym = self.interner.intern(prop_name);
                                    self.chunk
                                        .code
                                        .push(OpCode::AssignStaticProp(class_sym, prop_sym));
                                }
                            }
                        }
                    }
                }
                Expr::ArrayDimFetch { .. } => {
                    let (base, keys) = Self::flatten_dim_fetch(var);

                    if let Expr::ClassConstFetch {
                        class, constant, ..
                    } = base
                    {
                        if let Expr::Variable { span, .. } = class {
                            let class_name = self.get_text(*span);
                            if !class_name.starts_with(b"$") {
                                if let Expr::Variable {
                                    span: const_span, ..
                                } = constant
                                {
                                    let const_name = self.get_text(*const_span);
                                    if const_name.starts_with(b"$") {
                                        let prop_name = &const_name[1..];
                                        let prop_sym = self.interner.intern(prop_name);
                                        let class_sym = self.resolve_class_sym_from_span(*span);

                                        self.chunk
                                            .code
                                            .push(OpCode::FetchStaticProp(class_sym, prop_sym));

                                        for key in &keys {
                                            if let Some(k) = key {
                                                self.emit_expr(k);
                                            } else {
                                                let idx = self.add_constant(Val::AppendPlaceholder);
                                                self.push_op(OpCode::Const(idx as u16));
                                            }
                                        }

                                        self.emit_expr(expr);

                                        self.push_op(OpCode::StoreNestedDim(
                                            keys.len() as u8,
                                        ));

                                        self.chunk
                                            .code
                                            .push(OpCode::AssignStaticProp(class_sym, prop_sym));
                                        return;
                                    }
                                }
                            }
                        }
                    }

                    if let Expr::PropertyFetch {
                        target, property, ..
                    } = base
                    {
                        self.emit_expr(target);
                        self.push_op(OpCode::Dup);

                        let static_sym = if let Expr::Variable { span, .. } = property {
                            let name = self.get_text(*span);
                            if !name.starts_with(b"$") {
                                Some(self.interner.intern(name))
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(sym) = static_sym {
                            self.push_op(OpCode::FetchProp(sym));

                            for key in &keys {
                                if let Some(k) = key {
                                    self.emit_expr(k);
                                } else {
                                    let idx = self.add_constant(Val::AppendPlaceholder);
                                    self.push_op(OpCode::Const(idx as u16));
                                }
                            }

                            self.emit_expr(expr);

                            self.chunk
                                .code
                                .push(OpCode::StoreNestedDim(keys.len() as u8));

                            self.push_op(OpCode::AssignProp(sym));
                        } else {
                            // Dynamic property name: $obj->{$expr}['key'] = val
                            // Stack: [obj, obj] (target already duped)

                            self.emit_expr(property); // [obj, obj, name]
                            self.push_op(OpCode::Dup); // [obj, obj, name, name]

                            let suffix = self.chunk.code.len();
                            let tmp_name_str = format!("__tmp_prop_name_{}", suffix);
                            let tmp_name = self.interner.intern(tmp_name_str.as_bytes());
                            self.push_op(OpCode::StoreVar(tmp_name)); // [obj, obj, name]

                            self.push_op(OpCode::FetchPropDynamic); // [obj, array]

                            for key in &keys {
                                if let Some(k) = key {
                                    self.emit_expr(k);
                                } else {
                                    let idx = self.add_constant(Val::AppendPlaceholder);
                                    self.push_op(OpCode::Const(idx as u16));
                                }
                            }

                            self.emit_expr(expr); // [obj, array, keys..., val]

                            self.chunk
                                .code
                                .push(OpCode::StoreNestedDim(keys.len() as u8)); // [obj, modified_array]

                            let tmp_val_str = format!("__tmp_assign_val_{}", suffix);
                            let tmp_val = self.interner.intern(tmp_val_str.as_bytes());
                            self.push_op(OpCode::StoreVar(tmp_val)); // [obj]

                            self.push_op(OpCode::LoadVar(tmp_name)); // [obj, name]
                            self.push_op(OpCode::LoadVar(tmp_val)); // [obj, name, modified_array]

                            self.push_op(OpCode::AssignPropDynamic); // [result]
                        }
                    } else {
                        self.emit_expr(base);
                        for key in &keys {
                            if let Some(k) = key {
                                self.emit_expr(k);
                            } else {
                                let idx = self.add_constant(Val::AppendPlaceholder);
                                self.push_op(OpCode::Const(idx as u16));
                            }
                        }

                        self.emit_expr(expr);

                        self.chunk
                            .code
                            .push(OpCode::StoreNestedDim(keys.len() as u8));

                        if let Expr::Variable { span, .. } = base {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                let var_name = &name[1..];
                                let sym = self.interner.intern(var_name);
                                self.push_op(OpCode::StoreVar(sym));
                                self.push_op(OpCode::LoadVar(sym));
                            }
                        }
                    }
                }
                Expr::Array { items, .. } => {
                    // list($a, $b, $c) = expr
                    // Emit the right-hand side expression (should be an array)
                    self.emit_expr(expr);

                    // Extract each element and assign to variables
                    for (i, item) in items.iter().enumerate() {
                        let value = item.value;
                        if let Expr::Variable { span, .. } = value {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                // Duplicate the array on stack for next iteration
                                self.push_op(OpCode::Dup);
                                // Push the index
                                let idx_val = Val::Int(i as i64);
                                let idx_const = self.add_constant(idx_val);
                                self.push_op(OpCode::Const(idx_const as u16));
                                // Fetch array[i] (pops index and duplicated array, pushes value, leaves original array)
                                self.push_op(OpCode::FetchDim);
                                // Store to variable (pops value)
                                let var_name = &name[1..];
                                let sym = self.interner.intern(var_name);
                                self.push_op(OpCode::StoreVar(sym));
                            }
                        }
                    }
                    // Leave the original array on the stack as the assignment result
                    // (statement-level Pop will remove it if needed)
                }
                _ => {}
            },
            Expr::AssignRef { var, expr, .. } => {
                match var {
                    Expr::Variable { span, .. } => {
                        // Check if expr is a variable
                        let mut handled = false;
                        if let Expr::Variable { span: src_span, .. } = expr {
                            let src_name = self.get_text(*src_span);
                            if src_name.starts_with(b"$") {
                                let src_sym = self.interner.intern(&src_name[1..]);
                                self.push_op(OpCode::MakeVarRef(src_sym));
                                handled = true;
                            }
                        }

                        if !handled {
                            self.emit_expr_for_write(expr);
                            self.push_op(OpCode::MakeRef);
                        }

                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let var_name = &name[1..];
                            let sym = self.interner.intern(var_name);
                            self.push_op(OpCode::AssignRef(sym));
                            self.push_op(OpCode::LoadVar(sym));
                        }
                    }
                    Expr::ArrayDimFetch {
                        array: array_var,
                        dim,
                        ..
                    } => {
                        self.emit_expr(array_var);
                        if let Some(d) = dim {
                            self.emit_expr(d);
                        } else {
                            // TODO: Handle append
                            self.push_op(OpCode::Const(0));
                        }

                        let mut handled = false;
                        if let Expr::Variable { span: src_span, .. } = expr {
                            let src_name = self.get_text(*src_span);
                            if src_name.starts_with(b"$") {
                                let src_sym = self.interner.intern(&src_name[1..]);
                                self.push_op(OpCode::MakeVarRef(src_sym));
                                handled = true;
                            }
                        }

                        if !handled {
                            self.emit_expr(expr);
                            self.push_op(OpCode::MakeRef);
                        }

                        self.push_op(OpCode::AssignDimRef);

                        // Store back the updated array if target is a variable
                        if let Expr::Variable { span, .. } = array_var {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                let sym = self.interner.intern(&name[1..]);
                                self.push_op(OpCode::StoreVar(sym));
                            } else {
                                self.push_op(OpCode::Pop);
                            }
                        } else {
                            self.push_op(OpCode::Pop);
                        }
                    }
                    _ => {
                        // TODO: Support other targets for reference assignment
                    }
                }
            }
            Expr::AssignOp { var, op, expr, .. } => {
                match var {
                    Expr::Variable { span, .. } => {
                        let name = self.get_text(*span);
                        if name.starts_with(b"$") {
                            let var_name = &name[1..];
                            let sym = self.interner.intern(var_name);

                            if let AssignOp::Coalesce = op {
                                // Check if set
                                self.push_op(OpCode::IssetVar(sym));
                                let jump_idx = self.chunk.code.len();
                                self.push_op(OpCode::JmpIfTrue(0));

                                // Not set: Evaluate expr, assign, load
                                self.emit_expr(expr);
                                self.push_op(OpCode::StoreVar(sym));
                                self.push_op(OpCode::LoadVar(sym));

                                let end_jump_idx = self.chunk.code.len();
                                self.push_op(OpCode::Jmp(0));

                                // Set: Load var
                                let label_set = self.chunk.code.len();
                                self.chunk.code[jump_idx] = OpCode::JmpIfTrue(label_set as u32);
                                self.push_op(OpCode::LoadVar(sym));

                                // End
                                let label_end = self.chunk.code.len();
                                self.chunk.code[end_jump_idx] = OpCode::Jmp(label_end as u32);
                                return;
                            }

                            // Load var
                            self.push_op(OpCode::LoadVar(sym));

                            // Evaluate expr
                            self.emit_expr(expr);

                            // Op
                            match op {
                                AssignOp::Plus => self.push_op(OpCode::Add),
                                AssignOp::Minus => self.push_op(OpCode::Sub),
                                AssignOp::Mul => self.push_op(OpCode::Mul),
                                AssignOp::Div => self.push_op(OpCode::Div),
                                AssignOp::Mod => self.push_op(OpCode::Mod),
                                AssignOp::Concat => self.push_op(OpCode::Concat),
                                AssignOp::Pow => self.push_op(OpCode::Pow),
                                AssignOp::BitAnd => self.push_op(OpCode::BitwiseAnd),
                                AssignOp::BitOr => self.push_op(OpCode::BitwiseOr),
                                AssignOp::BitXor => self.push_op(OpCode::BitwiseXor),
                                AssignOp::ShiftLeft => self.push_op(OpCode::ShiftLeft),
                                AssignOp::ShiftRight => self.push_op(OpCode::ShiftRight),
                                _ => {} // TODO: Implement other ops
                            }

                            // Store
                            self.push_op(OpCode::StoreVar(sym));
                            self.push_op(OpCode::LoadVar(sym));
                        }
                    }
                    Expr::IndirectVariable { name, .. } => {
                        self.emit_expr(name);
                        self.push_op(OpCode::Dup);

                        if let AssignOp::Coalesce = op {
                            self.push_op(OpCode::IssetVarDynamic);
                            let jump_idx = self.chunk.code.len();
                            self.push_op(OpCode::JmpIfTrue(0));

                            self.emit_expr(expr);
                            self.push_op(OpCode::StoreVarDynamic);

                            let end_jump_idx = self.chunk.code.len();
                            self.push_op(OpCode::Jmp(0));

                            let label_set = self.chunk.code.len();
                            self.chunk.code[jump_idx] = OpCode::JmpIfTrue(label_set as u32);
                            self.push_op(OpCode::LoadVarDynamic);

                            let label_end = self.chunk.code.len();
                            self.chunk.code[end_jump_idx] = OpCode::Jmp(label_end as u32);
                            return;
                        }

                        self.push_op(OpCode::LoadVarDynamic);
                        self.emit_expr(expr);

                        match op {
                            AssignOp::Plus => self.push_op(OpCode::Add),
                            AssignOp::Minus => self.push_op(OpCode::Sub),
                            AssignOp::Mul => self.push_op(OpCode::Mul),
                            AssignOp::Div => self.push_op(OpCode::Div),
                            AssignOp::Mod => self.push_op(OpCode::Mod),
                            AssignOp::Concat => self.push_op(OpCode::Concat),
                            AssignOp::Pow => self.push_op(OpCode::Pow),
                            AssignOp::BitAnd => self.push_op(OpCode::BitwiseAnd),
                            AssignOp::BitOr => self.push_op(OpCode::BitwiseOr),
                            AssignOp::BitXor => self.push_op(OpCode::BitwiseXor),
                            AssignOp::ShiftLeft => self.push_op(OpCode::ShiftLeft),
                            AssignOp::ShiftRight => self.push_op(OpCode::ShiftRight),
                            _ => {}
                        }

                        self.push_op(OpCode::StoreVarDynamic);
                    }
                    Expr::PropertyFetch {
                        target, property, ..
                    } => {
                        self.emit_expr(target);
                        self.push_op(OpCode::Dup);

                        if let Expr::Variable { span, .. } = property {
                            let name = self.get_text(*span);
                            if !name.starts_with(b"$") {
                                let sym = self.interner.intern(name);

                                if let AssignOp::Coalesce = op {
                                    self.push_op(OpCode::Dup);
                                    self.push_op(OpCode::IssetProp(sym));
                                    let jump_idx = self.chunk.code.len();
                                    self.push_op(OpCode::JmpIfTrue(0));

                                    self.emit_expr(expr);
                                    self.push_op(OpCode::AssignProp(sym));

                                    let end_jump_idx = self.chunk.code.len();
                                    self.push_op(OpCode::Jmp(0));

                                    let label_set = self.chunk.code.len();
                                    self.chunk.code[jump_idx] = OpCode::JmpIfTrue(label_set as u32);
                                    self.push_op(OpCode::FetchProp(sym));

                                    let label_end = self.chunk.code.len();
                                    self.chunk.code[end_jump_idx] = OpCode::Jmp(label_end as u32);
                                    return;
                                }

                                self.push_op(OpCode::FetchProp(sym));

                                self.emit_expr(expr);

                                match op {
                                    AssignOp::Plus => self.push_op(OpCode::Add),
                                    AssignOp::Minus => self.push_op(OpCode::Sub),
                                    AssignOp::Mul => self.push_op(OpCode::Mul),
                                    AssignOp::Div => self.push_op(OpCode::Div),
                                    AssignOp::Mod => self.push_op(OpCode::Mod),
                                    AssignOp::Concat => self.push_op(OpCode::Concat),
                                    AssignOp::Pow => self.push_op(OpCode::Pow),
                                    AssignOp::BitAnd => self.push_op(OpCode::BitwiseAnd),
                                    AssignOp::BitOr => self.push_op(OpCode::BitwiseOr),
                                    AssignOp::BitXor => self.push_op(OpCode::BitwiseXor),
                                    AssignOp::ShiftLeft => self.push_op(OpCode::ShiftLeft),
                                    AssignOp::ShiftRight => {
                                        self.push_op(OpCode::ShiftRight)
                                    }
                                    _ => {}
                                }

                                self.push_op(OpCode::AssignProp(sym));
                            }
                        }
                    }
                    Expr::ClassConstFetch {
                        class, constant, ..
                    } => {
                        if let Expr::Variable { span, .. } = class {
                            let class_name = self.get_text(*span);
                            if !class_name.starts_with(b"$") {
                                let class_sym = self.resolve_class_sym_from_span(*span);
                                let resolved_name = self.resolve_class_name(class_name);

                                if let Expr::Variable {
                                    span: const_span, ..
                                } = constant
                                {
                                    let const_name = self.get_text(*const_span);
                                    if const_name.starts_with(b"$") {
                                        let prop_name = &const_name[1..];
                                        let prop_sym = self.interner.intern(prop_name);

                                        if let AssignOp::Coalesce = op {
                                            let idx =
                                                self.add_constant(Val::String(resolved_name.into()));
                                            self.push_op(OpCode::Const(idx as u16));
                                            self.push_op(OpCode::IssetStaticProp(prop_sym));

                                            let jump_idx = self.chunk.code.len();
                                            self.push_op(OpCode::JmpIfFalse(0));

                                            self.chunk
                                                .code
                                                .push(OpCode::FetchStaticProp(class_sym, prop_sym));
                                            let jump_end_idx = self.chunk.code.len();
                                            self.push_op(OpCode::Jmp(0));

                                            let label_assign = self.chunk.code.len();
                                            self.chunk.code[jump_idx] =
                                                OpCode::JmpIfFalse(label_assign as u32);

                                            self.emit_expr(expr);
                                            self.push_op(OpCode::AssignStaticProp(
                                                class_sym, prop_sym,
                                            ));

                                            let label_end = self.chunk.code.len();
                                            self.chunk.code[jump_end_idx] =
                                                OpCode::Jmp(label_end as u32);
                                            return;
                                        }

                                        self.chunk
                                            .code
                                            .push(OpCode::FetchStaticProp(class_sym, prop_sym));
                                        self.emit_expr(expr);

                                        match op {
                                            AssignOp::Plus => self.push_op(OpCode::Add),
                                            AssignOp::Minus => self.push_op(OpCode::Sub),
                                            AssignOp::Mul => self.push_op(OpCode::Mul),
                                            AssignOp::Div => self.push_op(OpCode::Div),
                                            AssignOp::Mod => self.push_op(OpCode::Mod),
                                            AssignOp::Concat => {
                                                self.push_op(OpCode::Concat)
                                            }
                                            AssignOp::Pow => self.push_op(OpCode::Pow),
                                            AssignOp::BitAnd => {
                                                self.push_op(OpCode::BitwiseAnd)
                                            }
                                            AssignOp::BitOr => {
                                                self.push_op(OpCode::BitwiseOr)
                                            }
                                            AssignOp::BitXor => {
                                                self.push_op(OpCode::BitwiseXor)
                                            }
                                            AssignOp::ShiftLeft => {
                                                self.push_op(OpCode::ShiftLeft)
                                            }
                                            AssignOp::ShiftRight => {
                                                self.push_op(OpCode::ShiftRight)
                                            }
                                            _ => {}
                                        }

                                        self.chunk
                                            .code
                                            .push(OpCode::AssignStaticProp(class_sym, prop_sym));
                                    }
                                }
                            }
                        }
                    }
                    Expr::ArrayDimFetch { .. } => {
                        let (base, keys) = Self::flatten_dim_fetch(var);

                        // 1. Emit base array
                        self.emit_expr(base);

                        // 2. Emit keys
                        for key in &keys {
                            if let Some(k) = key {
                                self.emit_expr(k);
                            } else {
                                // Append not supported in AssignOp (e.g. $a[] += 1 is invalid)
                                // But maybe $a[] ??= 1 is valid? No, ??= is assign op.
                                // PHP Fatal error:  Cannot use [] for reading
                                // So we can assume keys are present for AssignOp (read-modify-write)
                                // But wait, $a[] = 1 is valid. $a[] += 1 is NOT valid.
                                // So we can panic or emit error if key is None.
                                // For now, push 0 or null?
                                // Actually, let's just push 0 as placeholder, but it will fail at runtime if used for reading.
                                self.push_op(OpCode::Const(0));
                            }
                        }

                        // 3. Fetch value (peek array & keys, push val)
                        // Stack: [array, keys...]
                        self.chunk
                            .code
                            .push(OpCode::FetchNestedDim(keys.len() as u8));
                        // Stack: [array, keys..., val]

                        if let AssignOp::Coalesce = op {
                            let jump_idx = self.chunk.code.len();
                            self.push_op(OpCode::Coalesce(0));

                            // If null, evaluate rhs
                            self.emit_expr(expr);

                            let label_store = self.chunk.code.len();
                            self.chunk.code[jump_idx] = OpCode::Coalesce(label_store as u32);
                        } else {
                            // 4. Emit expr (rhs)
                            self.emit_expr(expr);
                            // Stack: [array, keys..., val, rhs]

                            // 5. Op
                            match op {
                                AssignOp::Plus => self.push_op(OpCode::Add),
                                AssignOp::Minus => self.push_op(OpCode::Sub),
                                AssignOp::Mul => self.push_op(OpCode::Mul),
                                AssignOp::Div => self.push_op(OpCode::Div),
                                AssignOp::Mod => self.push_op(OpCode::Mod),
                                AssignOp::Concat => self.push_op(OpCode::Concat),
                                AssignOp::Pow => self.push_op(OpCode::Pow),
                                AssignOp::BitAnd => self.push_op(OpCode::BitwiseAnd),
                                AssignOp::BitOr => self.push_op(OpCode::BitwiseOr),
                                AssignOp::BitXor => self.push_op(OpCode::BitwiseXor),
                                AssignOp::ShiftLeft => self.push_op(OpCode::ShiftLeft),
                                AssignOp::ShiftRight => self.push_op(OpCode::ShiftRight),
                                _ => {}
                            }
                        }

                        // 6. Store result back
                        // Stack: [array, keys..., result]
                        self.chunk
                            .code
                            .push(OpCode::StoreNestedDim(keys.len() as u8));
                        // Stack: [new_array] (StoreNestedDim pushes the modified array back? No, wait.)

                        // Wait, I checked StoreNestedDim implementation.
                        // It does NOT push anything back.
                        // But assign_nested_dim pushes new_handle back!
                        // And StoreNestedDim calls assign_nested_dim.
                        // So StoreNestedDim DOES push new_array back.

                        // So Stack: [new_array]

                        // 7. Update variable if base was a variable
                        if let Expr::Variable { span, .. } = base {
                            let name = self.get_text(*span);
                            if name.starts_with(b"$") {
                                let var_name = &name[1..];
                                let sym = self.interner.intern(var_name);
                                self.push_op(OpCode::StoreVar(sym));
                                self.push_op(OpCode::LoadVar(sym));
                            }
                        }
                    }
                    _ => {} // TODO: Other targets
                }
            }
            Expr::MagicConst { kind, span, .. } => {
                // Handle magic constants like __DIR__, __FILE__, etc.
                let value = match kind {
                    MagicConstKind::Dir => {
                        // __DIR__ returns the directory of the current file
                        if let Some(ref path) = self.file_path {
                            if let Some(parent) = Path::new(path).parent() {
                                let dir_str = parent.to_string_lossy();
                                Val::String(Rc::new(dir_str.as_bytes().to_vec()))
                            } else {
                                Val::String(Rc::new(b".".to_vec()))
                            }
                        } else {
                            // No file path tracked, return current directory
                            Val::String(Rc::new(b".".to_vec()))
                        }
                    }
                    MagicConstKind::File => {
                        // __FILE__ returns the full path of the current file
                        if let Some(ref path) = self.file_path {
                            Val::String(Rc::new(path.as_bytes().to_vec()))
                        } else {
                            // No file path tracked, return empty string
                            Val::String(Rc::new(Vec::new()))
                        }
                    }
                    MagicConstKind::Line => {
                        // __LINE__ returns the current line number
                        let line = self.get_line_number(span.start);
                        Val::Int(line)
                    }
                    MagicConstKind::Class => {
                        // __CLASS__ returns the class name (or empty if not in a class)
                        if let Some(class_sym) = self.current_class {
                            if let Some(class_name) = self.interner.lookup(class_sym) {
                                Val::String(Rc::new(class_name.to_vec()))
                            } else {
                                Val::String(Rc::new(Vec::new()))
                            }
                        } else {
                            Val::String(Rc::new(Vec::new()))
                        }
                    }
                    MagicConstKind::Trait => {
                        // __TRAIT__ returns the trait name
                        if let Some(trait_sym) = self.current_trait {
                            if let Some(trait_name) = self.interner.lookup(trait_sym) {
                                Val::String(Rc::new(trait_name.to_vec()))
                            } else {
                                Val::String(Rc::new(Vec::new()))
                            }
                        } else {
                            Val::String(Rc::new(Vec::new()))
                        }
                    }
                    MagicConstKind::Method => {
                        // __METHOD__ returns Class::method or just method
                        if let Some(func_sym) = self.current_function {
                            if let Some(func_name) = self.interner.lookup(func_sym) {
                                Val::String(Rc::new(func_name.to_vec()))
                            } else {
                                Val::String(Rc::new(Vec::new()))
                            }
                        } else {
                            Val::String(Rc::new(Vec::new()))
                        }
                    }
                    MagicConstKind::Function => {
                        // __FUNCTION__ returns the function name (without class)
                        if let Some(func_sym) = self.current_function {
                            if let Some(func_name) = self.interner.lookup(func_sym) {
                                // Strip class prefix if present (Class::method -> method)
                                let name_str = func_name;
                                if let Some(pos) = name_str.iter().position(|&b| b == b':') {
                                    if pos + 1 < name_str.len() && name_str[pos + 1] == b':' {
                                        // Found ::, return part after it
                                        Val::String(Rc::new(name_str[pos + 2..].to_vec()))
                                    } else {
                                        Val::String(Rc::new(name_str.to_vec()))
                                    }
                                } else {
                                    Val::String(Rc::new(name_str.to_vec()))
                                }
                            } else {
                                Val::String(Rc::new(Vec::new()))
                            }
                        } else {
                            Val::String(Rc::new(Vec::new()))
                        }
                    }
                    MagicConstKind::Namespace => {
                        // __NAMESPACE__ returns the namespace name
                        if let Some(ns_sym) = self.current_namespace {
                            if let Some(ns_name) = self.interner.lookup(ns_sym) {
                                Val::String(Rc::new(ns_name.to_vec()))
                            } else {
                                Val::String(Rc::new(Vec::new()))
                            }
                        } else {
                            Val::String(Rc::new(Vec::new()))
                        }
                    }
                    MagicConstKind::Property => {
                        // __PROPERTY__ (PHP 8.3+) - not commonly used yet
                        // Would need property context tracking
                        Val::String(Rc::new(Vec::new()))
                    }
                };
                let idx = self.add_constant(value);
                self.push_op(OpCode::Const(idx as u16));
            }
            _ => {}
        }
    }

    fn flatten_dim_fetch<'a, 'ast>(
        mut expr: &'a Expr<'ast>,
    ) -> (&'a Expr<'ast>, Vec<Option<&'a Expr<'ast>>>) {
        let mut keys = Vec::new();
        while let Expr::ArrayDimFetch { array, dim, .. } = expr {
            keys.push(*dim);
            expr = array;
        }
        keys.reverse();
        (expr, keys)
    }

    fn add_constant(&mut self, val: Val) -> usize {
        self.chunk.constants.push(val);
        self.chunk.constants.len() - 1
    }

    fn eval_constant_expr(&self, expr: &Expr) -> Val {
        match expr {
            Expr::Integer { value, .. } => {
                let s_str = std::str::from_utf8(value).unwrap_or("0");
                if let Ok(i) = s_str.parse::<i64>() {
                    Val::Int(i)
                } else {
                    Val::Int(0)
                }
            }
            Expr::Float { value, .. } => {
                let s_str = std::str::from_utf8(value).unwrap_or("0.0");
                if let Ok(f) = s_str.parse::<f64>() {
                    Val::Float(f)
                } else {
                    Val::Float(0.0)
                }
            }
            Expr::String { value, .. } => {
                let s = value;
                if s.len() >= 2
                    && ((s[0] == b'"' && s[s.len() - 1] == b'"')
                        || (s[0] == b'\'' && s[s.len() - 1] == b'\''))
                {
                    Val::String(s[1..s.len() - 1].to_vec().into())
                } else {
                    Val::String(s.to_vec().into())
                }
            }
            Expr::Boolean { value, .. } => Val::Bool(*value),
            Expr::Null { .. } => Val::Null,
            Expr::Array { items, .. } => {
                if items.is_empty() {
                    Val::Array(Rc::new(crate::core::value::ArrayData::new()))
                } else {
                    // Build a compile-time constant array template
                    use crate::core::value::ConstArrayKey;
                    use indexmap::IndexMap;

                    let mut const_array = IndexMap::new();
                    let mut next_index = 0i64;

                    for item in *items {
                        if item.unpack {
                            // Array unpacking not supported in constant expressions
                            continue;
                        }

                        let val = self.eval_constant_expr(item.value);

                        if let Some(key_expr) = item.key {
                            let key_val = self.eval_constant_expr(key_expr);
                            let key = match key_val {
                                Val::Int(i) => {
                                    if i >= next_index {
                                        next_index = i + 1;
                                    }
                                    ConstArrayKey::Int(i)
                                }
                                Val::String(s) => ConstArrayKey::Str(s),
                                Val::Float(f) => {
                                    let i = f as i64;
                                    if i >= next_index {
                                        next_index = i + 1;
                                    }
                                    ConstArrayKey::Int(i)
                                }
                                Val::Bool(b) => {
                                    let i = if b { 1 } else { 0 };
                                    if i >= next_index {
                                        next_index = i + 1;
                                    }
                                    ConstArrayKey::Int(i)
                                }
                                _ => ConstArrayKey::Int(next_index),
                            };
                            const_array.insert(key, val);
                        } else {
                            const_array.insert(ConstArrayKey::Int(next_index), val);
                            next_index += 1;
                        }
                    }

                    Val::ConstArray(Rc::new(const_array))
                }
            }
            Expr::ClassConstFetch {
                class, constant, ..
            } => {
                const TARGET_CLASS: i64 = 1 << 0;
                const TARGET_FUNCTION: i64 = 1 << 1;
                const TARGET_METHOD: i64 = 1 << 2;
                const TARGET_PROPERTY: i64 = 1 << 3;
                const TARGET_CLASS_CONST: i64 = 1 << 4;
                const TARGET_PARAMETER: i64 = 1 << 5;
                const TARGET_CONST: i64 = 1 << 6;
                const TARGET_ALL: i64 = (1 << 7) - 1;
                const IS_REPEATABLE: i64 = 1 << 7;

                if let (
                    Expr::Variable {
                        span: class_span, ..
                    },
                    Expr::Variable {
                        span: const_span, ..
                    },
                ) = (class, constant)
                {
                    let class_name = self.get_text(*class_span);
                    let const_name = self.get_text(*const_span);
                    let is_attribute_class = class_name.eq_ignore_ascii_case(b"Attribute")
                        || class_name.ends_with(b"\\Attribute")
                        || class_name.ends_with(b"\\attribute");
                    if is_attribute_class {
                        let value = if const_name.eq_ignore_ascii_case(b"TARGET_CLASS") {
                            TARGET_CLASS
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_FUNCTION") {
                            TARGET_FUNCTION
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_METHOD") {
                            TARGET_METHOD
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_PROPERTY") {
                            TARGET_PROPERTY
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_CLASS_CONST") {
                            TARGET_CLASS_CONST
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_PARAMETER") {
                            TARGET_PARAMETER
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_CONST") {
                            TARGET_CONST
                        } else if const_name.eq_ignore_ascii_case(b"TARGET_ALL") {
                            TARGET_ALL
                        } else if const_name.eq_ignore_ascii_case(b"IS_REPEATABLE") {
                            IS_REPEATABLE
                        } else {
                            0
                        };
                        return Val::Int(value);
                    }
                }

                Val::Null
            }
            _ => Val::Null,
        }
    }

    fn build_attribute_list(&self, groups: &[AttributeGroup]) -> Val {
        use crate::core::value::ConstArrayKey;
        use indexmap::IndexMap;

        let mut attrs = IndexMap::new();
        let mut attr_index = 0i64;

        for group in groups {
            for attr in group.attributes {
                let name_bytes = self.get_text(attr.name.span);
                let mut args = IndexMap::new();
                let mut next_index = 0i64;

                for arg in attr.args {
                    let value = self.eval_constant_expr(arg.value);
                    let key = if let Some(name) = arg.name {
                        let key_bytes = self.get_text(name.span);
                        ConstArrayKey::Str(Rc::new(key_bytes.to_vec()))
                    } else {
                        let key = ConstArrayKey::Int(next_index);
                        next_index += 1;
                        key
                    };
                    args.insert(key, value);
                }

                let mut attr_map = IndexMap::new();
                attr_map.insert(
                    ConstArrayKey::Str(Rc::new(b"name".to_vec())),
                    Val::String(Rc::new(name_bytes.to_vec())),
                );
                attr_map.insert(
                    ConstArrayKey::Str(Rc::new(b"args".to_vec())),
                    Val::ConstArray(Rc::new(args)),
                );

                attrs.insert(
                    ConstArrayKey::Int(attr_index),
                    Val::ConstArray(Rc::new(attr_map)),
                );
                attr_index += 1;
            }
        }

        Val::ConstArray(Rc::new(attrs))
    }

    fn get_text(&self, span: crate::parser::span::Span) -> &'src [u8] {
        &self.source[span.start..span.end]
    }

    /// Emit constants for static property access (Class::$property)
    /// Returns true if successfully emitted, false if not a valid static property reference
    fn emit_static_property_access(&mut self, class: &Expr, constant: &Expr) -> bool {
        if let (
            Expr::Variable {
                name: class_span, ..
            },
            Expr::Variable {
                name: prop_span, ..
            },
        ) = (class, constant)
        {
            let class_name = self.get_text(*class_span);
            let prop_name = self.get_text(*prop_span);

            // Valid static property: Class::$property (class name without $, property with $)
            if !class_name.starts_with(b"$") && prop_name.starts_with(b"$") {
                let resolved_name = self.resolve_class_name(class_name);
                let class_idx = self.add_constant(Val::String(Rc::new(resolved_name)));
                let prop_idx = self.add_constant(Val::String(Rc::new(prop_name[1..].to_vec())));
                self.push_op(OpCode::Const(class_idx as u16));
                self.push_op(OpCode::Const(prop_idx as u16));
                return true;
            }
        }
        false
    }

    /// Calculate line number from byte offset (1-indexed)
    fn get_line_number(&self, offset: usize) -> i64 {
        let mut line = 1i64;
        for (i, &byte) in self.source.iter().enumerate() {
            if i >= offset {
                break;
            }
            if byte == b'\n' {
                line += 1;
            }
        }
        line
    }

    /// Convert AST Type to ReturnType
    fn convert_type(&mut self, ty: &Type) -> Option<ReturnType> {
        match ty {
            Type::Simple(tok) => match tok.kind {
                TokenKind::TypeInt => Some(ReturnType::Int),
                TokenKind::TypeFloat => Some(ReturnType::Float),
                TokenKind::TypeString => Some(ReturnType::String),
                TokenKind::TypeBool => Some(ReturnType::Bool),
                TokenKind::Array => Some(ReturnType::Array),
                TokenKind::TypeObject => Some(ReturnType::Object),
                TokenKind::TypeVoid => Some(ReturnType::Void),
                TokenKind::TypeNever => Some(ReturnType::Never),
                TokenKind::TypeMixed => Some(ReturnType::Mixed),
                TokenKind::TypeNull => Some(ReturnType::Null),
                TokenKind::TypeTrue => Some(ReturnType::True),
                TokenKind::TypeFalse => Some(ReturnType::False),
                TokenKind::TypeCallable => Some(ReturnType::Callable),
                TokenKind::TypeIterable => Some(ReturnType::Iterable),
                TokenKind::Static => Some(ReturnType::Static),
                _ => None,
            },
            Type::Name(name) => {
                let name_str = self.get_text(name.span);
                let resolved = self.resolve_class_name(name_str);
                let sym = self.interner.intern(&resolved);
                Some(ReturnType::Named(sym))
            }
            Type::Union(types) => {
                let converted: Vec<_> = types.iter().filter_map(|t| self.convert_type(t)).collect();
                if converted.is_empty() {
                    None
                } else {
                    Some(ReturnType::Union(converted))
                }
            }
            Type::Intersection(types) => {
                let converted: Vec<_> = types.iter().filter_map(|t| self.convert_type(t)).collect();
                if converted.is_empty() {
                    None
                } else {
                    Some(ReturnType::Intersection(converted))
                }
            }
            Type::Nullable(inner) => self
                .convert_type(inner)
                .map(|t| ReturnType::Nullable(Box::new(t))),
        }
    }

    fn emit_expr_for_write(&mut self, expr: &Expr) {
        match expr {
            Expr::Variable { span, .. } => {
                let name = self.get_text(*span);
                if name.starts_with(b"$") {
                    let sym = self.interner.intern(&name[1..]);
                    self.push_op(OpCode::LoadVar(sym));
                } else {
                    self.emit_expr(expr);
                }
            }
            Expr::ArrayDimFetch { array, dim, .. } => {
                self.emit_expr_for_write(array);
                if let Some(d) = dim {
                    self.emit_expr(d);
                } else {
                    // Append uses 0 as dummy dimension for FetchDimW (it will auto-increment)
                    let idx = self.add_constant(Val::Int(0));
                    self.push_op(OpCode::Const(idx as u16));
                }
                self.push_op(OpCode::FetchDimW);
            }
            Expr::PropertyFetch {
                target, property, ..
            } => {
                self.emit_expr_for_write(target);
                if let Expr::Variable { span, .. } = property {
                    let name = self.get_text(*span);
                    let idx = self.add_constant(Val::String(name.to_vec().into()));
                    self.push_op(OpCode::Const(idx as u16));
                } else {
                    self.emit_expr(property);
                }
                self.push_op(OpCode::FetchObjW);
            }
            Expr::ClassConstFetch {
                class, constant, ..
            } => {
                let is_static_prop = if let Expr::Variable { span, .. } = constant {
                    let name = self.get_text(*span);
                    name.starts_with(b"$")
                } else {
                    false
                };

                if is_static_prop {
                    let mut class_name = b"".to_vec();
                    if let Expr::Variable { span, .. } = class {
                        class_name = self.get_text(*span).to_vec();
                    }

                    if !class_name.starts_with(b"$") {
                        let resolved_name = self.resolve_class_name(&class_name);
                        let class_idx = self.add_constant(Val::String(Rc::new(resolved_name)));
                        self.push_op(OpCode::Const(class_idx as u16));

                        if let Expr::Variable {
                            span: prop_span, ..
                        } = constant
                        {
                            let prop_name = self.get_text(*prop_span);
                            let prop_idx =
                                self.add_constant(Val::String(Rc::new(prop_name[1..].to_vec())));
                            self.push_op(OpCode::Const(prop_idx as u16));
                            self.push_op(OpCode::FetchStaticPropW);
                        }
                    } else {
                        // Dynamic class: $obj::$prop
                        self.emit_expr(class);
                        if let Expr::Variable {
                            span: prop_span, ..
                        } = constant
                        {
                            let prop_name = self.get_text(*prop_span);
                            let prop_idx =
                                self.add_constant(Val::String(Rc::new(prop_name[1..].to_vec())));
                            self.push_op(OpCode::Const(prop_idx as u16));
                            self.push_op(OpCode::FetchStaticPropW);
                        }
                    }
                } else {
                    self.emit_expr(expr);
                }
            }
            _ => self.emit_expr(expr),
        }
    }
}
