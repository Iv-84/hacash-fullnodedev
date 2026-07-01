/* parse ir list */
pub fn parse_ir_list(stuff: &[u8], seek: &mut usize) -> VmrtRes<IRNodeArray> {
    let u16max = u16::MAX as usize;
    let codelen = stuff.len();
    if codelen > u16max {
        return itr_err_code!(CodeTooLong);
    }
    let mut list = IRNodeArray::new_list();
    loop {
        let pres = parse_ir_node(stuff, seek)?;
        let Some(irnode) = pres else {
            break; // end
        };
        list.push(irnode);
    }
    // finish
    Ok(list)
}

/* parse ir block */
pub fn parse_ir_block(stuff: &[u8], seek: &mut usize) -> VmrtRes<IRNodeArray> {
    let u16max = u16::MAX as usize;
    let codelen = stuff.len();
    if codelen > u16max {
        return itr_err_code!(CodeTooLong);
    }
    let mut block = IRNodeArray::new_block();
    loop {
        let pres = parse_ir_node(stuff, seek)?;
        let Some(irnode) = pres else {
            break; // end
        };
        block.push(irnode);
    }
    // finish
    Ok(block)
}

/* * * parse one node (public interface for serialized IR) */
pub fn parse_ir_node_one(stuff: &[u8], seek: &mut usize) -> VmrtRes<Box<dyn IRNode>> {
    parse_ir_node_must(stuff, seek, 0, false, IrParseContext::Tree, 0)
}

