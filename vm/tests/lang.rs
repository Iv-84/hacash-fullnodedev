// use sys::*;
// use vm::IRNode;
// use vm::rt::BytecodePrint;
// use vm::ir::IRCodePrint;
// use vm::lang::{Tokenizer, Syntax};

use vm::IRNode;
use vm::PrintOption;
use vm::ir::*;
use vm::lang::*;
use vm::rt::*;

mod common;

#[test]
fn t1() {
    // lang_to_bytecode("return 0").unwrap();

    let payable_hac_fitsh = r##"
        // var addr = 1
        self.deposit(1)
        end
    "##;

    let ircodes = lang_to_ircode(&payable_hac_fitsh).unwrap();

    println!("\n{}\n", ircodes.bytecode_print(false).unwrap());

    let bytecodes = convert_ir_to_bytecode(&ircodes).unwrap();

    println!("\n{}\n", bytecodes.bytecode_print(false).unwrap());
}

#[test]
fn multi_return_ir_func_is_rejected_without_panic() {
    let script = r##"
        swap(1, 2)
    "##;

    let res = std::panic::catch_unwind(|| lang_to_irnode(script));
    assert!(res.is_ok(), "compiler panicked for multi-return ir func");
    assert!(res.unwrap().is_err());
}

#[test]
fn bind_slot_and_cache_print() {
    let script = r##"
        var x $0 = 1
        bind foo = $0
        bind bar = foo
        print bar
        print bar
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircodes).unwrap();
    assert!(printed.matches("print $0").count() >= 2);
}

#[test]
fn duplicate_lib_index_binding_is_rejected() {
    let script = r##"
        lib A = 1
        lib B = 1
        return A.0xabcdef01()
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(
        err.contains("lib index 1") && err.contains("binding already exists"),
        "err: {}",
        err
    );
}

#[test]
fn callextview_and_local_call_print() {
    let script = r##"
        ext(2):0xabcdef01()
        this.0x00ab4130()
        self:0x00ab4131()
        self::0x00ab4132()
        calluseview 3::0xdeadbeef()
        ext(4)::0xfacecafe()
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircodes).unwrap();
    assert!(printed.contains("call view ext(2).0xabcdef01("));
    assert!(printed.contains("call edit this.0x00ab4130("));
    assert!(printed.contains("call view self.0x00ab4131("));
    assert!(printed.contains("call pure self.0x00ab4132("));
    assert!(printed.contains("call view use(3).0xdeadbeef("));
    assert!(printed.contains("call pure use(4).0xfacecafe("));
}

#[test]
fn callthis_callself_callsuper_print_and_roundtrip() {
    let script = r##"
        this.0x00ab4130(1)
        self.0x00ab4130(2)
        super.0x00ab4130(3)
    "##;
    let (ircd, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&ircd, Some(&smap)).unwrap();
    assert!(printed.contains("this.0x00ab4130("));
    assert!(printed.contains("self.0x00ab4130("));
    assert!(printed.contains("super.0x00ab4130("));

    let sugar = r##"
        this.foo(1)
        self.foo(2)
        super.foo(3)
    "##;
    let (ircd2, smap2) = lang_to_ircode_with_sourcemap(sugar).unwrap();
    let printed2 = format_ircode_to_lang(&ircd2, Some(&smap2)).unwrap();
    assert!(printed2.contains("this.foo("));
    assert!(printed2.contains("self.foo("));
    assert!(printed2.contains("super.foo("));
}

#[test]
fn self_view_and_self_pure_short_syntax_print_when_names_exist() {
    let script = r##"
        return self:view_ok() + self::pure_ok()
    "##;
    let (ircd, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&ircd, Some(&smap)).unwrap();
    assert!(printed.contains("call view self.view_ok("));
    assert!(printed.contains("call pure self.pure_ok("));
}

#[test]
fn codecall_short_syntax_print_when_names_exist() {
    let script = r##"
        lib C = 0
        codecall C.probe
    "##;
    let (ircd, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&ircd, Some(&smap)).unwrap();
    assert!(printed.contains("codecall "));
    assert!(printed.contains("ext(0).") || printed.contains("lib(0).") || printed.contains("0x"));
}

#[test]
fn callext_keyword_print() {
    let script = r##"
        ext(1).0xabcdef01()
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircodes).unwrap();
    assert!(printed.contains("ext(1).0xabcdef01("));
}

#[test]
fn decompile_system_calls_preserve_default_empty_arg_unless_opted_in() {
    let script = r##"
        print context_address()
        print block_height()
    "##;

    let block = lang_to_irnode(script).unwrap();
    let plain = PrintOption::new("  ", 0);
    let printed_plain = Formater::new(&plain).print(&block);
    assert!(printed_plain.contains("context_address()"));
    assert!(printed_plain.contains("block_height()"));

    let mut pretty = PrintOption::new("  ", 0);
    pretty.hide_default_call_argv = true;
    let printed_pretty = Formater::new(&pretty).print(&block);
    assert!(printed_pretty.contains("context_address()"));
    assert!(printed_pretty.contains("block_height()"));
}

#[test]
fn decompile_contract_calls_hide_default_nil_only_when_opted_in() {
    let script = r##"
        ext(1).0xabcdef01()
    "##;
    let block = lang_to_irnode(script).unwrap();

    let plain = PrintOption::new("  ", 0);
    let printed_plain = Formater::new(&plain).print(&block);
    assert!(printed_plain.contains("ext(1).0xabcdef01(nil)"));

    let mut pretty = PrintOption::new("  ", 0);
    pretty.hide_default_call_argv = true;
    let printed_pretty = Formater::new(&pretty).print(&block);
    assert!(printed_pretty.contains("ext(1).0xabcdef01()"));
}

