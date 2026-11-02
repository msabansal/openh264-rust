#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    fn target() -> Target {
        Target {
            family: TargetFamily::Unix,
            os: TargetOs::Linux,
            arch: TargetArch::X86_64,
            env: TargetEnv::Gnu,
        }
    }

    fn fixture(name: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name)
            .to_str()
            .unwrap()
            .to_string()
    }

    fn output() -> PathBuf {
        PathBuf::from(std::env::var_os("OUT_DIR").expect("Set OUT_DIR to a fresh qualification directory"))
    }

    fn archive(name: &str, build: &mut Build) {
        build
            .cargo_metadata(false)
            .target("x86_64-unknown-linux-gnu")
            .host("x86_64-unknown-linux-gnu")
            .out_dir(output())
            .opt_level(0)
            .debug(false)
            .compile(name);
    }

    fn run(command: &mut Command) -> std::process::Output {
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{command:?}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        result
    }

    #[test]
    fn components_survive_archive_merge_and_link() {
        assert!(std::env::var_os("OPENH264_NO_ASM").is_none());
        fs::create_dir_all(output()).unwrap();
        let mut common = Build::new();
        let mut encoder = Build::new();
        try_compile_nasm(&target(), &mut common, &fixture("common"), "common").unwrap();
        try_compile_nasm(&target(), &mut encoder, &fixture("encoder"), "encoder").unwrap();
        archive("caller_common", &mut common);
        archive("caller_encoder", &mut encoder);

        let merged = output().join("merged.a");
        let script = format!(
            "CREATE {}\nADDLIB {}\nADDLIB {}\nSAVE\nEND\n",
            merged.display(),
            output().join("libcaller_common.a").display(),
            output().join("libcaller_encoder.a").display()
        );
        let mut merger = Command::new("ar")
            .arg("-M")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            merger.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
        }
        let result = merger.wait_with_output().unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let listing = run(Command::new("ar").arg("t").arg(&merged));
        let mut members: Vec<_> = String::from_utf8(listing.stdout)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        members.sort();
        assert_eq!(members, ["common-dct.o", "common-extra.o", "encoder-dct.o", "encoder-extra.o"]);

        let executable = output().join("collision-link");
        run(Command::new("cc")
            .arg(fixture("main.c"))
            .arg(&merged)
            .arg("-o")
            .arg(&executable));
        run(&mut Command::new(executable));
    }

    #[test]
    fn same_component_collisions_are_rejected_before_assembly() {
        let out_dir = output().join("collision");
        for sources in [
            vec!["one/dct.asm".to_string(), "two/dct.asm".to_string()],
            vec!["one/dct.asm".to_string(), "two/DCT.asm".to_string()],
            vec!["one/Ä.asm".to_string(), "two/ä.asm".to_string()],
            vec!["dct.asm".to_string(), "common-dct.asm".to_string()],
        ] {
            let error = nasm_object_paths("common", &sources, &out_dir).unwrap_err();
            assert!(error.contains("collision"), "{error}");
            assert!(!out_dir.exists());
        }
        let error = try_compile_nasm(&target(), &mut Build::new(), &fixture("collision"), "processing").unwrap_err();
        assert!(error.contains("collision"), "{error}");
        assert!(!output().join("nasm/processing").exists());
    }

    #[test]
    fn malformed_results_cannot_prepare_objects_for_archiving() {
        let out_dir = output().join("malformed");
        fs::create_dir_all(&out_dir).unwrap();
        let dct = out_dir.join("dct.o");
        let aux = out_dir.join("extra.o");
        let rogue = out_dir.join("rogue.o");
        let outside = output().join("outside/extra.o");
        fs::create_dir_all(outside.parent().unwrap()).unwrap();
        fs::write(&dct, b"object present").unwrap();
        fs::write(&rogue, b"unexpected object").unwrap();
        fs::write(&outside, b"object outside the component directory").unwrap();
        let expected = vec![dct.clone(), aux.clone()];

        for returned in [
            vec![dct.clone()],
            vec![dct.clone(), dct.clone()],
            vec![dct.clone(), rogue],
            vec![dct.clone(), outside],
            vec![dct.clone(), aux.clone()],
        ] {
            assert!(prepare_nasm_objects("common", &expected, returned).is_err());
            assert!(dct.exists());
            assert!(!out_dir.join("common-dct.o").exists());
        }
        fs::write(&aux, []).unwrap();
        assert!(prepare_nasm_objects("common", &expected, expected.clone()).is_err());
        assert!(dct.exists());
        assert!(!out_dir.join("common-dct.o").exists());
    }

    #[test]
    fn rename_failure_cannot_return_archive_inputs() {
        let out_dir = output().join("rename-failure");
        fs::create_dir_all(out_dir.join("common-dct.o")).unwrap();
        let object = out_dir.join("dct.o");
        fs::write(&object, b"object present").unwrap();
        assert!(prepare_nasm_objects("common", &[object.clone()], vec![object.clone()]).is_err());
        assert!(object.exists());
    }

    #[test]
    fn invalid_assembly_cannot_reach_the_native_archive() {
        assert!(std::env::var_os("OPENH264_NO_ASM").is_none());
        let failure = std::panic::catch_unwind(|| {
            compile_and_add_openh264_static_lib(&target(), "decoder", &fixture("invalid"), ".", &[]);
        })
        .expect_err("Bad assembly must fail the production build, not disable X86_ASM");
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(message.contains("NASM failed for component decoder"), "{message}");
        assert!(!output().join("libopenh264_decoder.a").exists());
        assert!(!output().join("nasm/decoder/decoder-ok.o").exists());
    }

    #[test]
    fn unsupported_architecture_still_skips_nasm() {
        let mut unsupported = target();
        unsupported.arch = TargetArch::Aarch64;
        let mut build = Build::new();
        try_compile_nasm(&unsupported, &mut build, &fixture("invalid"), "processing").unwrap();
        assert!(!output().join("nasm/processing").exists());
    }

    #[test]
    #[ignore = "Run separately with OPENH264_NO_ASM=1"]
    fn explicit_no_asm_still_skips_nasm() {
        assert!(std::env::var_os("OPENH264_NO_ASM").is_some());
        let mut build = Build::new();
        try_compile_nasm(&target(), &mut build, &fixture("invalid"), "processing").unwrap();
        assert!(!output().join("nasm/processing").exists());
    }
}
