fn main() {
    #[cfg(feature = "source")]
    source_build::compile();
}

#[cfg(feature = "source")]
mod source_build {
    extern crate core;

    use cc::Build;
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use walkdir::WalkDir;

    /// Finds all files with an extension, ignoring some.
    ///
    /// TODO: If this gets more complicated, we might want to inline the list of files available in meson.
    fn glob_import<P: AsRef<Path>>(root: P, extenstion: &str, exclude: &str) -> Vec<String> {
        WalkDir::new(root)
            .into_iter()
            .map(|x| x.unwrap())
            .filter(|x| x.path().to_str().unwrap().ends_with(extenstion))
            .map(|x| x.path().to_str().unwrap().to_string())
            .filter(|x| !x.contains(exclude))
            .collect()
    }

    #[derive(Debug, Copy, Clone)]
    enum TargetFamily {
        Unix,
        Windows,
        Wasm,
    }
    #[derive(Debug, Copy, Clone)]
    enum TargetOs {
        Windows,
        Macos,
        Ios,
        Linux,
        Android,
        Freebsd,
        Dragonfly,
        Openbsd,
        Netbsd,
    }
    #[derive(Debug, Copy, Clone)]
    enum TargetArch {
        X86,
        X86_64,
        Arm,
        Aarch64,
        Mips,
        Powerpc,
        Powerpc64,
        Riscv64,
        S390x,
        Wasm32,
    }
    #[derive(Debug, Copy, Clone)]
    enum TargetEnv {
        NoEnv,
        Gnu,
        Msvc,
        Musl,
        Sim,
        Sgx,
    }

    #[derive(Debug, Copy, Clone)]
    struct Target {
        family: TargetFamily,
        os: TargetOs,
        arch: TargetArch,
        env: TargetEnv,
    }

    impl Target {
        fn from_env() -> Self {
            let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap();
            let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
            let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
            let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap();

            let family = match family.as_str() {
                "unix" => TargetFamily::Unix,
                "windows" => TargetFamily::Windows,
                "wasm" => TargetFamily::Wasm,
                _ => panic!("Unknown target family: {family}"),
            };
            let os = match os.as_str() {
                "windows" => TargetOs::Windows,
                "macos" => TargetOs::Macos,
                "ios" => TargetOs::Ios,
                "linux" => TargetOs::Linux,
                "android" => TargetOs::Android,
                "freebsd" => TargetOs::Freebsd,
                "dragonfly" => TargetOs::Dragonfly,
                "openbsd" => TargetOs::Openbsd,
                "netbsd" => TargetOs::Netbsd,
                _ => panic!("Unknown target os: {os}"),
            };
            let arch = match arch.as_str() {
                "x86" => TargetArch::X86,
                "x86_64" => TargetArch::X86_64,
                "arm" => TargetArch::Arm,
                "aarch64" => TargetArch::Aarch64,
                "mips" => TargetArch::Mips,
                "powerpc" => TargetArch::Powerpc,
                "powerpc64" => TargetArch::Powerpc64,
                "riscv64" => TargetArch::Riscv64,
                "s390x" => TargetArch::S390x,
                "wasm32" => TargetArch::Wasm32,
                _ => panic!("Unknown target arch: {arch}"),
            };
            let env = match env.as_str() {
                "" => TargetEnv::NoEnv,
                "gnu" => TargetEnv::Gnu,
                "msvc" => TargetEnv::Msvc,
                "musl" => TargetEnv::Musl,
                "sim" => TargetEnv::Sim,
                "sgx" => TargetEnv::Sgx,
                _ => panic!("Unknown target env: {env}"),
            };

            Self { family, os, arch, env }
        }
    }

    #[derive(Debug)]
    struct NasmConfiguration {
        /// Extension for assembly files
        asm_extension: String,
        /// Directory to add to asm include path
        include_dir: String,
        /// Exclude asm files from compilation
        /// Used to prevent compilation of assembly files included by other assembly files
        asm_exclude: String,
        /// A C++ define used to identify the assembly platform
        cpp_define: String,
        /// An assembly define used to select sub-platforms
        asm_platform_define: String,
        /// Whether to prefix symbols with an underscore
        prefix_symbols: bool,
    }

