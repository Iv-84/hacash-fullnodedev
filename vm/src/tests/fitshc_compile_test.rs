#[cfg(test)]
mod fitshc_compile_tests {
    use crate::fitshc::compile as fitshc_compile;
    use crate::fitshc::compile_with_warnings as fitshc_compile_with_warnings;
    use crate::ir::convert_ir_to_runtime_bytecode;
    use crate::rt::{Bytecode, CodePkg, CodeType};

    const PRAGMA: &str = "pragma fitsh 1.0.0\n";

    fn strict_src(src: &str) -> String {
        if src.trim_start().starts_with("pragma ") {
            src.to_string()
        } else {
            format!("{}{}", PRAGMA, src)
        }
    }

    fn compile_src(src: &str) -> sys::Ret<crate::fitshc::compiler::FitshCompileOutput> {
        fitshc_compile(&strict_src(src))
    }

    fn direct_compile_err(src: &str) -> String {
        match fitshc_compile(src) {
            Ok(_) => panic!("fitshc compile should fail"),
            Err(e) => e.to_string(),
        }
    }

    fn expect_compile_err(src: &str, needle: &str) {
        let err = match compile_src(src) {
            Ok(_) => panic!("fitshc compile should fail"),
            Err(e) => e,
        };
        let text = err.to_string();
        assert!(
            text.contains(needle),
            "error mismatch\nsource:\n{}\nexpected substring: {}\nactual: {}",
            src,
            needle,
            text
        );
    }

    #[test]
    fn rejects_unknown_contract_body_token_instead_of_skipping() {
        let src = r#"
            contract demo {
                return 1
                function external f() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "unexpected token in contract body");
    }