#[test]
fn args_constructor_roundtrips_in_value_context() {
    let script = r##"
        return tuple(7, map { "kind": "hnft" })
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(printed.contains("tuple(7, map"));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn args_to_list_roundtrips_as_function_call() {
    let script = r##"
        return tuple_to_list(tuple(1, 2))
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(printed.contains("tuple_to_list(tuple(1, 2))"));
    assert!(!printed.contains(" as list"));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn pack_args_surface_syntax_is_rejected() {
    let err = lang_to_ircode("return pack_args(1, 2)").unwrap_err();
    assert!(err.contains("pack_args"), "unexpected error: {err}");
}

#[test]
fn ntenv_placeholder_surface_syntax_is_rejected() {
    let err = lang_to_ircode(r#"return context_address("")"#).unwrap_err();
    assert!(
        err.contains("takes no arguments"),
        "unexpected error: {err}"
    );
}

#[test]
fn zero_arg_action_placeholder_surface_syntax_is_rejected() {
    let err = lang_to_ircode(r#"return block_height("")"#).unwrap_err();
    assert!(
        err.contains("argv length must 0 but got 1"),
        "unexpected error: {err}"
    );
}

#[test]
fn not_operator_binds_tighter_than_binary_ops() {
    let script = r##"
        print !1 == 0
        print !1 && 0 == 0
        print !1 is nil
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn not_operator_parenthesizes_lower_precedence_operands_on_roundtrip() {
    // The parser uses IRNodeWrapOne to record explicit parentheses, but WrapOne is not
    // serialized into ircode. Decompilation must still print parentheses when required
    // by precedence, otherwise recompilation changes semantics.
    let script = r##"
        print !(1 == 1 && 2 == 2)
        print !(1 + 2 * 3 == 7)
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn pow_left_nested_requires_parentheses_on_roundtrip() {
    // `**` is right-associative in the parser, so a left-nested IR tree must be
    // printed with parentheses to preserve semantics.
    let script = r##"
        print (2 ** 3) ** 2
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn inline_block_expression_with_statements_roundtrips() {
    // Block expressions (IRBLOCKR) may contain multiple statements and a final value.
    // Decompilation must keep braces/newlines so the recompiled semantics match.
    let script = r##"
        var stmt = {
            print 5
            0
        }
        print stmt
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn call_args_preserve_block_expression_argument_boundaries() {
    // Block expressions may include newlines; decompilation must not split one argument
    // into many by line-joining.
    let script = r##"
        print sha3({
            print 1
            "abc"
        })

        transfer_hac_to(emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS, {
            print 2
            1
        })
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn const_defs_not_injected_into_inline_expressions() {
    // Source-map const defs are emitted as a top-level prelude.
    // Inline printing (used for call args / expression formatting) must not
    // inject `const ...` lines into expression contexts.
    let script = r##"
        const ONE = 1

        // The compiler replaces `ONE` with literal `1` in IR;
        // decompilation should restore `ONE` and emit `const ONE = 1` once at top-level.
        print sha3(ONE + 2)
    "##;

    let (block, source_map) = lang_to_irnode_with_sourcemap(script).unwrap();
    let mut opt = PrintOption::new("    ", 0).with_source_map(&source_map);
    opt.recover_literals = true;
    let printed = Formater::new(&opt).print(&block);

    assert!(printed.contains("const ONE = 1"));
    assert_eq!(printed.matches("const ONE = 1").count(), 1);
    assert!(printed.contains("sha3(ONE + 2)"));
    assert!(!printed.contains("sha3(const"));

    let const_pos = printed.find("const ONE = 1").unwrap();
    let block_pos = printed.find('{').unwrap();
    assert!(const_pos < block_pos);
}

#[test]
fn inline_if_expression_argument_with_multiline_blocks_roundtrips() {
    // `if` expressions may print across multiple lines, but when used as a call argument
    // they flow through `print_inline()` which may flatten newlines.
    // This must remain parseable and preserve semantics.
    let script = r##"
        print sha3(if true {
            print 1
            "a"
        } else {
            print 2
            "b"
        })
    "##;

    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn inline_if_expression_argument_with_var_and_final_value_roundtrips() {
    // Stress `print_inline()` newline flattening: block bodies contain a declaration and a final value.
    // If newlines are flattened incorrectly, parsing can become ambiguous or fail.
    let script = r##"
        print sha3(if true {
            var x = 1
            x
        } else {
            2
        })
    "##;

    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn inline_multiline_map_expression_argument_roundtrips() {
    // `map { ... }` often prints across multiple lines; when used as a call argument it
    // flows through `print_inline()` and may have newlines flattened.
    let script = r##"
        print sha3(map {
            "a": {
                print 1
                "x"
            }
            "b": 2
        })
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn inline_multiline_list_expression_argument_roundtrips() {
    // Same as map: ensure list printing remains parseable when flattened for inline args.
    let script = r##"
        print sha3(list {
            {
                print 1
                "x"
            }
            2
        })
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn inline_if_expression_as_list_element_roundtrips() {
    // Nested inline contexts: list literal contains an `if` expression whose branches are blocks.
    // This exercises: print_inline(if) -> newline flattening, inside list { ... } which itself
    // becomes an inline call argument.
    let script = r##"
        print sha3(list {
            if true {
                var x = 1
                x
            } else {
                2
            }
        })
    "##;

    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn ir_func_call_arguments_parse_in_value_context() {
    // IR function arguments must parse in value context so `if ... else ...` has a return value.
    let script = r##"
        bind numbers = [1]
        append(numbers, if true { 2 } else { 3 })
        print numbers
    "##;

    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn log_arguments_parse_in_value_context() {
    // `log(...)` is an argument-taking instruction. Its arguments are value expressions.
    // Ensure constructs like `if ... else ...` are parsed as expression-form `IRIFR`.
    let script = r##"
        log(if true { 1 } else { 2 }, 0)
    "##;

    let ircodes = lang_to_ircode(script).unwrap();
    assert!(
        ircodes.iter().any(|b| *b == Bytecode::IRIFR as u8),
        "expected IRIFR for `if` used as log argument"
    );
}

#[test]
fn var_put_print_roundtrip() {
    let script = r##"
        var total $0 = 1
        total = total + 1
        var other $1 = total
        other = $1
    "##;
    let (block, source_map) = lang_to_irnode_with_sourcemap(script).unwrap();
    let opt = PrintOption::new("    ", 0).with_source_map(&source_map);
    let printed = Formater::new(&opt).print(&block);
    assert!(printed.contains("var total $0 = 1"));
    assert!(printed.contains("total = "));
    assert!(printed.contains("var other $1 = total"));
    assert!(printed.contains("other = other"));
}

#[test]
fn var_cannot_rebind_param_slot() {
    let script = r##"
        param { addr, amt }
        var zhu $1 = amt
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("slot 1 already bound"));
}

#[test]
fn let_cannot_reassign() {
    let script = r##"
        let x = 1
        x = 2
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("cannot assign to immutable symbol 'x'"));
}

#[test]
fn var_initializer_must_be_value_expression() {
    // `bind` is a declaration/statement (returns no value). Allowing it as a var initializer
    // would compile into `PUT` without a stack value, causing stack/semantic issues.
    let script = r##"
        var x = bind y = 1
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("initializer") || err.contains("return"));
}

#[test]
fn return_argument_must_be_value_expression() {
    let script = r##"
        return end
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("return") || err.contains("return value"));
}

#[test]
fn assert_argument_must_be_value_expression() {
    let script = r##"
        assert end
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("assert") || err.contains("return value"));
}

#[test]
fn throw_argument_must_be_value_expression() {
    let script = r##"
        throw end
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("throw") || err.contains("return value"));
}

#[test]
fn item_get_index_must_be_value_expression() {
    let script = r##"
        var arr = list { 1 }
        print arr[end]
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("return value") || err.contains("not have return value"));
}

#[test]
fn else_if_statement_allows_statement_branches() {
    // In statement context, else-if chains should parse as IRIF (not IRIFR),
    // so branch blocks may be statement-only.
    let script = r##"
        if true {
            print 1
        } else if true {
            print 2
        } else {
            print 3
        }
        end
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn else_if_expression_still_requires_value_branches() {
    // In expression context, every branch in the else-if chain must return a value.
    let script = r##"
        print if true {
            1
        } else if true {
            print 2
        } else {
            3
        }
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("block expression") || err.contains("return value"));
}

#[test]
fn if_expression_requires_else_branch_at_parse_time() {
    let script = r##"
        print if true { 1 }
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("if expression must have else branch"));
}

#[test]
fn item_get_if_expression_index_roundtrips() {
    let script = r##"
        var arr = [10, 20]
        print arr[if true {
            0
        } else {
            1
        }]
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn item_get_receiver_list_literal_roundtrips() {
    // Indexing should work on any value expression, including list literals.
    let script = r##"
        var x = [1, 2][0]
        print x
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn item_get_receiver_call_result_roundtrips() {
    // Indexing should work on call results (receiver isn't an identifier).
    let script = r##"
        var b = sha3("abc")[0]
        print b
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn item_get_receiver_binary_expression_preserves_precedence_roundtrips() {
    // Formatter must emit parentheses: `(a + b)[0]` must not decompile as `a + b[0]`.
    let script = r##"
        var a = [1]
        var b = [2]
        var x = (a + b)[0]
        print x
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn item_get_receiver_bind_symbol_roundtrips() {
    // `bind` symbols are value expressions but not slots; indexing must still work.
    let script = r##"
        bind arr = [7, 9]
        print arr[1]
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn map_value_if_expression_roundtrips() {
    let script = r##"
        var flag = true
        let obj = map {
            "v": if flag {
                1
            } else {
                2
            }
        }
        print obj
    "##;
    let _ = common::checked_compile_fitsh_to_ir(script);
}

#[test]
fn var_without_reassign_prints_as_let() {
    let script = r##"
        var x $0 = 1
        print x
    "##;
    let (block, source_map) = lang_to_irnode_with_sourcemap(script).unwrap();
    let printed = irnode_to_lang_with_sourcemap(block, &source_map).unwrap();
    assert!(printed.contains("let x $0 ="));
}

#[test]
fn bind_var_interleave_print() {
    let script = r##"
        var x $0 = 10
        bind aux = x
        var y = aux
        bind cache = y
        print x
        print cache
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircodes).unwrap();
    assert!(printed.contains("$0 = 10"));
    assert!(printed.contains("$1 = $0"));
    assert!(printed.matches("print $0").count() >= 1);
    assert!(printed.matches("print $1").count() >= 1);
}

#[test]
fn print_decomp_bind_alias_clones_expression() {
    let script = r##"
        bind base = {
            if true {
                { 1 }
            } else {
                { 2 }
            }
        }
        bind alias = base
        print base
        print alias
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircode).unwrap();
    println!("{}", printed);
    assert!(printed.matches("print ").count() >= 2);
    let cond_count = printed.matches("if 1 {").count() + printed.matches("if true {").count();
    assert!(cond_count >= 2, "unexpected decompiled text:\n{}", printed);
    assert!(printed.contains("} else {"));
}

#[test]
fn block_and_if_expression_use_expr_opcodes() {
    let script = r##"
        print {
            if false {
                1
            } else {
                2
            }
        }
        // Ensure we still emit a real IRBLOCKR that cannot be simplified away.
        print {
            print 9
            10
        }
        print if true { 3 } else { 4 }
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRBLOCKR as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRIFR as u8));
}

#[test]
fn block_and_if_statement_use_stmt_opcodes() {
    let script = r##"
        if true {
            print 1
        } else {
            print 2
        }
        {
            print 3
        }
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRIF as u8));
    assert!(!ircodes.iter().any(|b| *b == Bytecode::IRBLOCKR as u8));
    assert!(!ircodes.iter().any(|b| *b == Bytecode::IRIFR as u8));
}

#[test]
fn if_expression_single_stmt_branches_elide_irblockr_in_ircode() {
    // Deprecated: we require ircode byte-for-byte stability, so single-statement
    // block wrappers must remain representable. Keep a sanity check that the
    // expression opcode form is used.
    let script = r##"
        print if true { 3 } else { 4 }
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRIFR as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRBLOCKR as u8));
}

#[test]
fn if_statement_and_while_single_stmt_bodies_elide_irblock_in_ircode() {
    // Deprecated: ircode stability requires preserving IRBLOCK wrappers.
    let script = r##"
        if true { print 1 } else { print 2 }
        while true { print 3 }
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRIF as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRWHILE as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRBLOCK as u8));
}

#[test]
fn while_break_continue_roundtrip_and_codegen() {
    let script = r##"
        var i = 0
        while i < 10 {
            i += 1
            if i == 3 {
                continue
            }
            if i == 8 {
                break
            }
        }
        return i
    "##;
    let ircodes = common::checked_compile_fitsh_to_ir(script);
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRWHILE as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRBREAK as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRCONTINUE as u8));

    let bytecodes = convert_ir_to_bytecode(&ircodes).unwrap();
    assert!(!bytecodes.iter().any(|b| *b == Bytecode::IRBREAK as u8));
    assert!(!bytecodes.iter().any(|b| *b == Bytecode::IRCONTINUE as u8));
    assert!(verify_bytecodes(&bytecodes).is_ok());

    let printed = ircode_to_lang(&ircodes).unwrap();
    assert!(printed.contains("continue"));
    assert!(printed.contains("break"));
}