    impl NasmConfiguration {
        fn find(target: Target) -> Option<Self> {
            use TargetArch::*;
            use TargetFamily::*;
            use TargetOs::*;

            match target.arch {
                X86_64 | X86 => {}
                _ => return None,
            }

            // this function basically repeats what is done across several included makefiles in the `build` directory of the upstream

            let asm_extension = match target.arch {
                X86_64 | X86 => ".asm",
                _ => return None,
            };

            let asm_dir = match target.arch {
                X86_64 | X86 => "x86",
                _ => return None,
            };

            let asm_exclude = match target.arch {
                X86_64 | X86 => "asm_inc.asm",
                _ => return None,
            };

            // CPP defines to inform which assembly symbols to use.
            let cpp_define = match target.arch {
                X86_64 | X86 => "X86_ASM",
                _ => return None,
            };

            // A special define needed for some platforms.
            let asm_platform_define = match target.arch {
                X86_64 => match target.family {
                    Unix => "UNIX64",
                    TargetFamily::Windows => "WIN64",
                    _ => return None,
                },
                X86 => "X86_32",
                _ => return None,
            };

            // Prefix symbols exported from assembly with an underscore on x86/x86_64 macOS and x86 Windows.
            let prefix_underscores = matches!(
                target,
                Target {
                    os: Macos,
                    arch: X86_64 | X86,
                    ..
                } | Target {
                    os: TargetOs::Windows,
                    arch: X86,
                    ..
                }
            );

            Some(Self {
                asm_extension: asm_extension.to_string(),
                include_dir: asm_dir.to_string(),
                asm_exclude: asm_exclude.to_string(),
                cpp_define: cpp_define.to_string(),
                asm_platform_define: asm_platform_define.to_string(),
                prefix_symbols: prefix_underscores,
            })
        }
    }

    fn nasm_object_paths(component: &str, sources: &[String], out_dir: &Path) -> Result<Vec<PathBuf>, String> {
        if !matches!(component, "common" | "processing" | "decoder" | "encoder") {
            return Err(format!("Unknown NASM component: {component}"));
        }

        let mut names = BTreeSet::new();
        let mut expected = Vec::with_capacity(sources.len());
        for source in sources {
            let filename = Path::new(source)
                .file_name()
                .ok_or_else(|| format!("Invalid NASM source path: {source}"))?;
            let object_name = Path::new(filename).with_extension("o");
            let name = object_name
                .to_str()
                .ok_or_else(|| format!("Invalid NASM object name for {source}"))?
                .to_lowercase();
            // Be conservative on every host so Windows cannot overwrite case-only duplicates.
            if !names.insert(name) {
                return Err(format!("NASM output-name collision in component {component}: {source}"));
            }
            expected.push(out_dir.join(object_name));
        }
        // Renaming must not overwrite another selected source's unprefixed object.
        if names.iter().any(|name| names.contains(&format!("{component}-{name}"))) {
            return Err(format!("NASM rename collision in component {component}"));
        }
        Ok(expected)
    }

