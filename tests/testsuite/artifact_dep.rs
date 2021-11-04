use cargo_test_support::registry::Package;
use cargo_test_support::{basic_bin_manifest, basic_manifest, project, publish, registry, Project};

#[cargo_test]
fn check_with_invalid_artifact_dependency() {
    // invalid name
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "unknown" }
            "#,
        )
        .file("src/lib.rs", "extern crate bar;") // this would fail but we don't get there, artifacts are no libs
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[ERROR] failed to parse manifest at `[..]/Cargo.toml`

Caused by:
  'unknown' is not a valid artifact specifier.
",
        )
        .with_status(101)
        .run();

    // lib specified without artifact
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", lib = true }
            "#,
        )
        .file("src/lib.rs", "")
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[ERROR] failed to parse manifest at `[..]/Cargo.toml`

Caused by:
  'lib' specifier cannot be used without an 'artifact = …' value
",
        )
        .with_status(101)
        .run();
}

#[cargo_test]
fn build_without_nightly_shows_warnings_and_ignores_them() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin" }
            "#,
        )
        .file("src/lib.rs", "extern crate bar;")
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("check")
        .with_stderr(
            "\
[WARNING] `artifact = [..]` ignored for dependency `bar` as `-Z bindeps` is not set.
[CHECKING] bar [..]
[CHECKING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();

    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", lib = false }
            "#,
        )
        .file("src/lib.rs", "extern crate bar;") // this would fail if artifacts are available as these aren't libs by default
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("check")
        .with_stderr(
            "\
[WARNING] `lib` specifiers need an `artifact = …` value and would fail the operation when `-Z bindeps` is provided.
[CHECKING] bar [..]
[CHECKING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();
}

#[cargo_test]
fn disallow_artifact_and_no_artifact_dep_to_same_package_within_the_same_dep_category() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin" }
                bar_stable = { path = "bar/", package = "bar" }
            "#,
        )
        .file("src/lib.rs", "")
        .file("bar/Cargo.toml", &basic_bin_manifest("bar"))
        .file("bar/src/main.rs", "fn main() {}")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_status(101)
        .with_stderr(
            "[ERROR] the crate `foo v0.0.0 ([CWD])` depends on crate `bar v0.5.0 ([CWD]/bar)` multiple times with different names",
        )
        .run();
}

#[cargo_test]
fn build_script_with_bin_artifacts() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [build-dependencies]
                bar = { path = "bar/", artifact = ["bin", "staticlib", "cdylib"] }
            "#,
        )
        .file("src/lib.rs", "")
        .file("build.rs", r#"
            fn main() {
                let baz: std::path::PathBuf = std::env::var("CARGO_BIN_FILE_BAR_baz").expect("CARGO_BIN_FILE_BAR_baz").into();
                println!("{}", baz.display());
                assert!(&baz.is_file()); 
                
                let lib: std::path::PathBuf = std::env::var("CARGO_STATICLIB_FILE_BAR_bar").expect("CARGO_STATICLIB_FILE_BAR_bar").into();
                println!("{}", lib.display());
                assert!(&lib.is_file()); 
                
                let lib: std::path::PathBuf = std::env::var("CARGO_CDYLIB_FILE_BAR_bar").expect("CARGO_CDYLIB_FILE_BAR_bar").into();
                println!("{}", lib.display());
                assert!(&lib.is_file()); 
                
                let dir: std::path::PathBuf = std::env::var("CARGO_BIN_DIR_BAR").expect("CARGO_BIN_DIR_BAR").into();
                println!("{}", dir.display());
                assert!(dir.is_dir());
                
                let bar: std::path::PathBuf = std::env::var("CARGO_BIN_FILE_BAR").expect("CARGO_BIN_FILE_BAR").into();
                println!("{}", bar.display());
                assert!(&bar.is_file()); 
                
                let bar2: std::path::PathBuf = std::env::var("CARGO_BIN_FILE_BAR_bar").expect("CARGO_BIN_FILE_BAR_bar").into();
                println!("{}", bar2.display());
                assert_eq!(bar, bar2);
            }
        "#)
        .file(
            "bar/Cargo.toml",
            r#"
                [package]
                name = "bar"
                version = "0.5.0"
                authors = []
                
                [lib]
                crate-type = ["staticlib", "cdylib"]
            "#,
        )
        .file("bar/src/bin/bar.rs", "fn main() {}")
        .file("bar/src/bin/baz.rs", "fn main() {}")
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr_contains("[COMPILING] foo [..]")
        .with_stderr_contains("[COMPILING] bar v0.5.0 ([CWD]/bar)")
        .with_stderr_contains("[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]")
        .run();

    let build_script_output = build_script_output_string(&p, "foo");
    let msg = "we need the binary directory for this artifact along with all binary paths";
    #[cfg(any(not(windows), target_env = "gnu"))]
    {
        cargo_test_support::compare::match_exact(
            "[..]/artifact/bar-[..]/bin/baz-[..]\n\
             [..]/artifact/bar-[..]/staticlib/libbar-[..].a\n\
             [..]/artifact/bar-[..]/cdylib/[..]bar.[..]\n\
             [..]/artifact/bar-[..]/bin\n\
             [..]/artifact/bar-[..]/bin/bar-[..]\n\
             [..]/artifact/bar-[..]/bin/bar-[..]",
            &build_script_output,
            msg,
            "",
            None,
        )
        .unwrap();
    }
    #[cfg(all(windows, not(target_env = "gnu")))]
    {
        cargo_test_support::compare::match_exact(
            "[..]/artifact/bar-[..]/bin/baz.exe\n\
             [..]/artifact/bar-[..]/staticlib/bar-[..].lib\n\
             [..]/artifact/bar-[..]/cdylib/bar.dll\n\
             [..]/artifact/bar-[..]/bin\n\
             [..]/artifact/bar-[..]/bin/bar.exe\n\
             [..]/artifact/bar-[..]/bin/bar.exe",
            &build_script_output,
            msg,
            "",
            None,
        )
        .unwrap();
    }

    assert!(
        !p.bin("bar").is_file(),
        "artifacts are located in their own directory, exclusively, and won't be lifted up"
    );
    assert!(!p.bin("baz").is_file(),);
    assert_artifact_executable_output(&p, "debug", "bar", "bar");
}