#[test]
fn while_nested_break_continue_codegen_verifies() {
    let script = r##"
        var i = 0
        var j = 0
        while i < 3 {
            i += 1
            j = 0
            while j < 5 {
                j += 1
                if j == 2 {
                    continue
                }
                if j == 4 {
                    break
                }
            }
        }
        return i
    "##;
    let ircodes = common::checked_compile_fitsh_to_ir(script);
    let bytecodes = convert_ir_to_bytecode(&ircodes).unwrap();
    verify_bytecodes(&bytecodes).unwrap();
}

#[test]
fn nested_expression_contexts_emit_expr_opcodes() {
    let script = r##"
        print {
            if true {
                bind local_inner = if false {
                    { if false { 10 } else { 11 } }
                } else {
                    { { 12 } }
                }
                local_inner
            } else {
                {
                    bind deep = { if true { { 13 } } else { { 14 } } }
                    deep
                }
            }
        }
        print { { if true { { 15 } } else { { 16 } } } }
        print if false { { 17 } } else { { 18 } }
        print if true { { 19 } } else { { 20 } }
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    let blockr = ircodes
        .iter()
        .filter(|b| **b == Bytecode::IRBLOCKR as u8)
        .count();
    let ifr = ircodes
        .iter()
        .filter(|b| **b == Bytecode::IRIFR as u8)
        .count();
    assert!(blockr >= 1);
    assert!(ifr >= 4);
}

#[test]
fn reject_break_outside_while() {
    let script = r##"
        break
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("break can only be used inside while loop"));
}

#[test]
fn reject_continue_outside_while() {
    let script = r##"
        continue
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("continue can only be used inside while loop"));
}

#[test]
fn reject_break_continue_in_expression_context() {
    let script1 = r##"
        print break
    "##;
    let err1 = lang_to_ircode(script1).unwrap_err();
    assert!(err1.contains("break statement cannot be used as expression"));

    let script2 = r##"
        print continue
    "##;
    let err2 = lang_to_ircode(script2).unwrap_err();
    assert!(err2.contains("continue statement cannot be used as expression"));
}

#[test]
fn trim_param_unpack_false_does_not_bind_fake_param_names() {
    let script = r##"
        unpack(roll_0(), 0)
        print 1
    "##;
    let block = lang_to_irnode(script).unwrap();
    let mut smap = SourceMap::default();
    smap.register_param_names(vec!["a".to_string(), "b".to_string()])
        .unwrap();
    let mut opt = PrintOption::new("  ", 0).with_source_map(&smap);
    opt.trim_param_unpack = false;
    let printed = Formater::new(&opt).print(&block);
    assert!(!printed.contains("var a $0"), "printed: {}", printed);
    assert!(!printed.contains("var b $1"), "printed: {}", printed);
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(lang_to_ircode(script).unwrap(), reparsed);
}

#[test]
fn reusing_same_formater_instance_does_not_leak_runtime_state() {
    let script = r##"
        const ONE = 1
        var sum = ONE + 1
        print sum
    "##;
    let (block, smap) = lang_to_irnode_with_sourcemap(script).unwrap();
    let mut opt = PrintOption::new("  ", 0).with_source_map(&smap);
    opt.recover_literals = true;
    let formatter = Formater::new(&opt);
    let first = formatter.print(&block);
    let second = formatter.print(&block);
    assert_eq!(first, second);
    assert!(first.contains("const ONE = 1"), "first: {}", first);
    assert!(second.contains("const ONE = 1"), "second: {}", second);
}