    fn prepare_nasm_objects(component: &str, expected: &[PathBuf], returned: Vec<PathBuf>) -> Result<Vec<PathBuf>, String> {
        let expected_set: BTreeSet<_> = expected.iter().collect();
        let returned_set: BTreeSet<_> = returned.iter().collect();
        if expected.len() != returned.len() || expected_set.len() != expected.len() || expected_set != returned_set {
            return Err(format!(
                "Incomplete, duplicate or unexpected NASM outputs for component {component}"
            ));
        }

        // Validate the entire result before renaming or adding anything to cc.
        let mut prepared = Vec::with_capacity(returned.len());
        for object in &returned {
            let metadata =
                std::fs::metadata(object).map_err(|error| format!("Missing NASM object {}: {error}", object.display()))?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(format!("Invalid NASM object: {}", object.display()));
            }
            let filename = object
                .file_name()
                .ok_or_else(|| format!("Invalid NASM object path: {}", object.display()))?;
            let mut name = OsString::from(format!("{component}-"));
            name.push(filename);
            prepared.push(object.with_file_name(name));
        }
        for (object, renamed) in returned.iter().zip(&prepared) {
            std::fs::rename(object, renamed).map_err(|error| {
                format!(
                    "Cannot rename NASM object {} to {}: {error}",
                    object.display(),
                    renamed.display()
                )
            })?;
        }
        Ok(prepared)
    }

    #[allow(unused)]
    fn try_compile_nasm(target: &Target, cc_build_command: &mut Build, root: &str, component: &str) -> Result<(), String> {
        if std::env::var("OPENH264_NO_ASM").is_ok() {
            println!("NASM compilation disabled by environment variable.");
            return Ok(());
        }

        let Some(config) = NasmConfiguration::find(*target) else {
            println!("No NASM configuration found for target, not using any assembly.\nTarget: {target:?}");
            return Ok(());
        };

        let sources = glob_import(root, &config.asm_extension, &config.asm_exclude);
        let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR must be set for NASM")?)
            .join("nasm")
            .join(component);
        let expected = nasm_object_paths(component, &sources, &out_dir)?;

        // Try to compile NASM targets
        let mut nasm_build = nasm_rs::Build::new();
        nasm_build.out_dir(&out_dir);
        let mut nasm_build = nasm_build.include(format!("upstream/codec/common/{}/", config.include_dir));
        nasm_build = nasm_build.define(&config.asm_platform_define, None);
        if config.prefix_symbols {
            nasm_build = nasm_build.define("PREFIX", None);
        }

        let object_files = nasm_build.files(&sources).compile_objects()?;
        let object_files = prepare_nasm_objects(component, &expected, object_files)?;

        // This here only _EXTENDS_ the build command we got passed, it doesn't
        // _RUN_ any build command on its own (we still invoked `nasm` above
        // though).
        cc_build_command.define(&config.cpp_define, None);

        for object in &object_files {
            cc_build_command.object(object);
        }
        Ok(())
    }

    /// Builds an OpenH264 sub-library and adds it to the project.
    fn compile_and_add_openh264_static_lib(target: &Target, name: &str, root: &str, suffix: &str, includes: &[&str]) {
        let mut cc_build = cc::Build::new();

        if let Err(error) = try_compile_nasm(target, &mut cc_build, root, name) {
            panic!("NASM failed for component {name}: {error}");
        }

        cc_build
            .include("upstream/codec/api/wels/")
            .include("upstream/codec/common/inc/")
            .cpp(true)
            .warnings(false)
            .files(glob_import(root, ".cpp", "DllEntry.cpp")) // Otherwise fails when compiling on Linux
            .pic(true)
            // Upstream sets these two and if we don't we get segmentation faults on Linux and MacOS ... Happy times.
            .flag_if_supported("-fno-strict-aliasing")
            .flag_if_supported("-fembed-bitcode")
            .flag_if_supported("-fno-common")
            .flag_if_supported("-undefined dynamic_lookup");

        // disable stack protectors on mingw:
        // (seems to be the way to go https://github.com/rdp/ffmpeg-windows-build-helpers/issues/380)

        if !matches!(target.os, TargetOs::Windows) || !matches!(target.env, TargetEnv::Gnu) {
            cc_build.flag_if_supported("-fstack-protector-all");
        }

        // cl.exe cannot assemble .S files.
        // TODO: generalize try_compile_nasm and invoke armasm64.exe.
        if !matches!(target.os, TargetOs::Windows) {
            match target.arch {
                TargetArch::Arm => {
                    cc_build.define("HAVE_NEON", None);
                    cc_build.include("upstream/codec/common/arm");
                    cc_build.files(glob_import(
                        Path::new(root).join(suffix).join("arm"),
                        ".S",
                        "arm_arch_common_macro.S",
                    ));
                }
                TargetArch::Aarch64 => {
                    cc_build.define("HAVE_NEON_AARCH64", None);
                    cc_build.include("upstream/codec/common/arm64");
                    cc_build.files(glob_import(
                        Path::new(root).join(suffix).join("arm64"),
                        ".S",
                        "arm_arch64_common_macro.S",
                    ));
                }
                _ => {}
            }
        }

        for include in includes {
            cc_build.include(include);
        }

        cc_build.compile(format!("openh264_{name}").as_str());

        println!("cargo:rustc-link-lib=static=openh264_{name}");
    }

    pub fn compile() {
        let target = Target::from_env();

        compile_and_add_openh264_static_lib(&target, "common", "upstream/codec/common", ".", &[]);

        compile_and_add_openh264_static_lib(
            &target,
            "processing",
            "upstream/codec/processing",
            "src",
            &[
                "upstream/codec/processing/src/common/",
                "upstream/codec/processing/interface/",
            ],
        );

        // #[cfg(feature = "decoder")]
        compile_and_add_openh264_static_lib(
            &target,
            "decoder",
            "upstream/codec/decoder",
            "core",
            &["upstream/codec/decoder/core/inc/", "upstream/codec/decoder/plus/inc/"],
        );

        // #[cfg(feature = "encoder")]
        compile_and_add_openh264_static_lib(
            &target,
            "encoder",
            "upstream/codec/encoder",
            "core",
            &[
                "upstream/codec/encoder/core/inc/",
                "upstream/codec/encoder/plus/inc/",
                "upstream/codec/processing/interface/",
            ],
        );
    }

    #[cfg(test)]
    include!("tests/nasm-caller/cases.rs");
}