#[cargo_test]
fn build_script_with_bin_artifact_and_lib_false() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [build-dependencies]
                bar = { path = "bar/", artifact = "bin" }
            "#,
        )
        .file("src/lib.rs", "")
        .file(
            "build.rs",
            r#"
            fn main() {
               bar::doit()
            }
        "#,
        )
        .file("bar/Cargo.toml", &basic_bin_manifest("bar"))
        .file("bar/src/main.rs", "fn main() { bar::doit(); }")
        .file(
            "bar/src/lib.rs",
            r#"
            pub fn doit() {
               panic!("cannot be called from build script due to lib = false");
            }
        "#,
        )
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_status(101)
        .with_stderr_contains(
            "error[E0433]: failed to resolve: use of undeclared crate or module `bar`",
        )
        .with_stderr_contains(" --> build.rs:3:16")
        .run();
}

#[cargo_test]
fn lib_with_bin_artifact_and_lib_false() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin" }
            "#,
        )
        .file(
            "src/lib.rs",
            r#"
            fn main() {
               bar::doit()
            }"#,
        )
        .file("bar/Cargo.toml", &basic_bin_manifest("bar"))
        .file("bar/src/main.rs", "fn main() { bar::doit(); }")
        .file(
            "bar/src/lib.rs",
            r#"
            pub fn doit() {
               panic!("cannot be called from library due to lib = false");
            }
        "#,
        )
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_status(101)
        .with_stderr_contains(
            "error[E0433]: failed to resolve: use of undeclared crate or module `bar`",
        )
        .with_stderr_contains(" --> src/lib.rs:3:16")
        .run();
}