#[test]
fn malformed_pbuf_try_print_returns_explicit_error() {
    let malformed = IRNodeParams {
        hrtv: true,
        inst: Bytecode::PBUF,
        para: vec![3, 0xaa],
    };
    let printed = Formater::new(&PrintOption::new("  ", 0)).print(&malformed);
    assert!(printed.contains("0x03aa"), "printed: {}", printed);
}

#[test]
fn print_options_on_off_preserve_ircode_bytes() {
    // This script intentionally hits multiple formatter options:
    // - has a cast opcode (so `recover_literals` must not drop it)
    // - has packed-call nil placeholder handling (so `hide_default_call_argv` must be lossless)
    // - has list literal (so `flatten_array_list` must be lossless)
    // - has if/while blocks (so bracing must preserve container opcodes)
    let script = r##"
        param { amt }
        print amt as u8
        ext(1).0xabcdef01()
        print context_address()
        print [1, 2]
        if true { print 3 } else { print 4 }
        while false { print 5 }
    "##;

    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let mut seek = 0;
    let block = parse_ir_block(&ircode, &mut seek).unwrap();
    assert_eq!(seek, ircode.len());

    // All options off.
    let plain = PrintOption::new("  ", 0).with_source_map(&smap);
    let plain_text = Formater::new(&plain).print(&block);
    let plain_ir = lang_to_ircode(&plain_text).unwrap();
    assert_eq!(ircode, plain_ir);

    // All options on (mirrors `format_ircode_to_lang`).
    let mut pretty = PrintOption::new("  ", 0).with_source_map(&smap);
    pretty.trim_root_block = true;
    pretty.trim_head_alloc = true;
    pretty.trim_param_unpack = true;
    pretty.hide_default_call_argv = true;
    pretty.call_short_syntax = true;
    pretty.flatten_call_list = true;
    pretty.flatten_array_list = true;
    pretty.flatten_syscall_cat = true;
    pretty.recover_literals = true;

    let pretty_text = Formater::new(&pretty).print(&block);
    let pretty_ir = lang_to_ircode(&pretty_text)
        .map_err(|e| {
            format!(
                "{}\n---- pretty_text ----\n{}\n---------------------\n",
                e, pretty_text
            )
        })
        .unwrap();
    assert_eq!(ircode, pretty_ir);
}

#[test]
fn flatten_call_list_preserves_container_values_in_call_args() {
    let cases = [
        ("return ext(1).0xabcdef01([1])", "[1]"),
        ("return ext(1).0xabcdef01([1, 2])", "[1, 2]"),
        ("return ext(1).0xabcdef01(tuple(1))", "tuple(1)"),
    ];

    for (script, expect) in cases {
        let ircode = lang_to_ircode(script).unwrap();
        let pretty_text = format_ircode_to_lang(&ircode, None).unwrap();
        let pretty_ir = lang_to_ircode(&pretty_text)
            .map_err(|e| {
                format!(
                    "{}\n---- pretty_text ----\n{}\n---------------------\n",
                    e, pretty_text
                )
            })
            .unwrap();

        assert!(
            pretty_text.contains(expect),
            "expected '{}' in decompiled text, got: {}",
            expect,
            pretty_text
        );
        assert_eq!(
            ircode, pretty_ir,
            "roundtrip mismatch for script: {}\nprinted: {}",
            script, pretty_text
        );
    }
}

#[test]
fn all_print_options_disabled_preserve_ircode_bytes() {
    let script = r##"
        param { amt }
        lib C = 1
        print 1 as u64
        ext(1).0xabcdef01()
        print [1, 2]
        if true { print 3 } else { print 4 }
        while false { print 5 }
        codecall C.probe
    "##;

    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();

    for map_enabled in [false, true] {
        let mut opt = PrintOption::new("  ", 0);
        if map_enabled {
            opt.map = Some(&smap);
        }
        opt.emit_lib_prelude = false;
        opt.trim_root_block = false;
        opt.trim_head_alloc = false;
        opt.trim_param_unpack = false;
        opt.hide_default_call_argv = false;
        opt.call_short_syntax = false;
        opt.flatten_call_list = false;
        opt.flatten_array_list = false;
        opt.flatten_syscall_cat = false;
        opt.recover_literals = false;
        opt.simplify_numeric_as_suffix = false;

        let text = Formater::new(&opt).print(&lang_to_irnode(script).unwrap());
        let reparsed = lang_to_ircode(&text)
            .map_err(|e| {
                format!(
                    "{}\n---- all-off printed (map_enabled={}) ----\n{}\n---------------------\n",
                    e, map_enabled, text
                )
            })
            .unwrap();
        assert_eq!(
            ircode, reparsed,
            "all-off roundtrip mismatch (map_enabled={})\n{}",
            map_enabled, text
        );
    }
}

#[test]
fn print_option_each_toggle_and_sourcemap_on_off_preserve_ircode_bytes() {
    // This script includes a param-unpack and a local use, so sourcemap-less decompilation
    // must still emit a compilable `param { ... }` and preserve byte-for-byte ircode.
    let script = r##"
        param { amt }
        print amt as u8
        ext(1).0xabcdef01()
        print context_address()
        print [1, 2]
        if true { print 3 } else { print 4 }
        while false { print 5 }
    "##;

    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let mut seek = 0;
    let block = parse_ir_block(&ircode, &mut seek).unwrap();
    assert_eq!(seek, ircode.len());

    #[derive(Clone, Copy, Debug)]
    enum OptKey {
        EmitLibPrelude,
        TrimRootBlock,
        TrimHeadAlloc,
        TrimParamUnpack,
        HideDefaultCallArgv,
        CallShortSyntax,
        FlattenCallList,
        FlattenArrayList,
        FlattenSyscallCat,
        RecoverLiterals,
        SimplifyNumericAsSuffix,
    }

    fn set_opt(opt: &mut PrintOption, key: OptKey, val: bool) {
        match key {
            OptKey::EmitLibPrelude => opt.emit_lib_prelude = val,
            OptKey::TrimRootBlock => opt.trim_root_block = val,
            OptKey::TrimHeadAlloc => opt.trim_head_alloc = val,
            OptKey::TrimParamUnpack => opt.trim_param_unpack = val,
            OptKey::HideDefaultCallArgv => opt.hide_default_call_argv = val,
            OptKey::CallShortSyntax => opt.call_short_syntax = val,
            OptKey::FlattenCallList => opt.flatten_call_list = val,
            OptKey::FlattenArrayList => opt.flatten_array_list = val,
            OptKey::FlattenSyscallCat => opt.flatten_syscall_cat = val,
            OptKey::RecoverLiterals => opt.recover_literals = val,
            OptKey::SimplifyNumericAsSuffix => opt.simplify_numeric_as_suffix = val,
        }
    }

    let keys = [
        OptKey::EmitLibPrelude,
        OptKey::TrimRootBlock,
        OptKey::TrimHeadAlloc,
        OptKey::TrimParamUnpack,
        OptKey::HideDefaultCallArgv,
        OptKey::CallShortSyntax,
        OptKey::FlattenCallList,
        OptKey::FlattenArrayList,
        OptKey::FlattenSyscallCat,
        OptKey::RecoverLiterals,
        OptKey::SimplifyNumericAsSuffix,
    ];

    for map_enabled in [false, true] {
        for key in keys {
            for val in [false, true] {
                let mut opt = PrintOption::new("  ", 0);
                if map_enabled {
                    opt.map = Some(&smap);
                }
                set_opt(&mut opt, key, val);
                let text = Formater::new(&opt).print(&block);
                let ir2 = lang_to_ircode(&text)
                    .map_err(|e| {
                        format!(
                            "{}\n---- printed (map_enabled={}, opt={:?}={}) ----\n{}\n---------------------\n",
                            e, map_enabled, key, val, text
                        )
                    })
                    .unwrap();
                assert_eq!(
                    ircode, ir2,
                    "ircode mismatch (map_enabled={}, opt={:?}={})\n{}",
                    map_enabled, key, val, text
                );
            }
        }
    }
}

