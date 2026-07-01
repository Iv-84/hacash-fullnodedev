use dyn_clone::clone_box;
use field::Address as FieldAddress;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;

use crate::ir::*;
use crate::native::*;
use crate::rt::*;
use crate::value::*;
use crate::*;

mod call;
mod context;
mod cursor;
mod expr;
mod stmt;

use cursor::Cursor;

enum SymbolEntryV2 {
    Slot(u8),
    Bind(Box<dyn IRNode>),
    Const(Box<dyn IRNode>),
}

#[derive(Clone, Copy)]
struct SlotStateV2 {
    mutable: bool,
}

#[derive(Default)]
struct ParserModeV2 {
    expect_retval: bool,
    loop_depth: usize,
}

#[derive(Default)]
struct ParserEmitV2 {
    irnode: IRNodeArray,
    source_map: SourceMap,
}

#[derive(Default)]
struct ParserInjectedV2 {
    ext_params: Option<Vec<(String, ValueTy)>>,
    ext_libs: Option<Vec<(String, u8, Option<FieldAddress>)>>,
    ext_consts: Option<Vec<(String, Box<dyn IRNode>)>>,
    skip_empty_param_prelude: bool,
}

pub struct Syntax {
    cursor: Cursor,
    symbols: HashMap<String, SymbolEntryV2>,
    slots: HashMap<u8, SlotStateV2>,
    slot_used: HashSet<u8>,
    libs: HashMap<String, (u8, Option<FieldAddress>)>,
    local_alloc: u8,
    mode: ParserModeV2,
    emit: ParserEmitV2,
    injected: ParserInjectedV2,
}