#[cargo_test]
fn build_script_with_selected_dashed_bin_artifact_and_lib_true() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [build-dependencies]
                bar-baz = { path = "bar/", artifact = "bin:baz-suffix", lib = true }
            "#,
        )
        .file("src/lib.rs", "")
        .file("build.rs", r#"
            fn main() {
               bar_baz::print_env()
            }
        "#)
        .file(
            "bar/Cargo.toml",
            r#"
                [package]
                name = "bar-baz"
                version = "0.5.0"
                authors = []
                
                [[bin]]
                name = "bar"
                
                [[bin]]
                name = "baz-suffix"
            "#,
        )
        .file("bar/src/main.rs", "fn main() {}")
        .file("bar/src/lib.rs", r#"
            pub fn print_env() {
                let dir: std::path::PathBuf = std::env::var("CARGO_BIN_DIR_BAR_BAZ").expect("CARGO_BIN_DIR_BAR_BAZ").into();
                let bin: std::path::PathBuf = std::env::var("CARGO_BIN_FILE_BAR_BAZ_baz-suffix").expect("CARGO_BIN_FILE_BAR_BAZ_baz-suffix").into();
                println!("{}", dir.display());
                println!("{}", bin.display());
                assert!(dir.is_dir());
                assert!(&bin.is_file());
                assert!(std::env::var("CARGO_BIN_FILE_BAR_BAZ").is_err(), "CARGO_BIN_FILE_BAR_BAZ isn't set due to name mismatch");
                assert!(std::env::var("CARGO_BIN_FILE_BAR_BAZ_bar").is_err(), "CARGO_BIN_FILE_BAR_BAZ_bar isn't set as binary isn't selected");
            }
        "#)
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar-baz v0.5.0 ([CWD]/bar)
[COMPILING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]",
        )
        .run();

    let build_script_output = build_script_output_string(&p, "foo");
    let msg = "we need the binary directory for this artifact and the binary itself";

    #[cfg(any(not(windows), target_env = "gnu"))]
    {
        cargo_test_support::compare::match_exact(
            "[..]/artifact/bar-baz-[..]/bin\n\
        [..]/artifact/bar-baz-[..]/bin/baz_suffix-[..]",
            &build_script_output,
            msg,
            "",
            None,
        )
        .unwrap();
    }
    #[cfg(all(windows, not(target_env = "gnu")))]
    {
        cargo_test_support::compare::match_exact(
            &format!(
                "[..]/artifact/bar-baz-[..]/bin\n\
                 [..]/artifact/bar-baz-[..]/bin/baz_suffix{}",
                std::env::consts::EXE_SUFFIX,
            ),
            &build_script_output,
            msg,
            "",
            None,
        )
        .unwrap();
    }

    assert!(
        !p.bin("bar").is_file(),
        "artifacts are located in their own directory, exclusively, and won't be lifted up"
    );
    assert_artifact_executable_output(&p, "debug", "bar", "baz_suffix");
}

#[cargo_test]
fn lib_with_selected_dashed_bin_artifact_and_lib_true() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar-baz = { path = "bar/", artifact = ["bin:baz-suffix", "staticlib", "cdylib"], lib = true }
            "#,
        )
        .file(
            "src/lib.rs",
            r#"
            pub fn foo() {
                bar_baz::exists();
                
                env!("CARGO_BIN_DIR_BAR_BAZ");
                let _b = include_bytes!(env!("CARGO_BIN_FILE_BAR_BAZ_baz-suffix"));
                let _b = include_bytes!(env!("CARGO_STATICLIB_FILE_BAR_BAZ"));
                let _b = include_bytes!(env!("CARGO_STATICLIB_FILE_BAR_BAZ_bar-baz"));
                let _b = include_bytes!(env!("CARGO_CDYLIB_FILE_BAR_BAZ"));
                let _b = include_bytes!(env!("CARGO_CDYLIB_FILE_BAR_BAZ_bar-baz"));
            }
        "#,
        )
        .file(
            "bar/Cargo.toml",
            r#"
                [package]
                name = "bar-baz"
                version = "0.5.0"
                authors = []
                
                [lib]
                crate-type = ["rlib", "staticlib", "cdylib"]
                
                [[bin]]
                name = "bar"
                
                [[bin]]
                name = "baz-suffix"
            "#,
        )
        .file("bar/src/main.rs", "fn main() {}")
        .file("bar/src/lib.rs", "pub fn exists() {}")
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar-baz v0.5.0 ([CWD]/bar)
[COMPILING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]",
        )
        .run();

    assert!(
        !p.bin("bar").is_file(),
        "artifacts are located in their own directory, exclusively, and won't be lifted up"
    );
    assert_artifact_executable_output(&p, "debug", "bar", "baz_suffix");
}