#[test]
fn var_rhs_block_expression_emits_expr_opcodes() {
    let script = r##"
        var holder = {
            if true {
                bind local_inner = if false {
                    {
                        if true { 1 } else { 2 }
                    }
                } else {
                    { 3 }
                }
                local_inner
            } else {
                { 4 }
            }
        }
        var stmt = {
            print 5
            0
        }
    "##;
    let ircodes = lang_to_ircode(script).unwrap();
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRBLOCKR as u8));
    assert!(ircodes.iter().any(|b| *b == Bytecode::IRIFR as u8));
}

fn check_fitsh_ir_roundtrip(script: &str, keywords: &[&str]) {
    let _ = lang_to_irnode(script).unwrap();
    let ircode_bytes = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircode_bytes).unwrap();
    for &kw in keywords {
        assert!(
            printed.contains(kw),
            "Fitsh decompile output missing '{}'\n{}",
            kw,
            printed
        );
    }
    let mut idx = 0;
    let _ = parse_ir_block(&ircode_bytes, &mut idx).unwrap();
    assert_eq!(idx, ircode_bytes.len());
}

#[test]
fn fitsh_ir_roundtrip_suite() {
    let scripts: [(&str, &[&str]); 4] = [
        (
            r##"
                lib HacSwap = 1: VFE6Zu4Wwee1vjEkQLxgVbv3c6Ju9iTaa
                param { amt }
                var counter = amt
                while counter > 0 {
                    counter -= 1
                }
                bind sum = {
                    var builder = counter
                    builder += amt
                    builder
                }
                if sum > 0 {
                    ext(1):0xabcdef01(sum)
                } else {
                    ext(1)::0xdeadbeef(sum, 0)
                }
                this.0x00ab4130(sum)
            "##,
            &[
                "while",
                "call view ext(1).0xabcdef01",
                "call pure use(1).0xdeadbeef",
                "call edit this.0x00ab4130",
                "if",
            ],
        ),
        (
            r##"
                bind numbers = [1, 2, 3]
                bind info = map {
                    "numbers": numbers
                    "total": 3
                }
                append(numbers, 4)
                print numbers
                print info
            "##,
            &["map", "append", "print", "numbers", "total"],
        ),
        (
            r##"
                var x $0 = 42
                bind aux = {
                    bind local_inner = x
                    local_inner + 1
                }
                var y = aux
                bind result = {
                    var staged = y
                    staged * x
                }
                print result
            "##,
            &["$0 = 42", "$1 =", "print ", "$2 * $0"],
        ),
        (
            r##"
                bind dv = scaled_div_ceil(abs_diff(10001, 10000), 3, 10000)
                bind dvf = scaled_div_floor(abs_diff(10001, 10000), 3, 10000)
                bind ok = within_bps(10001, 10000, 1, 3)
                bind px = lerp_floor(101, 100, 1, 2)
                bind pxu = lerp_ceil(0, 1, 1, 2)
                bind absok = abs_diff_lte(105, 100, 5)
                print dv
                print dvf
                print ok
                print px
                print pxu
                print absok
            "##,
            &[
                "scaled_div_ceil",
                "scaled_div_floor",
                "within_bps",
                "lerp_floor",
                "lerp_ceil",
                "abs_diff_lte",
            ],
        ),
    ];

    for (script, keywords) in scripts {
        check_fitsh_ir_roundtrip(script, keywords);
    }
}

#[test]
fn direct_muldiv_decompile_roundtrip_keeps_opcode_identity() {
    let ircode = vec![
        Bytecode::RET as u8,
        Bytecode::MULDIV as u8,
        Bytecode::P1 as u8,
        Bytecode::P2 as u8,
        Bytecode::P3 as u8,
    ];
    let printed = ircode_to_lang(&ircode).unwrap();
    assert!(
        printed.contains("mul_div"),
        "expected direct helper mul_div in decompile output, got: {}",
        printed
    );
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(
        ircode, reparsed,
        "direct mul_div roundtrip changed opcode identity\n{}",
        printed
    );
}

#[test]
fn all_fin_source_names_decompile_recompile_roundtrip() {
    for spec in fin_specs() {
        let args = (1..=spec.argc().unwrap())
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let script = format!("return {}({})", spec.name, args);
        let ircode = lang_to_ircode(&script).unwrap();
        let printed = ircode_to_lang(&ircode).unwrap();
        let reparsed = lang_to_ircode(&printed).unwrap();
        assert_eq!(
            ircode, reparsed,
            "FIN source call {} roundtrip changed IR\n{}",
            spec.name, printed
        );
    }
}

#[test]
fn all_fin_runtime_specs_decompile_recompile_roundtrip() {
    let families = [
        Bytecode::FIN2,
        Bytecode::FIN3,
        Bytecode::FIN4,
        Bytecode::FINP3,
        Bytecode::FINP4,
        Bytecode::FINPOW3,
    ];
    let args = [
        Bytecode::P0 as u8,
        Bytecode::P1 as u8,
        Bytecode::P2 as u8,
        Bytecode::P3 as u8,
    ];

    for family in families {
        for fin_id in 0u8..=u8::MAX {
            let Ok(spec) = fin_spec_lookup(family, fin_id) else {
                continue;
            };
            let mut ircode = vec![Bytecode::RET as u8, family as u8, fin_id];
            ircode.extend_from_slice(&args[..spec.argc().unwrap() as usize]);

            let printed = ircode_to_lang(&ircode).unwrap();
            assert!(
                printed.contains(spec.name),
                "expected FIN name {} in decompile output, got: {}",
                spec.name,
                printed
            );
            let reparsed = lang_to_ircode(&printed).unwrap();
            assert_eq!(
                ircode, reparsed,
                "FIN runtime spec {:?}/{} roundtrip changed IR\n{}",
                family, fin_id, printed
            );
        }
    }
}

