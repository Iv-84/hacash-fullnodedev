//! Doc coverage checks for gas-cost.md.
//!
//! Goal: keep gas-cost.md aligned with current runtime billing behavior and
//! prevent legacy terms/omissions from creeping back.

const DOC: &str = include_str!("../doc/gas-cost.md");

#[test]
fn doc_tracks_current_dynamic_metering_groups() {
    // stack-copy related
    for key in [
        "DUPN",
        "GETX",
        "PBUF",
        "PBUFL",
        "PUT, PUTX, MPUT, GPUT",
        "byte/28",
        "stack_write_div",
        "CAT, JOIN, BYTE, CUT, LEFT, RIGHT, LDROP, RDROP",
        "SWAP moves 2",
        "CHOOSE moves 2",
        "EQ, NEQ, XLG (`==` / `!=` only)",
        "stack_cmp_div",
        "Ordered comparisons LT, GT, LE, GE",
    ] {
        assert!(DOC.contains(key), "missing stack-copy coverage key: {key}");
    }

    // action/native related
    for key in [
        "NTFUNC input argv bytes",
        "NTENV return",
        "NTCTL return",
        "does not separately byte-meter the NTCTL input",
        "ACTVIEW input body bytes",
        "ACTENV return",
        "ACTION input body bytes",
        "host-returned gas (`bgasu`)",
    ] {
        assert!(
            DOC.contains(key),
            "missing action/native coverage key: {key}"
        );
    }

    // storage/space/manual sections
    for key in [
        "1024: every newly created persistent storage key",
        "SSTAT: base 32 only",
        "SDEL: base 28 only",
        "SEDIT: base 64",
        "SPUT: base 128",
        "SGET: base 64",
        "every cold contract load",
        "BURN: default base gas 1",
        "IR format fee",
        "raw serialized IR byte length",
        "Charging point: frame entry",
        "compiled code only",
        "Coverage Notes",
        "UNPACK (source container",
    ] {
        assert!(DOC.contains(key), "missing section key: {key}");
    }
}

#[test]
fn doc_does_not_use_legacy_opcode_names() {
    assert!(
        !DOC.contains("NTCALL"),
        "legacy name NTCALL should not appear in doc"
    );
    assert!(
        !DOC.contains("EXTFUNC"),
        "legacy name EXTFUNC should not appear in doc"
    );
    assert!(
        !DOC.contains("`EXTVIEW`") && !DOC.contains(" EXTVIEW"),
        "legacy name EXTVIEW should not appear in doc"
    );
    assert!(
        !DOC.contains("EXTENV"),
        "legacy name EXTENV should not appear in doc"
    );
    assert!(
        !DOC.contains("EXTACTION"),
        "legacy name EXTACTION should not appear in doc"
    );
    assert!(
        !DOC.contains("SSAVE"),
        "legacy name SSAVE should not appear in doc"
    );
    assert!(
        !DOC.contains("SREST"),
        "legacy name SREST should not appear in doc"
    );
    assert!(
        !DOC.contains("BURN(compile_fee)"),
        "legacy IR compile-fee BURN prefix should not appear in doc"
    );
    assert!(
        !DOC.contains("runtime-appended `END`"),
        "legacy IR runtime-appended END wording should not appear in doc"
    );
}