#[cargo_test]
fn allow_artifact_and_no_artifact_dep_to_same_package_within_different_dep_categories() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin" }
                
                [dev-dependencies]
                bar = { path = "bar/", package = "bar" }
            "#,
        )
        .file(
            "src/lib.rs",
            r#"
            pub fn foo() {
                env!("CARGO_BIN_DIR_BAR");
                let _b = include_bytes!(env!("CARGO_BIN_FILE_BAR"));
            }"#,
        )
        .file("bar/Cargo.toml", &basic_bin_manifest("bar"))
        .file("bar/src/main.rs", "fn main() {}")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr_contains("[COMPILING] bar v0.5.0 ([CWD]/bar)")
        .with_stderr_contains("[CHECKING] foo [..]")
        .with_stderr_contains("[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]")
        .run();
}

#[cargo_test]
#[ignore]
fn disallow_using_example_binaries_as_artifacts() {}

#[cargo_test]
#[ignore]
fn disallow_dep_renames_with_multiple_versions() {
    Package::new("bar", "1.0.0").publish();

    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin" }
                bar_stable = { package = "bar", version = "1.0.0", artifact = "bin" }
            "#,
        )
        .file("src/lib.rs", "") // this would fail if artifacts are available as these aren't libs by default
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_status(101)
        .with_stderr(
            "\
[UPDATING] [..]
[DOWNLOADING] crates ...
[DOWNLOADED] bar v1.0.0 [..]
[CHECKING] bar [..]
[CHECKING] bar v1.0.0
[CHECKING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();
}

#[cargo_test]
#[ignore]
fn crate_renames_affect_the_artifact_dependency_name_and_multiple_names_are_allowed() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar_renamed = { path = "bar/", artifact = "bin", package = "bar" }
                bar =         { path = "bar/", artifact = "bin" }
            "#,
        )
        .file("src/lib.rs", r#"
            fn foo() {
                 let _v = (env!("CARGO_BIN_FILE_BAR_RENAMED"),
                           env!("CARGO_BIN_FILE_BAR_RENAMED_bar"));
                 let _v = (env!("CARGO_BIN_FILE_BAR"),
                           env!("CARGO_BIN_FILE_BAR_bar"));
            }"#)
        .file(
            "build.rs",
            r#"fn main() {
               assert!(option_env!("CARGO_BIN_FILE_BAR_RENAMED").is_none());
               assert!(option_env!("CARGO_BIN_FILE_BAR_RENAMED_bar").is_none());
               println!("{}", std::env::var("CARGO_BIN_FILE_BAR_RENAMED").expect("CARGO_BIN_FILE_BAR_RENAMED"));
               println!("{}", std::env::var("CARGO_BIN_FILE_BAR_RENAMED_bar").expect("CARGO_BIN_FILE_BAR_RENAMED_bar"));
               
               assert!(option_env!("CARGO_BIN_FILE_BAR").is_none());
               assert!(option_env!("CARGO_BIN_FILE_BAR_bar").is_none());
               println!("{}", std::env::var("CARGO_BIN_FILE_BAR").expect("CARGO_BIN_FILE_BAR"));
               println!("{}", std::env::var("CARGO_BIN_FILE_BAR_bar").expect("CARGO_BIN_FILE_BAR_bar"));
           }"#,
        )
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/main.rs", "")
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar [..]
[COMPILING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();
}

#[cargo_test]
// TODO(ST): try to actually access the artifacts
#[ignore]
fn check_rust_libs_are_available_with_lib_true() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin", lib = true }
            "#,
        )
        .file(
            "src/lib.rs",
            r#"extern crate bar; static v: &str = env!("CARGO_BIN_FILE_BAR");"#,
        )
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .file("bar/src/main.rs", "")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[CHECKING] bar [..]
[CHECKING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();
}

#[cargo_test]
fn prevent_no_lib_warning_with_artifact_dependencies() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin" }
            "#,
        )
        .file("src/lib.rs", "")
        .file("bar/Cargo.toml", &basic_bin_manifest("bar"))
        .file("bar/src/main.rs", "fn main() {}")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr_contains("[COMPILING] bar v0.5.0 ([CWD]/bar)")
        .with_stderr_contains("[CHECKING] foo v0.0.0 ([CWD])")
        .with_stderr_contains("[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]")
        .run();
}