#[test]
fn trim_param_unpack_without_sourcemap_keeps_multi_param_unpack() {
    let script = r##"
        param { a, b }
        b = 3
        print a
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let printed = format_ircode_to_lang(&ircode, None).unwrap();
    assert!(printed.contains("unpack(roll_0(), 0)"));
    assert!(!printed.contains("param {"));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn explicit_unpack_roll0_p0_is_not_rewritten_by_param_names_alone() {
    let script = r##"
        unpack(roll_0(), 0)
        print 1
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let mut smap = SourceMap::default();
    smap.register_param_names(vec!["a".to_string(), "b".to_string()])
        .unwrap();
    let printed = format_ircode_to_lang(&ircode, Some(&smap)).unwrap();
    assert!(printed.contains("unpack(roll_0(), 0)"));
    assert!(!printed.contains("param { a, b }"));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn single_param_block_prints_from_ir_roundtrip() {
    let script = r##"
        param { amt }
        print amt
    "##;
    let (block, source_map) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&block, Some(&source_map)).unwrap();
    assert!(printed.contains("param { amt }"));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(block, reparsed);
}

#[test]
fn explicit_put_roll0_is_not_rewritten_by_param_names_alone() {
    let script = r##"
        var x $0 = roll_0()
        print x
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let mut smap = SourceMap::default();
    smap.register_param_names(vec!["amt".to_string()]).unwrap();
    let printed = format_ircode_to_lang(&ircode, Some(&smap)).unwrap();
    assert!(!printed.contains("param { amt }"));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn statement_position_block_expr_keeps_irblockr_via_parentheses() {
    let mut expr = IRNodeArray::with_opcode(Bytecode::IRBLOCKR);
    expr.push(push_inst(Bytecode::P1));
    let mut root = IRNodeArray::with_opcode(Bytecode::IRBLOCK);
    root.push(Box::new(expr));

    let ircode = drop_irblock_wrap(root.serialize()).unwrap();
    let printed = format_ircode_to_lang(&ircode, None).unwrap();
    assert!(printed.contains("({"), "printed: {}", printed);
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn statement_position_if_expr_keeps_irifr_via_parentheses() {
    let mut then_block = IRNodeArray::with_opcode(Bytecode::IRBLOCKR);
    then_block.push(push_inst(Bytecode::P1));
    let mut else_block = IRNodeArray::with_opcode(Bytecode::IRBLOCKR);
    else_block.push(push_inst(Bytecode::P2));
    let ifexpr = IRNodeTriple {
        hrtv: true,
        inst: Bytecode::IRIFR,
        subx: push_inst(Bytecode::PTRUE),
        suby: Box::new(then_block),
        subz: Box::new(else_block),
    };
    let mut root = IRNodeArray::with_opcode(Bytecode::IRBLOCK);
    root.push(Box::new(ifexpr));

    let ircode = drop_irblock_wrap(root.serialize()).unwrap();
    let printed = format_ircode_to_lang(&ircode, None).unwrap();
    assert!(printed.contains("(if true"), "printed: {}", printed);
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn param_block_prints_from_ir_roundtrip() {
    let script = r##"
        param { addr, sat }
        print addr
        print sat
    "##;
    let (block, source_map) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&block, Some(&source_map)).unwrap();
    assert!(printed.contains("param { addr, sat }"));
}

#[test]
fn reject_unclosed_log_argument_delimiter() {
    let script = r##"
        log(1 2
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("log argv number invalid"));
}

#[test]
fn reject_param_block_non_identifier_member() {
    let script = r##"
        param { a 1 }
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("param format invalid"));
}

#[test]
fn reject_param_as_value_expression() {
    let script = r##"
        print (param { a })
    "##;
    let err = lang_to_ircode(script).unwrap_err();
    assert!(err.contains("return value") || err.contains("format invalid"));
}

fn find_print_expression(block: &IRNodeArray) -> &Box<dyn IRNode> {
    block
        .iter()
        .find_map(|node| {
            if let Some(print) = node.as_any().downcast_ref::<IRNodeSingle>() {
                if print.inst == Bytecode::PRT {
                    return Some(&print.subx);
                }
            }
            None
        })
        .expect("expected `print` statement in block")
}

#[test]
fn expression_precedence_add_mul() {
    let script = r##"
        print 1 + 2 * 3 + 4
    "##;
    let block = lang_to_irnode(script).unwrap();
    let expr = find_print_expression(&block);

    let top_add = expr
        .as_any()
        .downcast_ref::<IRNodeDouble>()
        .expect("expected top-level addition");
    assert_eq!(top_add.inst, Bytecode::ADD);

    let left_add = top_add
        .subx
        .as_any()
        .downcast_ref::<IRNodeDouble>()
        .expect("expected nested addition on the left");
    assert_eq!(left_add.inst, Bytecode::ADD);

    let multiplication = left_add
        .suby
        .as_any()
        .downcast_ref::<IRNodeDouble>()
        .expect("expected multiplication as the right operand of the inner addition");
    assert_eq!(multiplication.inst, Bytecode::MUL);

    let left_mul = multiplication
        .subx
        .as_any()
        .downcast_ref::<IRNodeLeaf>()
        .expect("expected literal `2`");
    assert_eq!(left_mul.inst, Bytecode::P2);

    let right_mul = multiplication
        .suby
        .as_any()
        .downcast_ref::<IRNodeLeaf>()
        .expect("expected literal `3`");
    assert_eq!(right_mul.inst, Bytecode::P3);

    let right_const = top_add
        .suby
        .as_any()
        .downcast_ref::<IRNodeParam1>()
        .expect("expected constant `4`");
    assert_eq!(right_const.inst, Bytecode::PU8);
    assert_eq!(right_const.para, 4);
}

#[test]
fn expression_precedence_pow_right_assoc() {
    let script = r##"
        print 2 ** 3 ** 2
    "##;
    let block = lang_to_irnode(script).unwrap();
    let expr = find_print_expression(&block);

    let top_pow = expr
        .as_any()
        .downcast_ref::<IRNodeDouble>()
        .expect("expected top-level pow");
    assert_eq!(top_pow.inst, Bytecode::POW);

    let right_pow = top_pow
        .suby
        .as_any()
        .downcast_ref::<IRNodeDouble>()
        .expect("expected nested pow on the right");
    assert_eq!(right_pow.inst, Bytecode::POW);

    let inner_left = right_pow
        .subx
        .as_any()
        .downcast_ref::<IRNodeLeaf>()
        .expect("expected literal `3`");
    assert_eq!(inner_left.inst, Bytecode::P3);

    let inner_right = right_pow
        .suby
        .as_any()
        .downcast_ref::<IRNodeLeaf>()
        .expect("expected literal `2`");
    assert_eq!(inner_right.inst, Bytecode::P2);

    let top_left = top_pow
        .subx
        .as_any()
        .downcast_ref::<IRNodeLeaf>()
        .expect("expected literal `2`");
    assert_eq!(top_left.inst, Bytecode::P2);
}

#[test]
fn decompile_preserves_subtract_parens() {
    let script = r##"
        print 5 - (3 - 2)
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircode).unwrap();
    assert!(printed.contains("5 - (3 - 2)"));
    assert!(printed.contains("print"));
}

#[test]
fn decompile_preserves_multiply_parens() {
    let script = r##"
        print 5 * (3 * 2)
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircode).unwrap();
    assert!(printed.contains("5 * (3 * 2)"));
    assert!(printed.contains("print"));
}

#[test]
fn decompile_hacswap_sell_args_without_list() {
    let script = r##"
        lib HacSwap = 1: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
        var sat = 4626909 as u64
        var zhu = HacSwap.sell(sat, 100000, 300)
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&ircode, Some(&smap)).unwrap();
    // panic!("{}", printed);
    assert!(
        printed.contains("ext(1).sell(sat, 100000, 300)"),
        "unexpected decompiled text:\n{}",
        printed
    );
    assert!(!printed.contains("pack_list {"));
}

#[test]
fn decompile_action_transfer_args_split_by_arity() {
    let script = r##"
        var adr = address_ptr(1)
        var val = 12345 as u64
        transfer_sat_to(adr, val)
        transfer_hac_from(adr, zhu_to_hac(val))
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircode).unwrap();
    assert!(printed.contains("transfer_sat_to($0, $1)"));
    assert!(printed.contains("transfer_hac_from($0, zhu_to_hac($1))"));
}