    #[test]
    fn accepts_required_pragma_before_contract() {
        let src = r#"
            pragma fitsh 1.0.0
            contract demo {
                function external f() -> u8 { return 1 }
            }
        "#;
        assert!(fitshc_compile(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn rejects_use_pragma_prefix_before_contract() {
        let src = r#"
            use pragma 1.0.0
            contract demo {
                function external f() -> u8 { return 1 }
            }
        "#;
        let err = direct_compile_err(src);
        assert!(err.contains("expected 'pragma fitsh"));
    }

    #[test]
    fn rejects_missing_pragma_before_contract() {
        let src = r#"
            contract demo {
                function external f() -> u8 { return 1 }
            }
        "#;
        let err = direct_compile_err(src);
        assert!(err.contains("expected 'pragma fitsh"));
    }

    #[test]
    fn rejects_unsupported_major_version() {
        let src = r#"
            pragma fitsh 2.0.0
            contract demo {
                function external f() -> u8 { return 1 }
            }
        "#;
        let err = direct_compile_err(src);
        assert!(err.contains("unsupported fitsh major version"));
    }

    #[test]
    fn rejects_newer_minor_version() {
        let src = r#"
            pragma fitsh 1.1.0
            contract demo {
                function external f() -> u8 { return 1 }
            }
        "#;
        let err = direct_compile_err(src);
        assert!(err.contains("requires newer compatible features"));
    }

    #[test]
    fn warns_on_patch_version_difference() {
        let src = r#"
            pragma fitsh 1.0.1
            contract demo {
                function external f() -> u8 { return 1 }
            }
        "#;
        let (_, warnings) = fitshc_compile_with_warnings(src).expect("patch must compile");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("patch version 1.0.1 differs"));
    }

    #[test]
    fn rejects_trailing_tokens_after_contract_end() {
        let src = r#"
            contract demo {
                function external f() -> u8 { return 1 }
            }
            function external g() -> u8 { return 2 }
        "#;
        expect_compile_err(src, "unexpected token after contract end");
    }

    #[test]
    fn rejects_unclosed_parenthesized_return_type() {
        let src = r#"
            contract demo {
                function external f() -> (u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "expected ')' after return type");
    }

    #[test]
    fn rejects_missing_arg_colon() {
        let src = r#"
            contract demo {
                function external f(a u8) -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "expected ':' after arg name");
    }

    #[test]
    fn rejects_non_contract_library_address_without_panic() {
        let src = r#"
            contract demo {
                library [A: 18dekVcACnj6Tbd69SsexVMQ5KLBZZfn5K]
                function external f() -> u8 { return 1 }
            }
        "#;
        let res = std::panic::catch_unwind(|| compile_src(src));
        assert!(
            res.is_ok(),
            "fitshc compile panicked for non-contract library address"
        );
        let err = match res.unwrap() {
            Ok(_) => panic!("fitshc compile should fail"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("is not CONTRACT"));
    }

    #[test]
    fn rejects_non_contract_inherit_address_without_panic() {
        let src = r#"
            contract demo {
                inherit [A: 18dekVcACnj6Tbd69SsexVMQ5KLBZZfn5K]
                function external f() -> u8 { return 1 }
            }
        "#;
        let res = std::panic::catch_unwind(|| compile_src(src));
        assert!(
            res.is_ok(),
            "fitshc compile panicked for non-contract inherit address"
        );
        let err = match res.unwrap() {
            Ok(_) => panic!("fitshc compile should fail"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("is not CONTRACT"));
    }

    #[test]
    fn rejects_duplicate_top_level_const_without_body_compile() {
        let src = r#"
            contract demo {
                const A = 1
                const A = 2
            }
        "#;
        expect_compile_err(src, "duplicate const 'A'");
    }

    #[test]
    fn rejects_inherit_count_overflow_without_panic() {
        use crate::ContractAddress;
        use field::{Address, Uint4};

        let base = Address::from_readable("18dekVcACnj6Tbd69SsexVMQ5KLBZZfn5K").unwrap();
        let inherit = (0..=u8::MAX as u32)
            .map(|idx| {
                let addr = ContractAddress::calculate(&base, &Uint4::from(idx));
                format!("I{}: {}", idx, addr.to_addr().to_readable())
            })
            .collect::<Vec<_>>()
            .join(", ");
        let src = format!(
            "contract demo {{
    inherit [{}]
    function external f() -> u8 {{ return 1 }}
}}",
            inherit
        );
        let res = std::panic::catch_unwind(|| compile_src(&src));
        assert!(
            res.is_ok(),
            "fitshc compile panicked for overflowing inherit count"
        );
        let err = match res.unwrap() {
            Ok(_) => panic!("fitshc compile should fail"),
            Err(e) => e,
        };
        assert!(
            err.to_string()
                .contains("too many inherit contracts: max 255")
        );
    }

    #[test]
    fn rejects_duplicate_library_address() {
        let src = r#"
            contract demo {
                library [A: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS, B: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS]
                function external f() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "duplicate library address");
    }

    #[test]
    fn rejects_duplicate_inherit_address() {
        let src = r#"
            contract demo {
                inherit [A: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS, B: emqjNS9PscqdBpMtnC3Jfuc4mvZUPYTPS]
                function external f() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "duplicate inherit address");
    }

    #[test]
    fn rejects_duplicate_function_signature() {
        let src = r#"
            contract demo {
                function external f() -> u8 { return 1 }
                function external f() -> u8 { return 2 }
            }
        "#;
        expect_compile_err(src, "duplicate function 'f' signature");
    }

    #[test]
    fn rejects_duplicate_abstract_signature() {
        let src = r#"
            contract demo {
                abstract Construct(data: bytes) { abort }
                abstract Construct(data: bytes) { abort }
            }
        "#;
        expect_compile_err(src, "duplicate abstract 'Construct'");
    }

    #[test]
    fn rejects_contract_library_count_overflow_without_panic() {
        use crate::ContractAddress;
        use field::{Address, Uint4};

        let base = Address::from_readable("18dekVcACnj6Tbd69SsexVMQ5KLBZZfn5K").unwrap();
        let libs = (0..=u8::MAX as u32)
            .map(|idx| {
                let addr = ContractAddress::calculate(&base, &Uint4::from(idx));
                format!("L{}: {}", idx, addr.to_addr().to_readable())
            })
            .collect::<Vec<_>>()
            .join(", ");
        let src = format!(
            "contract demo {{
    library [{}]
    function external f() -> u8 {{ return 1 }}
}}",
            libs
        );
        let res = std::panic::catch_unwind(|| compile_src(&src));
        assert!(
            res.is_ok(),
            "fitshc compile panicked for overflowing library count"
        );
        let err = match res.unwrap() {
            Ok(_) => panic!("fitshc compile should fail"),
            Err(e) => e,
        };
        assert!(
            err.to_string()
                .contains("too many contract libraries: max 255")
        );
    }

    #[test]
    fn accepts_compo_param_and_return_types() {
        let src = r#"
            contract demo {
                function helper(doc: map) -> map {
                    return doc
                }
                function external run() -> map {
                    return this.helper(map { "a": 1 })
                }
            }
        "#;
        assert!(compile_src(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn accepts_tuple_return_type_and_constructor() {
        let src = r#"
            contract demo {
                function build() -> tuple {
                    return tuple(7, map { "kind": "hnft" })
                }
                function consume(num: u8, doc: map) -> u8 {
                    assert doc is map
                    return num
                }
                function external run() -> u8 {
                    return this.consume(this.build())
                }
            }
        "#;
        assert!(compile_src(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn accepts_soft_separators_in_contract_body_and_function_body() {
        let src = r#"
            contract demo {
                const A = 1;;,;
                function helper(a: u8,, b: u8) -> u8 {
                    var s = a + b;;,,
                    return s
                },;,
                function external run() -> u8 {
                    return this.helper(1,,;2)
                }
            }
        "#;
        assert!(compile_src(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn accepts_strict_deploy_block() {
        let src = r#"
            contract demo {
                deploy {
                    protocol_cost: amount("1:248"),
                    nonce: 1u32,
                    construct_argv: 0x0102,
                }
                function external run() -> u8 { return 1 }
            }
        "#;
        assert!(compile_src(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn accepts_ircode_log_before_terminal_statements() {
        let src = r#"
            contract demo {
                function external ircode log_then_throw() -> u64 {
                    log("topic", 1)
                    throw 7
                }
                function external ircode log_then_return() -> u64 {
                    log("topic", 1)
                    return 0
                }
            }
        "#;
        assert!(compile_src(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn ircode_log_does_not_swallow_following_expression_nodes() {
        let src = r#"
            contract demo {
                function external ircode run() -> u64 {
                    log("topic", 1)
                    2
                    3
                    return 0
                }
            }
        "#;
        let (contract, _, _, _) = compile_src(src).expect("fitshc compile should succeed");
        let mut sto = contract.into_sto();
        let func = sto.userfuncs.as_mut().first_mut().expect("user function");
        let pkg = CodePkg::try_from(std::mem::take(&mut func.code_stuff)).expect("code pkg");
        assert!(matches!(pkg.code_type().unwrap(), CodeType::IRNode));
        let codes = convert_ir_to_runtime_bytecode(&pkg.data).expect("IR codegen");
        let p2 = codes.iter().filter(|b| **b == Bytecode::P2 as u8).count();
        let p3 = codes.iter().filter(|b| **b == Bytecode::P3 as u8).count();
        assert_eq!(p2, 1, "P2 expression must remain a standalone statement");
        assert_eq!(p3, 1, "P3 expression must remain a standalone statement");
    }

    #[test]
    fn rejects_ircode_standalone_stack_source_ops() {
        let src = r#"
            contract demo {
                function external ircode run() -> u64 {
                    return roll_0()
                }
            }
        "#;
        expect_compile_err(src, "existing stack value");
    }

    #[test]
    fn rejects_duplicate_separator_in_deploy_block() {
        let src = r#"
            contract demo {
                deploy {
                    nonce: 1,,;
                    construct_argv: 0x0102
                }
                function external run() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "duplicate deploy separator");
    }

    #[test]
    fn rejects_legacy_protocol_cost_literal() {
        let src = r#"
            contract demo {
                deploy {
                    protocol_cost: "1:248"
                }
                function external run() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "expected amount");
    }

    #[test]
    fn rejects_construct_must_in_deploy_block() {
        let src = r#"
            contract demo {
                deploy {
                    construct_must: false
                }
                function external run() -> u8 { return 1 }
            }
        "#;
        let err = compile_src(src)
            .err()
            .expect("fitshc compile should reject construct_must");
        assert!(
            err.to_string()
                .contains("unknown deploy field 'construct_must'")
        );
    }

    #[test]
    fn accepts_bool_and_typed_top_level_consts() {
        let src = r#"
            contract demo {
                const OK: bool = true
                const LIM: u64 = 100
                function external run() -> u64 {
                    if OK { return LIM }
                    return 0
                }
            }
        "#;
        assert!(compile_src(src).is_ok(), "fitshc compile should succeed");
    }

    #[test]
    fn rejects_reserved_function_modifier() {
        let src = r#"
            contract demo {
                function virtual f() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "reserved function modifier 'virtual' is not supported");
    }

    #[test]
    fn rejects_duplicate_code_modifier() {
        let src = r#"
            contract demo {
                function external ircode bytecode f() -> u8 { return 1 }
            }
        "#;
        expect_compile_err(src, "function code modifier must appear at most once");
    }
}
