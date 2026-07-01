mod fitshc_example_compile {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn compile_example_fitsh_via_fitshc() {
        // Keep this fixture self-contained so parser/runtime surface changes in docs
        // do not break the fitshc integration smoke test.
        let src = r##"
pragma fitsh 1.0.0

contract Example {
    function external ping() -> u32 {
        var key = "key"
        var val = storage_load(key)
        if val is nil {
            storage_new(key, 1, 100)
        } else {
            storage_edit(key, 1)
        }
        return 0
    }
}
"##;

        let mut dir = std::env::temp_dir();
        dir.push("hacash_fitshc_tests");
        let _ = fs::create_dir_all(&dir);

        let tmp_fitsh = dir.join("example.fitsh");
        fs::write(&tmp_fitsh, src)
            .unwrap_or_else(|e| panic!("failed to write {}: {}", tmp_fitsh.display(), e));

        // fitshc writes outputs next to the input file.
        let expected_map = dir.join("example.contractmap.json");
        let expected_deploy = dir.join("example.deploy.json");
        let _ = fs::remove_file(&expected_map);
        let _ = fs::remove_file(&expected_deploy);

        let exe = PathBuf::from(env!("CARGO_BIN_EXE_fitshc"));
        let out = Command::new(&exe)
            .arg(&tmp_fitsh)
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {}", exe.display(), e));

        if !out.status.success() {
            panic!(
                "fitshc failed (status={})\n---- stdout ----\n{}\n---- stderr ----\n{}\n",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr),
            );
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Compile success!"),
            "stdout was: {}",
            stdout
        );
        assert!(expected_map.exists(), "missing {}", expected_map.display());
        assert!(
            expected_deploy.exists(),
            "missing {}",
            expected_deploy.display()
        );
    }
}