/* * * parse one node */
fn parse_ir_node(stuff: &[u8], seek: &mut usize) -> VmrtRes<Option<Box<dyn IRNode>>> {
    let codesz = stuff.len();
    if codesz == 0 || *seek >= codesz {
        return Ok(None); // finish end
    }
    Ok(Some(parse_ir_node_must(
        stuff,
        seek,
        0,
        false,
        IrParseContext::Tree,
        0,
    )?))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IrParseContext {
    Tree,
    StackList,
    StackValue,
}

fn is_ir_container_opcode(inst: Bytecode) -> bool {
    matches!(inst, IRBYTECODE | IRLIST | IRBLOCK | IRBLOCKR)
}

fn is_stack_list_tail_opcode(inst: Bytecode) -> bool {
    matches!(inst, PACKLIST | PACKMAP | PACKTUPLE)
}

// I-2: opcodes that implicitly read the existing top of stack (without
// declaring it via metadata `input`). `DUP` and `ROLL0` have no params and
// cannot be represented as a standalone IRNode in tree context — only
// `ROLL0` is permitted, and only inside an `IrParseContext::StackValue`
// slot (i.e. as the dedicated stack fixup placed in front of
// `PUT(slot, ...)` or `UNPACK(idx, ...)`).
//
// `ROLL` is intentionally excluded from this list: it carries a `u8` param
// (`meta.param=1`) and is parsed as `IRNodeParam1`, making it a valid
// standalone IRNode that encodes its own stack offset.
fn is_hidden_stack_input_opcode(inst: Bytecode) -> bool {
    matches!(inst, DUP | ROLL0)
}

// I-3: only `ROLL0` is allowed inside a `StackValue` slot. This is an
// intentional shape restriction so that IR round-trips deterministically;
// `DUP`/`ROLL n` would create ambiguities about which existing stack value
// the surrounding op is reading.
fn is_stack_value_opcode_allowed(inst: Bytecode, context: IrParseContext) -> bool {
    context == IrParseContext::StackValue && inst == ROLL0
}

// I-1: opcodes with `meta.input == 255` (dynamic stack input) cannot stand
// alone as IRNodes. They are only legal as:
//   * `POP` — explicit one-value pop in any tree context;
//   * IR container opcodes (`IRBYTECODE/IRLIST/IRBLOCK/IRBLOCKR`) — they have
//     their own structural arity;
//   * `PACKLIST/PACKMAP/PACKTUPLE` — only at the tail of an `IRLIST`
//     (`IrParseContext::StackList`), guarded by `validate_irlist_stack_tail`.
// Anything else (e.g. `DUPN/POPN/JOIN/REV`) must be expressed via a raw
// `IRBYTECODE` fragment; this keeps the standalone IRNode grammar
// unambiguous.
fn is_dynamic_stack_opcode_allowed(inst: Bytecode, context: IrParseContext) -> bool {
    inst == POP
        || is_ir_container_opcode(inst)
        || (context == IrParseContext::StackList && is_stack_list_tail_opcode(inst))
}

fn irnode_const_usize(node: &dyn IRNode) -> Option<usize> {
    if let Some(leaf) = node.as_any().downcast_ref::<IRNodeLeaf>() {
        return match leaf.inst {
            P0 => Some(0),
            P1 => Some(1),
            P2 => Some(2),
            P3 => Some(3),
            _ => None,
        };
    }
    if let Some(param1) = node.as_any().downcast_ref::<IRNodeParam1>() {
        if param1.inst == PU8 {
            return Some(param1.para as usize);
        }
    }
    if let Some(param2) = node.as_any().downcast_ref::<IRNodeParam2>() {
        if param2.inst == PU16 {
            return Some(u16::from_be_bytes(param2.para) as usize);
        }
    }
    None
}

fn validate_irlist_stack_tail(list: &[Box<dyn IRNode>]) -> VmrtErr {
    let Some(last) = list.last() else {
        return Ok(());
    };
    let inst: Bytecode = std_mem_transmute!(last.bytecode());
    if !is_stack_list_tail_opcode(inst) {
        return Ok(());
    }
    if list.len() < 2 {
        return itr_err_fmt!(InstInvalid, "{:?} requires a preceding item count", inst);
    }
    let elem_count = list.len() - 2;
    let count = irnode_const_usize(&**list.get(elem_count).unwrap()).ok_or_else(|| {
        ItrErr::new(
            InstInvalid,
            &format!("{:?} item count must be a literal", inst),
        )
    })?;
    if count != elem_count {
        return itr_err_fmt!(
            InstInvalid,
            "{:?} item count mismatch: expected {} got {}",
            inst,
            elem_count,
            count
        );
    }
    if count == 0 {
        return itr_err_fmt!(InstInvalid, "{:?} item count cannot be zero", inst);
    }
    if inst == PACKMAP && count % 2 != 0 {
        return itr_err_fmt!(InstInvalid, "PACKMAP item count must be even");
    }
    Ok(())
}

// must
fn parse_ir_node_must(
    stuff: &[u8],
    seek: &mut usize,
    depth: usize,
    isrtv: bool,
    context: IrParseContext,
    loop_depth: usize,
) -> VmrtRes<Box<dyn IRNode>> {
    if depth > 32 {
        return itr_err_code!(IRNodeOverDepth);
    }

    let codesz = stuff.len();
    if codesz == 0 || *seek >= codesz {
        return itr_err_code!(CodeOverflow);
    }

    // code
    let insbyte = stuff[*seek]; // u8
    let inst: Bytecode = std_mem_transmute!(insbyte);

    // IRBREAK / IRCONTINUE are only valid inside an IRWHILE body.
    if matches!(inst, IRBREAK | IRCONTINUE) && loop_depth == 0 {
        return itr_err_fmt!(
            InstInvalid,
            "{:?} is only valid inside a while loop body",
            inst
        );
    }

    // parse
    let irnode: Box<dyn IRNode>;
    // mv sk
    *seek += 1;

    macro_rules! itrp1 {
        () => {{
            let _r = *seek + 1;
            if _r > codesz {
                return itr_err_code!(CodeOverflow);
            }
            let bt = stuff[*seek];
            *seek = _r;
            bt
        }};
    }

    macro_rules! itrp2 {
        () => {{
            let _r = *seek + 2;
            if _r > codesz {
                return itr_err_code!(CodeOverflow);
            }
            let bts: [u8; 2] = stuff[*seek.._r].try_into().unwrap();
            *seek = _r;
            bts
        }};
    }

    macro_rules! itrbuf {
        ($l: expr) => {{
            let _r = *seek + $l;
            if _r > codesz {
                return itr_err_code!(CodeOverflow);
            }
            let bts = stuff[*seek.._r].to_vec();
            *seek = _r;
            bts
        }};
    }

    macro_rules! subdph {
        ($ndp:expr, $rtv:expr, $ctx:expr) => {
            parse_ir_node_must(stuff, seek, $ndp, $rtv, $ctx, loop_depth)?
        };
        ($ndp:expr, $rtv:expr) => {
            subdph!($ndp, $rtv, IrParseContext::Tree)
        };
    }

    // Sub-node for a while-loop body: inherits a deeper loop depth.
    macro_rules! subdph_loop_body {
        ($ndp:expr, $rtv:expr, $ctx:expr) => {
            parse_ir_node_must(stuff, seek, $ndp, $rtv, $ctx, loop_depth + 1)?
        };
    }

    macro_rules! submust {
        () => {
            subdph!(depth + 1, true)
        };
    }

    macro_rules! param1_multi {
        ($argc:expr, $hrtv:expr, $node:ident { $($field:ident),+ $(,)? }) => {{
            let para = itrp1!();
            validate_param1_multi_arity(inst, para, $hrtv, $argc)?;
            Box::new($node {
                hrtv: $hrtv,
                inst,
                para,
                $($field: submust!()),+
            })
        }}
    }

    // parse
    let meta = inst.metadata();
    if meta.output > 1 {
        // I-11: any opcode whose runtime output > 1 (including the `255`
        // dynamic-output marker used by `DUPN`/`REV`) cannot be expressed as
        // a single IRNode in the tree — a tree node has at most one stack
        // result. Such ops must be wrapped in a raw `IRBYTECODE` fragment.
        return itr_err_fmt!(
            InstInvalid,
            "{:?} has multi-output and is not representable in IRNode",
            inst
        );
    }
    if meta.input == 255 && !is_dynamic_stack_opcode_allowed(inst, context) {
        return itr_err_fmt!(
            InstInvalid,
            "{:?} has dynamic stack input and is not representable as a standalone IRNode",
            inst
        );
    }
    if is_hidden_stack_input_opcode(inst) && !is_stack_value_opcode_allowed(inst, context) {
        return itr_err_fmt!(
            InstInvalid,
            "{:?} reads an existing stack value and is not representable as a standalone IRNode",
            inst
        );
    }
    let hrtv = meta.output == 1;
    // parse
    irnode = match inst {
        // BYTECODE LIST BLOCK IF WHILE
        IRBYTECODE => {
            let p = itrp2!();
            let n = u16::from_be_bytes(p);
            let bytes = itrbuf!(n as usize);
            // Reject IR-only opcodes, absolute jumps, and misaligned
            // params before they reach codegen. `IRNodeBytecodes::new` is
            // the single point of truth for what a raw IR fragment is
            // allowed to contain; routing the parser through it keeps
            // serialized-IR ingestion on the same gate as in-process
            // builders.
            Box::new(IRNodeBytecodes::new(bytes)?)
        }
        IRLIST => {
            let mut list = IRNodeArray::with_opcode(inst);
            let p = itrp2!();
            let n = u16::from_be_bytes(p);
            let ndp = depth + 1;
            for i in 0..n {
                // IRLIST is a sequence container; whether it yields a value depends on its last item.
                list.push(parse_ir_node_must(
                    stuff,
                    seek,
                    ndp,
                    false,
                    maybe!(i + 1 == n, IrParseContext::StackList, IrParseContext::Tree),
                    loop_depth,
                )?);
            }
            validate_irlist_stack_tail(&list.subs)?;
            Box::new(list)
        }
        IRBLOCK | IRBLOCKR => {
            let mut block = IRNodeArray::with_opcode(inst);
            let p = itrp2!();
            let n = u16::from_be_bytes(p);
            let ndp = depth + 1;
            if inst == IRBLOCKR && n == 0 {
                return itr_err_fmt!(InstInvalid, "empty block expr");
            }
            for i in 0..n {
                // block statement items do not need to produce values, except: - IRBLOCKR (block expression) requires the last item to produce a value.
                let need_rtv = inst == IRBLOCKR && i + 1 == n;
                block.push(subdph!(ndp, need_rtv));
            }
            Box::new(block)
        }
        IRIF | IRIFR => {
            let ndp = depth + 1;
            let need_branch_rtv = inst == IRIFR;
            Box::new(IRNodeTriple {
                hrtv,
                inst,
                subx: subdph!(ndp, true),
                // if-expression requires both branches to produce values
                suby: subdph!(ndp, need_branch_rtv),
                subz: subdph!(ndp, need_branch_rtv),
            })
        }
        IRWHILE => {
            let ndp = depth + 1;
            // While condition stays in the surrounding loop scope; only the body
            // opens a new loop scope where IRBREAK/IRCONTINUE become legal.
            Box::new(IRNodeDouble {
                hrtv,
                inst,
                subx: subdph!(ndp, true),
                suby: subdph_loop_body!(ndp, false, IrParseContext::Tree),
            })
        }
        PBUF | PBUFL => {
            let (bl, size) = maybe!(
                PBUF == inst,
                {
                    let p1 = itrp1!();
                    (p1 as usize, vec![p1])
                },
                {
                    let p2 = itrp2!();
                    (u16::from_be_bytes(p2) as usize, p2.to_vec())
                }
            );
            let buf = itrbuf!(bl);
            let para: Vec<u8> = iter::empty().chain(size).chain(buf).collect();
            Box::new(IRNodeParams { hrtv, inst, para })
        }
        _ => {
            // other inst
            if !meta.valid {
                return itr_err_fmt!(InstInvalid, "bytecode {} not found", inst as u8);
            }
            match (meta.param, meta.input) {
                // (0, 0) => Box::new(IRNodeLeaf::notext(hrtv, inst)), // leaf
                (0, 1) => Box::new(IRNodeSingle {
                    hrtv,
                    inst,
                    subx: submust!(),
                }),
                (0, 2) => Box::new(IRNodeDouble {
                    hrtv,
                    inst,
                    subx: subdph!(
                        depth + 1,
                        true,
                        maybe!(
                            inst == UNPACK,
                            IrParseContext::StackValue,
                            IrParseContext::Tree
                        )
                    ),
                    suby: submust!(),
                }),
                (0, 3) => Box::new(IRNodeTriple {
                    hrtv,
                    inst,
                    subx: submust!(),
                    suby: submust!(),
                    subz: submust!(),
                }),
                (0, 4) => Box::new(IRNodeQuad {
                    hrtv,
                    inst,
                    subx: submust!(),
                    suby: submust!(),
                    subz: submust!(),
                    subw: submust!(),
                }),
                (0, 5) => Box::new(IRNodeQuint {
                    hrtv,
                    inst,
                    suba: submust!(),
                    subb: submust!(),
                    subc: submust!(),
                    subd: submust!(),
                    sube: submust!(),
                }),
                (0, 0 | 255) => Box::new(IRNodeLeaf::notext(hrtv, inst)), // leaf
                (1, 2) => param1_multi!(2, hrtv, IRNodeParam1Double { subx, suby }),
                (1, 3) => param1_multi!(3, hrtv, IRNodeParam1Triple { subx, suby, subz }),
                (1, 4) => param1_multi!(
                    4,
                    hrtv,
                    IRNodeParam1Quad {
                        subx,
                        suby,
                        subz,
                        subw
                    }
                ),
                (1, 0 | 255) => Box::new(IRNodeParam1 {
                    hrtv,
                    inst,
                    para: itrp1!(),
                    text: s!(""),
                }), // params one (no IR subtree args)
                (2, 0 | 255) => Box::new(IRNodeParam2 {
                    hrtv,
                    inst,
                    para: itrp2!(),
                }), // params two
                (1, 1) => Box::new(IRNodeParam1Single {
                    hrtv,
                    inst,
                    para: itrp1!(),
                    subx: subdph!(
                        depth + 1,
                        true,
                        maybe!(
                            inst == PUT,
                            IrParseContext::StackValue,
                            IrParseContext::Tree
                        )
                    ),
                }), // params one
                (2, 1) => Box::new(IRNodeParam2Single {
                    hrtv,
                    inst,
                    para: itrp2!(),
                    subx: subdph!(depth + 1, true, IrParseContext::Tree),
                }), // params two
                (a, 0) => Box::new(IRNodeParams {
                    hrtv,
                    inst,
                    para: itrbuf!(a as usize),
                }), // params
                (a, 1) => Box::new(IRNodeParamsSingle {
                    hrtv,
                    inst,
                    para: itrbuf!(a as usize),
                    subx: subdph!(depth + 1, true, IrParseContext::Tree),
                }),
                _ => {
                    return itr_err_fmt!(
                        InstInvalid,
                        "invalid irnode {:?} of ps={} i={}",
                        inst,
                        meta.param,
                        meta.input
                    );
                }
            }
        }
    };
    // check return value based on the actual parsed node (not just bytecode metadata). This is important for container nodes like IRLIST/IRBLOCK whose return-value-ness is contextual.
    if isrtv && !irnode.hasretval() {
        return itr_err_fmt!(
            InstInvalid,
            "irnode {} return value check failed",
            inst as u8
        );
    }
    Ok(irnode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irlist_allows_noret_when_not_required() {
        // IRLIST: [P0, LOG1(P0, P1)] is a valid statement sequence (LOG1 consumes topic0 + data and returns no value).
        let bytes: Vec<u8> = vec![
            IRLIST as u8,
            0x00,
            0x02,
            P0 as u8,
            LOG1 as u8,
            P0 as u8,
            P1 as u8,
        ];
        let mut seek = 0usize;
        let node = parse_ir_node_one(&bytes, &mut seek).expect("parse IRLIST");
        assert!(!node.hasretval());
        assert_eq!(seek, bytes.len());
    }

    #[test]
    fn irlist_must_return_value_in_value_context() {
        // RET requires its child to produce a value. If we put an IRLIST that ends with LOG1(P0, P1) (no retval), parsing must fail.
        let bytes: Vec<u8> = vec![
            RET as u8,
            IRLIST as u8,
            0x00,
            0x01,
            LOG1 as u8,
            P0 as u8,
            P1 as u8,
        ];
        let mut seek = 0usize;
        assert!(parse_ir_node_one(&bytes, &mut seek).is_err());
    }

    #[test]
    fn multi_output_opcodes_are_rejected_by_irnode_parser() {
        for inst in [DUPN, REV] {
            let bytes: Vec<u8> = vec![inst as u8, 2];
            let mut seek = 0usize;
            let err = parse_ir_node_one(&bytes, &mut seek)
                .unwrap_err()
                .to_string();
            assert!(err.contains("multi-output"), "{}", err);
        }
    }

    #[test]
    fn dynamic_stack_tail_opcodes_are_rejected_outside_irlist() {
        let cases = [
            vec![POPN as u8, 1],
            vec![JOIN as u8, 3],
            vec![PACKLIST as u8],
            vec![PACKMAP as u8],
            vec![PACKTUPLE as u8],
        ];
        for bytes in cases {
            let mut seek = 0usize;
            let err = parse_ir_node_one(&bytes, &mut seek)
                .unwrap_err()
                .to_string();
            assert!(err.contains("dynamic stack input"), "{}", err);
        }
    }

    #[test]
    fn hidden_stack_input_opcodes_are_rejected_outside_stack_value_context() {
        // DUP / ROLL0 have no params and implicitly read top of stack — they
        // cannot stand alone as IR nodes in tree context. ROLL is excluded
        // here because it carries an explicit u8 stack offset and is
        // represented as IRNodeParam1.
        let cases = [vec![DUP as u8], vec![ROLL0 as u8]];
        for bytes in cases {
            let mut seek = 0usize;
            let err = parse_ir_node_one(&bytes, &mut seek)
                .unwrap_err()
                .to_string();
            assert!(err.contains("existing stack value"), "{}", err);
        }
    }

    #[test]
    fn param_prelude_roll0_shapes_remain_allowed() {
        let cases = [
            (
                vec![PUT as u8, 0, ROLL0 as u8],
                vec![ROLL0 as u8, PUT as u8, 0],
            ),
            (
                vec![UNPACK as u8, ROLL0 as u8, P0 as u8],
                vec![ROLL0 as u8, P0 as u8, UNPACK as u8],
            ),
        ];
        for (bytes, expected_codegen) in cases {
            let mut seek = 0usize;
            let node = parse_ir_node_one(&bytes, &mut seek).expect("parse param prelude");
            assert!(!node.hasretval());
            assert_eq!(seek, bytes.len());
            assert_eq!(node.codegen().unwrap(), expected_codegen);
        }
    }

    #[test]
    fn stack_value_context_only_allows_roll0() {
        // In a StackValue slot, only ROLL0 is permitted. DUP would create an
        // ambiguity about which existing stack value the surrounding op is
        // reading. ROLL is a regular IRNodeParam1 elsewhere; it isn't tested
        // here because it can't appear directly in a StackValue context.
        for inst in [DUP] {
            let bytes = vec![PUT as u8, 0, inst as u8];
            let mut seek = 0usize;
            let err = parse_ir_node_one(&bytes, &mut seek)
                .unwrap_err()
                .to_string();
            assert!(err.contains("existing stack value"), "{}", err);
        }
    }

    #[test]
    fn stack_value_context_is_not_available_to_local_ops() {
        for inst in [XOP, XLG] {
            let bytes = vec![inst as u8, 0, ROLL0 as u8];
            let mut seek = 0usize;
            let err = parse_ir_node_one(&bytes, &mut seek)
                .unwrap_err()
                .to_string();
            assert!(err.contains("existing stack value"), "{}", err);
        }
    }

    #[test]
    fn irlist_allows_stack_tail_pack_opcodes() {
        let bytes = vec![IRLIST as u8, 0, 3, P1 as u8, P1 as u8, PACKLIST as u8];
        let mut seek = 0usize;
        let node = parse_ir_node_one(&bytes, &mut seek).expect("parse packed IRLIST");
        assert!(node.hasretval());
        assert_eq!(seek, bytes.len());
        assert_eq!(
            node.codegen().unwrap(),
            vec![P1 as u8, P1 as u8, PACKLIST as u8]
        );
    }

    #[test]
    fn irlist_rejects_malformed_stack_tail_pack_opcodes() {
        let cases = [
            vec![IRLIST as u8, 0, 1, PACKLIST as u8],
            vec![IRLIST as u8, 0, 2, P2 as u8, PACKLIST as u8],
            vec![IRLIST as u8, 0, 3, P1 as u8, P2 as u8, PACKMAP as u8],
        ];
        for bytes in cases {
            let mut seek = 0usize;
            assert!(parse_ir_node_one(&bytes, &mut seek).is_err());
        }
    }

    #[test]
    fn pop_remains_allowed_for_empty_param_prelude() {
        let bytes = vec![POP as u8];
        let mut seek = 0usize;
        let node = parse_ir_node_one(&bytes, &mut seek).expect("parse POP prelude");
        assert!(!node.hasretval());
        assert_eq!(seek, bytes.len());
        assert_eq!(node.codegen().unwrap(), bytes);
    }

    #[test]
    fn log_metadata_matches_runtime_arity() {
        let cases = [
            (LOG1, 2usize),
            (LOG2, 3usize),
            (LOG3, 4usize),
            (LOG4, 5usize),
        ];
        for (inst, argc) in cases {
            assert_eq!(inst.metadata().input as usize, argc);
            let mut bytes = vec![inst as u8];
            bytes.extend(std::iter::repeat(P0 as u8).take(argc));
            let mut seek = 0usize;
            let parsed = parse_ir_node_one(&bytes, &mut seek).unwrap();
            assert_eq!(seek, bytes.len());
            assert_eq!(parsed.subs(), argc);
            assert!(!parsed.hasretval());
            assert_eq!(parsed.codegen().unwrap(), {
                let mut out = vec![P0 as u8; argc];
                out.push(inst as u8);
                out
            });
        }
    }

    #[test]
    fn fin_ir_serializes_without_irlist_wrapper() {
        let n = IRNodeParam1Triple {
            hrtv: true,
            inst: Bytecode::FIN3,
            para: fin_source_call_spec("mul_div_floor").unwrap().unwrap().id,
            subx: push_inst(Bytecode::P1),
            suby: push_inst(Bytecode::P2),
            subz: push_inst(Bytecode::P3),
        };
        let bytes = n.serialize();
        assert_eq!(bytes[0], Bytecode::FIN3 as u8);
        let mut seek = 0usize;
        let parsed = parse_ir_node_one(&bytes, &mut seek).unwrap();
        assert_eq!(seek, bytes.len());
        assert!(
            parsed
                .as_any()
                .downcast_ref::<IRNodeParam1Triple>()
                .is_some()
        );
    }

    #[test]
    fn malformed_fin_ir_is_rejected() {
        let bytes = vec![
            Bytecode::FIN4 as u8,
            fin_source_call_spec("mul_add_div_floor")
                .unwrap()
                .unwrap()
                .id,
            Bytecode::P1 as u8,
        ];
        let mut seek = 0usize;
        assert!(parse_ir_node_one(&bytes, &mut seek).is_err());
    }
}