impl Default for Syntax {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Syntax {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            cursor: Cursor::new(tokens),
            emit: ParserEmitV2 {
                irnode: IRNodeArray::with_opcode(Bytecode::IRBLOCK),
                ..Default::default()
            },
            ..Self::empty()
        }
    }

    fn empty() -> Self {
        Self {
            cursor: Cursor::new(Vec::new()),
            symbols: HashMap::new(),
            slots: HashMap::new(),
            slot_used: HashSet::new(),
            libs: HashMap::new(),
            local_alloc: 0,
            mode: ParserModeV2::default(),
            emit: ParserEmitV2::default(),
            injected: ParserInjectedV2::default(),
        }
    }

    pub fn with_params(
        mut self,
        params: Vec<(String, ValueTy)>,
        skip_empty_param_prelude: bool,
    ) -> Self {
        self.injected.ext_params = Some(params);
        self.injected.skip_empty_param_prelude = skip_empty_param_prelude;
        self
    }

    pub fn with_libs(mut self, libs: Vec<(String, u8, Option<FieldAddress>)>) -> Self {
        self.injected.ext_libs = Some(libs);
        self
    }

    pub fn with_consts(mut self, consts: Vec<(String, Box<dyn IRNode>)>) -> Self {
        self.injected.ext_consts = Some(consts);
        self
    }

    pub fn parse(mut self) -> Ret<(IRNodeArray, SourceMap)> {
        self.emit.irnode.push(push_empty());
        self.install_injected_libs()?;
        self.install_injected_consts()?;
        self.install_injected_params()?;

        let subs = self.parse_top_level_items()?;
        self.emit.irnode.subs.extend(subs);
        self.finalize_alloc()?;
        Ok((self.emit.irnode, self.emit.source_map))
    }

    fn install_injected_libs(&mut self) -> Rerr {
        if let Some(libs) = self.injected.ext_libs.take() {
            for (name, idx, addr) in libs {
                self.bind_lib(name, idx, addr)?;
            }
        }
        Ok(())
    }

    fn install_injected_consts(&mut self) -> Rerr {
        if let Some(consts) = self.injected.ext_consts.take() {
            for (name, node) in consts {
                self.register_const_symbol(name, node)?;
            }
        }
        Ok(())
    }

    fn install_injected_params(&mut self) -> Ret<()> {
        let Some(params) = self.injected.ext_params.take() else {
            return Ok(());
        };
        if params.is_empty() && self.injected.skip_empty_param_prelude {
            return Ok(());
        }
        let mut names = Vec::with_capacity(params.len());
        for (i, (name, _ty)) in params.iter().enumerate() {
            if i > u8::MAX as usize {
                return errf!("param index {} overflow", i);
            }
            let idx = i as u8;
            self.bind_slot(name.clone(), idx, SlotKind::Param)?;
            names.push(name.clone());
        }
        if !names.is_empty() {
            self.emit.source_map.register_param_names(names)?;
        }
        self.emit
            .source_map
            .register_param_prelude_count(params.len() as u8)?;
        self.emit
            .irnode
            .push(Self::build_param_prelude(params.len(), true)?);
        Ok(())
    }

    fn finalize_alloc(&mut self) -> Ret<()> {
        use Bytecode::*;
        if self.local_alloc == 0 {
            return Ok(());
        }
        let alloc = Box::new(IRNodeParam1 {
            hrtv: false,
            inst: ALLOC,
            para: self.local_alloc,
            text: s!(""),
        });
        let subs = &mut self.emit.irnode.subs;
        if subs.len() > 1 && subs[1].bytecode() == ALLOC as u8 {
            subs[1] = alloc;
        } else {
            subs[0] = alloc;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::Tokenizer;

    fn parse(src: &str) -> Ret<Vec<u8>> {
        let tokens = Tokenizer::new(src.as_bytes()).parse()?;
        let (ir, _) = Syntax::new(tokens).parse()?;
        drop_irblock_wrap(ir.serialize())
    }

    #[test]
    fn syntax_accepts_implicit_slot_alias() {
        let code = parse("return $7").unwrap();
        assert!(!code.is_empty());
    }

    #[test]
    fn syntax_accepts_assign_via_slot_alias_to_let_slot() {
        let code = parse("let x = 1\n$0 = 2\nreturn x").unwrap();
        assert!(!code.is_empty());
    }

    #[test]
    fn syntax_rejects_top_level_stray_closing_brace() {
        let err = parse("} return 1").unwrap_err().to_string();
        assert!(
            err.contains("unsupported token") || err.contains("top-level"),
            "{}",
            err
        );
    }

    #[test]
    fn syntax_rejects_ambiguous_hex_like_selector_name() {
        let err = parse("return self.deadbeef()").unwrap_err().to_string();
        assert!(err.contains("ambiguous selector"), "{}", err);
    }

    #[test]
    fn syntax_accepts_explicit_raw_selector() {
        let code = parse("return self.0xdeadbeef()").unwrap();
        assert_eq!(code[0], Bytecode::RET as u8);
        assert_eq!(code[1], Bytecode::CALLSELF as u8);
        assert_eq!(&code[2..6], &[0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn syntax_rejects_naked_hex_codecall_body() {
        let err = parse("codecall abcdef0123").unwrap_err().to_string();
        assert!(
            err.contains("no lib binding") || err.contains("codecall body"),
            "{}",
            err
        );
    }

    #[test]
    fn syntax_accepts_soft_separators_between_statements() {
        let code = parse(
            r#"
            var a = 1;;,;,
            var b = 2,,,;
            return a + b
            "#,
        )
        .unwrap();
        assert!(!code.is_empty());
    }

    #[test]
    fn syntax_soft_separators_do_not_change_call_arg_semantics() {
        let with_sep = parse("return tuple(1, 2;;, 3)").unwrap();
        let no_sep = parse("return tuple(1 2 3)").unwrap();
        assert_eq!(with_sep, no_sep);
    }

    #[test]
    fn syntax_accepts_soft_separators_in_map_and_param() {
        let code = parse(
            r#"
            param { a,;, b,, }
            var m = map { "x": a,,; "y": b }
            return m
            "#,
        )
        .unwrap();
        assert!(!code.is_empty());
    }

    #[test]
    fn syntax_rejects_unterminated_if_block_at_eof() {
        let err = parse("if true { return 1").unwrap_err().to_string();
        assert!(err.contains("block format invalid"), "{}", err);
    }

    #[test]
    fn syntax_rejects_unterminated_map_at_eof() {
        let err = parse("return map { \"a\": 1").unwrap_err().to_string();
        assert!(err.contains("map format invalid"), "{}", err);
    }

    #[test]
    fn syntax_packmap_uses_total_item_count() {
        let code = parse("return map { \"a\": 1, \"b\": 2 }").unwrap();
        let packmap_idx = code
            .iter()
            .position(|b| *b == Bytecode::PACKMAP as u8)
            .expect("PACKMAP must exist");
        assert!(packmap_idx > 1, "PACKMAP must have a preceding count push");
        assert_eq!(
            code[packmap_idx - 2],
            Bytecode::PU8 as u8,
            "count should be pushed as PU8"
        );
        assert_eq!(
            code[packmap_idx - 1],
            4u8,
            "PACKMAP count must be total k/v items"
        );
    }

    #[test]
    fn syntax_rejects_unknown_types_and_suffixes() {
        let err_as = parse("return 1 as u256").unwrap_err().to_string();
        assert!(
            err_as.contains("<as> expression format invalid"),
            "{}",
            err_as
        );
        let err_is = parse("return 1 is uint").unwrap_err().to_string();
        assert!(
            err_is.contains("<is> expression format invalid"),
            "{}",
            err_is
        );
        let err = parse("return 1u256").unwrap_err().to_string();
        assert!(err.contains("integer type parse failed"), "{}", err);
    }
}