#[cargo_test]
fn show_no_lib_warning_with_artifact_dependencies_that_have_no_lib_but_lib_true() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin", lib = true }
            "#,
        )
        .file("src/lib.rs", "")
        .file("bar/Cargo.toml", &basic_bin_manifest("bar"))
        .file("bar/src/main.rs", "fn main() {}")
        .build();
    p.cargo("check -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr_contains("[WARNING] foo v0.0.0 ([CWD]) ignoring invalid dependency `bar` which is missing a lib target")
        .with_stderr_contains("[COMPILING] bar v0.5.0 ([CWD]/bar)")
        .with_stderr_contains("[CHECKING] foo [..]")
        .with_stderr_contains("[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]")
        .run();
}

#[cargo_test]
fn check_missing_crate_type_in_package_fails() {
    for crate_type in &["cdylib", "staticlib", "bin"] {
        let p = project()
            .file(
                "Cargo.toml",
                &format!(
                    r#"
                        [package]
                        name = "foo"
                        version = "0.0.0"
                        authors = []
                        
                        [dependencies]
                        bar = {{ path = "bar/", artifact = "{}" }}
                    "#,
                    crate_type
                ),
            )
            .file("src/lib.rs", "")
            .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1")) //no bin, just rlib
            .file("bar/src/lib.rs", "")
            .build();
        p.cargo("check -Z unstable-options -Z bindeps")
            .masquerade_as_nightly_cargo()
            .with_status(101)
            .with_stderr(
                "[ERROR] Dependency `bar` in crate `foo` requires a `[..]` artifact to be present.",
            )
            .run();
    }
}

#[cargo_test]
#[ignore]
fn env_vars_and_build_products_for_various_build_targets() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [build-dependencies]
                bar = { path = "bar/", artifact = ["cdylib", "staticlib"] }
                
                [dev-dependencies]
                bar = { path = "bar/", artifact = ["bin:a", "bin:b"] }
                
                [dependencies]
                bar = { path = "bar/", artifact = "bin", lib = true }
            "#,
        )
        .file("src/lib.rs", "")
        .file("build.rs", "fn main() {}")
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", r#"pub extern "C" fn bar_c() {}"#)
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar [..]
[COMPILING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();

    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.0"
                authors = []
                
                [dependencies]
                bar = { path = "bar/", artifact = ["bin", "cdylib", "staticlib"] }
            "#,
        )
        .file("src/lib.rs", "")
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("build -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar [..]
[COMPILING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();
}

#[cargo_test]
fn publish_artifact_dep() {
    registry::init();
    Package::new("bar", "1.0.0").publish();
    Package::new("baz", "1.0.0").publish();

    let p = project()
        .file(
            "Cargo.toml",
            r#"
            [package]
            name = "foo"
            version = "0.1.0"
            authors = []
            license = "MIT"
            description = "foo"
            documentation = "foo"
            homepage = "foo"
            repository = "foo"

            [dependencies]
            bar = { version = "1.0", artifact = "bin", lib = true }
            
            [build-dependencies]
            baz = { version = "1.0", artifact = ["bin:a", "cdylib", "staticlib"] }
            "#,
        )
        .file("src/lib.rs", "")
        .build();

    p.cargo("publish -Z unstable-options -Z bindeps --no-verify --token sekrit")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[UPDATING] [..]
[PACKAGING] foo v0.1.0 [..]
[UPLOADING] foo v0.1.0 [..]
",
        )
        .run();

    publish::validate_upload_with_contents(
        r#"
        {
          "authors": [],
          "badges": {},
          "categories": [],
          "deps": [{
              "default_features": true,
              "features": [],
              "kind": "normal",
              "name": "bar",
              "optional": false,
              "registry": "https://github.com/rust-lang/crates.io-index",
              "target": null,
              "version_req": "^1.0"
            },
            {
              "default_features": true,
              "features": [],
              "kind": "build",
              "name": "baz",
              "optional": false,
              "registry": "https://github.com/rust-lang/crates.io-index",
              "target": null,
              "version_req": "^1.0"
            }
          ],
          "description": "foo",
          "documentation": "foo",
          "features": {},
          "homepage": "foo",
          "keywords": [],
          "license": "MIT",
          "license_file": null,
          "links": null,
          "name": "foo",
          "readme": null,
          "readme_file": null,
          "repository": "foo",
          "vers": "0.1.0"
        }
        "#,
        "foo-0.1.0.crate",
        &["Cargo.toml", "Cargo.toml.orig", "src/lib.rs"],
        &[(
            "Cargo.toml",
            &format!(
                r#"{}
[package]
name = "foo"
version = "0.1.0"
authors = []
description = "foo"
homepage = "foo"
documentation = "foo"
license = "MIT"
repository = "foo"
[dependencies.bar]
version = "1.0"
artifact = ["bin"]
lib = true
[build-dependencies.baz]
version = "1.0"
artifact = ["bin:a", "cdylib", "staticlib"]"#,
                cargo::core::package::MANIFEST_PREAMBLE
            ),
        )],
    );
}