#[test]
fn recover_literals_false_keeps_bytes_constants_as_hex() {
    let string_ir = lang_to_irnode(r#"return "abc""#).unwrap();
    let string_out = Formater::new(&PrintOption::new("  ", 0)).print(&string_ir);
    assert!(string_out.contains("0x616263"));
    assert!(!string_out.contains("\"abc\""));
    assert_eq!(
        lang_to_ircode(&string_out).unwrap(),
        lang_to_ircode(r#"return "abc""#).unwrap()
    );

    let address_ir = lang_to_irnode("return emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS").unwrap();
    let address_out = Formater::new(&PrintOption::new("  ", 0)).print(&address_ir);
    assert!(address_out.contains("0x"));
    assert!(address_out.contains("as address"));
    assert!(!address_out.contains("emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS"));
    assert_eq!(
        lang_to_ircode(&address_out).unwrap(),
        lang_to_ircode("return emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS").unwrap()
    );
}

#[test]
fn recover_literals_true_keeps_raw_address_bytes_as_hex() {
    use field::{Address as FieldAddress, Serialize};

    let readable = "emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS";
    let addr = FieldAddress::from_readable(readable).unwrap();
    let hexstr = hex::encode(addr.serialize());
    let script = format!("return 0x{}", hexstr);
    let ircode = lang_to_ircode(&script).unwrap();
    let block = lang_to_irnode(&script).unwrap();
    let mut opt = PrintOption::new("  ", 0);
    opt.recover_literals = true;
    let printed = Formater::new(&opt).print(&block);

    assert!(printed.contains(&format!("0x{}", hexstr)));
    assert!(!printed.contains(readable));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn format_ircode_rehydrates_numeric_literal() {
    let script = r##"
        print 70000 as u32
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = format_ircode_to_lang(&ircode, Some(&smap)).unwrap();
    assert!(formatted.contains("70000"));
    assert!(!formatted.contains("as u32"));
}

#[test]
fn format_ircode_preserves_mismatched_cast() {
    let script = r##"
        print 1 as u64
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(formatted.contains("1 as u64"));
}

#[test]
fn format_ircode_preserves_mismatched_cast_to_bool() {
    let script = r##"
        print 1 as bool
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(formatted.contains("1 as bool"));
}

#[test]
fn format_ircode_preserves_mismatched_cast_to_address() {
    let script = r##"
        var x = 1
        print x as address
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(formatted.contains("cast_to(9") || formatted.contains("as address"));
}

#[test]
fn redundant_as_bytes_is_elided_and_roundtrip_stable() {
    let base = r##"
        print "data"
    "##;
    let script = r##"
        print "data" as bytes
    "##;
    let base_ir = lang_to_ircode(base).unwrap();
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    assert_eq!(ircode, base_ir);
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    let reparsed = lang_to_ircode(&formatted).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn redundant_as_address_is_elided_and_roundtrip_stable() {
    let base = r##"
        print emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
    "##;
    let script = r##"
        print emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS as address
    "##;
    let base_ir = lang_to_ircode(base).unwrap();
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    assert_eq!(ircode, base_ir);
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    let reparsed = lang_to_ircode(&formatted).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn non_redundant_bytes_to_address_cast_is_preserved() {
    let script = r##"
        print (emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS as bytes) as address
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(formatted.contains("as address"));
    let reparsed = lang_to_ircode(&formatted).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn invalid_bytes_literal_to_address_fails_at_compile_time() {
    let script = r##"
        print 0xABCD as address
    "##;
    assert!(lang_to_ircode(script).is_err());
}

#[test]
fn format_ircode_preserves_cto_u8_opcode_identity() {
    let script = r##"
        print cast_to(2, 1)
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(formatted.contains("cast_to(2"));
    let reparsed = lang_to_ircode(&formatted).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn format_ircode_preserves_cto_address_opcode_identity() {
    let script = r##"
        print cast_to(9, 1)
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let formatted = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(formatted.contains("cast_to(9") || formatted.contains("as address"));
    let reparsed = lang_to_ircode(&formatted).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn pair_count_packmap_is_not_decompiled_as_map_literal() {
    // Use raw IRBYTECODE payload so the decompiler exercises the PACKMAP
    // path directly, without going through the IRLIST validator which now
    // correctly rejects count/arity mismatches at parse time.
    let bytes = vec![
        Bytecode::P1 as u8,
        Bytecode::P2 as u8,
        Bytecode::PU8 as u8,
        1,
        Bytecode::PACKMAP as u8,
    ];
    let mut root = IRNodeArray::with_opcode(Bytecode::IRBLOCK);
    root.push(Box::new(IRNodeBytecodes { codes: bytes }));

    let ircode = drop_irblock_wrap(root.serialize()).unwrap();
    let printed = format_ircode_to_lang(&ircode, None).unwrap();
    // Decompiler must NOT synthesize a `map {}` literal from this raw
    // bytecode fragment; it must stay as `bytecode { ... }`.
    assert!(!printed.contains("map {"), "printed: {}", printed);
    assert!(printed.contains("bytecode {"), "printed: {}", printed);
    // The `bytecode { ... }` form is valid Fitsh syntax (it wraps raw
    // opcode names/bytes), so re-parsing succeeds and produces back an
    // `IRNodeBytecodes`. The roundtrip IR is not byte-identical (the
    // header frame differs) but the decompiled source stays safe.
}

#[test]
fn list_keyword_roundtrip() {
    let script = r##"
        print [1, 2, 3]
    "##;
    let block = lang_to_irnode(script).unwrap();
    let mut opt = PrintOption::new("  ", 0);
    opt.call_short_syntax = true;
    opt.flatten_array_list = true;
    let printed = Formater::new(&opt).print(&block);
    assert!(printed.contains("[1, 2, 3]"));
    assert!(lang_to_ircode(&printed).is_ok());

    let default_printed = irnode_to_lang(block.clone()).unwrap();
    assert!(default_printed.contains("list {"));
    assert!(lang_to_ircode(&default_printed).is_ok());
}

#[test]
fn list_keyword_literal_compiles() {
    let script = r##"
        var arr = list { 1 2 3 }
        print arr
    "##;
    assert!(lang_to_ircode(script).is_ok());
}

#[test]
fn list_keyword_empty_newlist() {
    let script = r##"
        var arr = list { }
    "##;
    let codes = lang_to_ircode(script).unwrap();
    assert!(codes.contains(&(Bytecode::NEWLIST as u8)));
}

#[test]
fn call_short_syntax_uses_comment_short_form() {
    let script = r##"
        lib HacSwap = 1: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
        var sat = 4626909 as u64
        var zhu = HacSwap.sell(sat, 100000, 300)
        print zhu
    "##;
    let (block, smap) = lang_to_irnode_with_sourcemap(script).unwrap();
    let mut opt = PrintOption::new("  ", 0);
    opt.map = Some(&smap);
    opt.trim_root_block = true;
    opt.trim_head_alloc = true;
    opt.trim_param_unpack = true;
    opt.call_short_syntax = true;
    let printed = Formater::new(&opt).print(&block);
    assert!(printed.contains("ext(1).sell("));
    assert!(printed.contains("print"));
}

#[test]
fn call_short_syntax_without_lib_prelude_falls_back_to_indexed_lib_ref() {
    let script = r##"
        lib Fund = 2
        return Fund.deposit(1)
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let (block, smap) = lang_to_irnode_with_sourcemap(script).unwrap();
    let mut opt = PrintOption::new("  ", 0);
    opt.map = Some(&smap);
    opt.trim_root_block = true;
    opt.call_short_syntax = true;
    opt.emit_lib_prelude = false;
    let printed = Formater::new(&opt).print(&block);
    assert!(printed.contains("ext(2).deposit("));
    assert!(!printed.contains("Fund.deposit("));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn empty_array_generates_newlist() {
    let script = r##"
        var arr = []
    "##;
    let codes = lang_to_ircode(script).unwrap();
    assert!(codes.contains(&(Bytecode::NEWLIST as u8)));
}

#[test]
fn display_root_block_after_lib_prelude_roundtrips() {
    let script = r##"
        lib Fund = 2
        return Fund.deposit(1)
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(printed.starts_with("{"), "printed: {}", printed);
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn display_root_block_after_const_prelude_roundtrips() {
    let script = r##"
        const ONE = 1
        return ONE + 1
    "##;
    let (block, smap) = lang_to_irnode_with_sourcemap(script).unwrap();
    let ircode = lang_to_ircode(script).unwrap();
    let mut opt = PrintOption::new("  ", 0);
    opt.map = Some(&smap);
    opt.recover_literals = true;
    let printed = Formater::new(&opt).print(&block);
    assert!(
        printed.starts_with(
            "const ONE = 1
{"
        ),
        "printed: {}",
        printed
    );
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn decompile_with_sourcemap_lists_lib_defs_at_top() {
    let script = r##"
        lib HacSwap = 1: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
        var sat = 100000000 as u64
        print sat
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(printed.starts_with("{"), "printed: {}", printed);
}

#[test]
fn sourcemap_lib_prelude_not_injected_into_inline_blocks() {
    let script = r##"
        lib HacSwap = 1: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS
        // block used as call argument must not receive `lib ...` prelude.
        print sha3({
            print 1
            "abc"
        })
    "##;
    let (block, smap) = lang_to_irnode_with_sourcemap(script).unwrap();
    let opt = PrintOption::new("  ", 0).with_source_map(&smap);
    let printed = Formater::new(&opt).print(&block);
    assert_eq!(printed.matches("lib HacSwap = 1:").count(), 0);
}

#[test]
fn decompile_abort_as_keyword_without_redundant_end() {
    let script = r##"
        print 2
        abort
        end
    "##;
    let ircode = lang_to_ircode(script).unwrap();
    let printed = ircode_to_lang(&ircode).unwrap();
    assert!(printed.contains("abort"));
    assert!(!printed.contains(
        "
end"
    ));
    assert!(!printed.contains("abort()"));
    assert!(!printed.contains("end()"));
}

#[test]
fn decompile_local_vars_use_slot_names() {
    let script = r##"
        var foo = 123 as u64
        var bar = foo
        print bar
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = format_ircode_to_lang(&ircode, Some(&smap)).unwrap();
    // panic!("{}", printed);
    assert!(printed.contains("print bar"));
    assert!(printed.contains("let foo"));
}

#[test]
fn explicit_shortcut_call_keywords_normalize_to_current_surface() {
    let script = r##"
        callext 1::0xabcdef01()
        callextview 2::0x01020304(1)
        calluseview 3::0x0a0b0c0d(nil)
        callusepure 4::0x1a2b3c4d(nil)
        callthis 0::0x11223344(2)
        callself 0::0x22334455(3)
        callsuper 0::0x33445566(4)
        callselfview 0::0x44556677(5)
        callselfpure 0::0x55667788(6)
    "##;
    let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
    let printed = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
    assert!(printed.contains("call edit ext(1).0xabcdef01("));
    assert!(printed.contains("call view ext(2).0x01020304("));
    assert!(printed.contains("call view use(3).0x0a0b0c0d("));
    assert!(printed.contains("call pure use(4).0x1a2b3c4d("));
    assert!(printed.contains("call edit this.0x11223344("));
    assert!(printed.contains("call edit self.0x22334455("));
    assert!(printed.contains("call edit super.0x33445566("));
    assert!(printed.contains("call view self.0x44556677("));
    assert!(printed.contains("call pure self.0x55667788("));
    let reparsed = lang_to_ircode(&printed).unwrap();
    assert_eq!(ircode, reparsed);
}

#[test]
fn each_call_opcode_roundtrips_and_emits_expected_bytecode() {
    let cases = [
        (
            "ext(1).0x01020304(1)",
            Bytecode::CALLEXT,
            "call edit ext(1).0x01020304(",
        ),
        (
            "this.0x11223344(2)",
            Bytecode::CALLTHIS,
            "call edit this.0x11223344(",
        ),
        (
            "self.0x22334455(3)",
            Bytecode::CALLSELF,
            "call edit self.0x22334455(",
        ),
        (
            "super.0x33445566(4)",
            Bytecode::CALLSUPER,
            "call edit super.0x33445566(",
        ),
        (
            "self:0x44556677(5)",
            Bytecode::CALLSELFVIEW,
            "call view self.0x44556677(",
        ),
        (
            "self::0x55667788(6)",
            Bytecode::CALLSELFPURE,
            "call pure self.0x55667788(",
        ),
        (
            "ext(1):0x66778899(7)",
            Bytecode::CALLEXTVIEW,
            "call view ext(1).0x66778899(",
        ),
        (
            "calluseview 1::0x778899aa(8)",
            Bytecode::CALLUSEVIEW,
            "call view use(1).0x778899aa(",
        ),
        (
            "callusepure 1::0x8899aabb(9)",
            Bytecode::CALLUSEPURE,
            "call pure use(1).0x8899aabb(",
        ),
        (
            "codecall ext(1).0x99aabbcc",
            Bytecode::CODECALL,
            "codecall ext(1).0x99aabbcc",
        ),
        (
            "callext 1::0xaabbccdd(10)",
            Bytecode::CALLEXT,
            "call edit ext(1).0xaabbccdd(",
        ),
        (
            "callthis 0::0xbbccddee(11)",
            Bytecode::CALLTHIS,
            "call edit this.0xbbccddee(",
        ),
        (
            "callself 0::0xccddeeff(12)",
            Bytecode::CALLSELF,
            "call edit self.0xccddeeff(",
        ),
        (
            "callsuper 0::0xddeeff00(13)",
            Bytecode::CALLSUPER,
            "call edit super.0xddeeff00(",
        ),
        (
            "callselfview 0::0xeeff0011(14)",
            Bytecode::CALLSELFVIEW,
            "call view self.0xeeff0011(",
        ),
        (
            "callselfpure 0::0xff001122(15)",
            Bytecode::CALLSELFPURE,
            "call pure self.0xff001122(",
        ),
    ];

    for (script, opcode, needle) in cases {
        let (ircode, smap) = lang_to_ircode_with_sourcemap(script).unwrap();
        let bytecodes = convert_ir_to_bytecode(&ircode).unwrap();
        assert!(
            bytecodes.contains(&(opcode as u8)),
            "missing opcode {:?} in {:?}",
            opcode,
            bytecodes
        );
        let printed = ircode_to_lang_with_sourcemap(&ircode, &smap).unwrap();
        assert!(
            printed.contains(needle),
            "missing '{}' in decompiled text:
{}",
            needle,
            printed
        );
        let reparsed = lang_to_ircode(&printed).unwrap();
        assert_eq!(
            ircode, reparsed,
            "roundtrip mismatch for script:
{}",
            script
        );
    }
}

#[test]
fn external_lib_shorthand_without_binding_is_not_supported() {
    assert!(lang_to_ircode("call 1::0x01020304(10, 20)").is_err());
}

#[test]
fn fitshc_sharedframe_modifier_is_rejected() {
    let source = r##"
        contract Demo {
            function external sharedframe jump() {
                return nil
            }
        }
    "##;
    match vm::fitshc::compile(source) {
        Ok(_) => panic!("sharedframe keyword should be rejected"),
        Err(_) => {}
    }
}
