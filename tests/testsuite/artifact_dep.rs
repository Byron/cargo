use cargo_test_support::registry::Package;
use cargo_test_support::{basic_manifest, project, publish, registry};

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
                bar = { path = "bar/", artifact = "bin", lib = true }
            "#,
        )
        .file("src/lib.rs", "")
        .file("bar/Cargo.toml", &basic_manifest("bar", "0.0.1"))
        .file("bar/src/lib.rs", "")
        .build();
    p.cargo("build")
        .with_stderr(
            "\
warning: 'artifact = [..]' ignored for dependency (bar) as -Z bindeps is not set.
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
    p.cargo("build")
        .with_stderr(
            "\
warning: 'artifact = [..]' ignored for dependency (bar) as -Z bindeps is not set.
[COMPILING] bar [..]
[COMPILING] foo [..]
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();
}

#[cargo_test]
fn build_makes_additional_envvars_available_in_rustc_invocations() {
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
[..] bar v0.0.1 ([CWD]/bar)
[..] bar v0.0.1 ([CWD]/bar)
[DOCUMENTING] foo v0.0.1 ([CWD])
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(p.root().join("target/doc/bar/index.html").is_file());

    // Verify that it only emits rmeta for the dependency.
    assert_eq!(p.glob("target/debug/**/*.rlib").count(), 0);
    assert_eq!(p.glob("target/debug/deps/libbar-*.rmeta").count(), 1);

    p.cargo("doc")
        .env("CARGO_LOG", "cargo::ops::cargo_rustc::fingerprint")
        .with_stdout("")
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(p.root().join("target/doc/bar/index.html").is_file());
}

#[cargo_test]
#[ignore] // TODO: assure this doesn't fail
fn no_doc_for_non_lib() {
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
[CHECKING] bar v0.0.1 ([CWD]/bar)
[DOCUMENTING] foo v0.0.1 ([CWD])
[FINISHED] dev [unoptimized + debuginfo] target(s) in [..]
",
        )
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(p.root().join("target/doc/bar/index.html").is_file());

    // Verify that it only emits rmeta for the dependency.
    assert_eq!(p.glob("target/debug/**/*.rlib").count(), 0);
    assert_eq!(p.glob("target/debug/deps/libbar-*.rmeta").count(), 1);

    p.cargo("doc")
        .env("CARGO_LOG", "cargo::ops::cargo_rustc::fingerprint")
        .with_stdout("")
        .run();

    assert!(p.root().join("target/doc").is_dir());
    assert!(p.root().join("target/doc/foo/index.html").is_file());
    assert!(p.root().join("target/doc/bar/index.html").is_file());
}