#[cargo_test]
fn doc_lib_true() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.1"
                authors = []

                [dependencies.bar]
                path = "bar"
                artifact = "bin"
                lib = true
            "#,
        )
        .file("src/lib.rs", "extern crate bar; pub fn foo() {}")
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "pub fn bar() {}")
        .file("bar/src/main.rs", "fn main() {}")
        .build();

    p.cargo("doc -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar v0.0.1 ([CWD]/bar)
[DOCUMENTING] bar v0.0.1 ([CWD]/bar)
[DOCUMENTING] foo v0.0.1 ([CWD])
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(p.root().join("target/doc/bar/index.html").is_file());

    // Verify that it emits rmeta for the bin and lib dependency.
    assert_eq!(p.glob("target/debug/artifact/*.rlib").count(), 0);
    assert_eq!(p.glob("target/debug/deps/libbar-*.rmeta").count(), 2);

    p.cargo("doc")
        .env("CARGO_LOG", "cargo::ops::cargo_rustc::fingerprint")
        .with_stdout("")
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(p.root().join("target/doc/bar/index.html").is_file());
}

#[cargo_test]
fn rustdoc_works_on_libs_with_artifacts_and_lib_false() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.0.1"
                authors = []

                [dependencies.bar]
                path = "bar"
                artifact = ["bin", "staticlib", "cdylib"]
            "#,
        )
        .file(
            "src/lib.rs",
            r#"
            pub fn foo() {
                env!("CARGO_BIN_DIR_BAR");
                let _b = include_bytes!(env!("CARGO_BIN_FILE_BAR"));
                let _b = include_bytes!(env!("CARGO_CDYLIB_FILE_BAR"));
                let _b = include_bytes!(env!("CARGO_CDYLIB_FILE_BAR_bar"));
                let _b = include_bytes!(env!("CARGO_STATICLIB_FILE_BAR"));
                let _b = include_bytes!(env!("CARGO_STATICLIB_FILE_BAR_bar"));
            }"#,
        )
        .file(
            "bar/Cargo.toml",
            r#"
                [package]
                name = "bar"
                version = "0.5.0"
                authors = []
                
                [lib]
                crate-type = ["staticlib", "cdylib"]
            "#,
        )
        .file("bar/src/lib.rs", "pub fn bar() {}")
        .file("bar/src/main.rs", "fn main() {}")
        .build();

    p.cargo("doc -Z unstable-options -Z bindeps")
        .masquerade_as_nightly_cargo()
        .with_stderr(
            "\
[COMPILING] bar v0.5.0 ([CWD]/bar)
[DOCUMENTING] foo v0.0.1 ([CWD])
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(
        !p.root().join("target/doc/bar/index.html").is_file(),
        "bar is not a lib dependency and thus remains undocumented"
    );
}

fn assert_artifact_executable_output(
    p: &Project,
    target_name: &str,
    dep_name: &str,
    bin_name: &str,
) {
    #[cfg(any(not(windows), target_env = "gnu"))]
    {
        assert_eq!(
            p.glob(format!(
                "target/{}/artifact/{}-*/bin/{}-*.d",
                target_name, dep_name, bin_name
            ))
            .count(),
            1,
            "artifacts are placed into their own output directory to not possibly clash"
        );
    }
    #[cfg(all(windows, not(target_env = "gnu")))]
    {
        assert_eq!(
            p.glob(format!(
                "target/{}/artifact/{}-*/bin/{}{}",
                target_name,
                dep_name,
                bin_name,
                std::env::consts::EXE_SUFFIX
            ))
            .count(),
            1,
            "artifacts are placed into their own output directory to not possibly clash"
        );
    }
}

fn build_script_output_string(p: &Project, package_name: &str) -> String {
    let paths = p
        .glob(format!("target/debug/build/{}-*/output", package_name))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(paths.len(), 1);
    std::fs::read_to_string(&paths[0]).unwrap()
}
