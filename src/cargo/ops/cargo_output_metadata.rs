use crate::core::compiler::{CompileKind, RustcTargetData};
use crate::core::dependency::{ArtifactKind, ArtifactTarget, DepKind};
use crate::core::package::SerializedPackage;
use crate::core::resolver::{features::CliFeatures, HasDevUnits, Resolve};
use crate::core::{Dependency, Package, PackageId, Workspace};
use crate::ops::{self, Packages};
use crate::util::interning::InternedString;
use crate::util::CargoResult;
use cargo_platform::Platform;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use toml_edit::easy as toml;

const VERSION: u32 = 1;

pub struct OutputMetadataOptions {
    pub cli_features: CliFeatures,
    pub no_deps: bool,
    pub version: u32,
    pub filter_platforms: Vec<String>,
}

/// Loads the manifest, resolves the dependencies of the package to the concrete
/// used versions - considering overrides - and writes all dependencies in a JSON
/// format to stdout.
pub fn output_metadata(ws: &Workspace<'_>, opt: &OutputMetadataOptions) -> CargoResult<ExportInfo> {
    if opt.version != VERSION {
        anyhow::bail!(
            "metadata version {} not supported, only {} is currently supported",
            opt.version,
            VERSION
        );
    }
    let (packages, resolve) = if opt.no_deps {
        let packages = ws.members().map(|pkg| pkg.serialized()).collect();
        (packages, None)
    } else {
        let (packages, resolve) = build_resolve_graph(ws, opt)?;
        (packages, Some(resolve))
    };

    Ok(ExportInfo {
        packages,
        workspace_members: ws.members().map(|pkg| pkg.package_id()).collect(),
        resolve,
        target_directory: ws.target_dir().into_path_unlocked(),
        version: VERSION,
        workspace_root: ws.root().to_path_buf(),
        metadata: ws.custom_metadata().cloned(),
    })
}

/// This is the structure that is serialized and displayed to the user.
///
/// See cargo-metadata.adoc for detailed documentation of the format.
#[derive(Serialize)]
pub struct ExportInfo {
    packages: Vec<SerializedPackage>,
    workspace_members: Vec<PackageId>,
    resolve: Option<MetadataResolve>,
    target_directory: PathBuf,
    version: u32,
    workspace_root: PathBuf,
    metadata: Option<toml::Value>,
}

#[derive(Serialize)]
struct MetadataResolve {
    nodes: Vec<MetadataResolveNode>,
    root: Option<PackageId>,
}

#[derive(Serialize)]
struct MetadataResolveNode {
    id: PackageId,
    dependencies: Vec<PackageId>,
    deps: Vec<Dep>,
    features: Vec<InternedString>,
}

#[derive(Serialize)]
struct Dep {
    name: InternedString,
    pkg: PackageId,
    dep_kinds: Vec<DepKindInfo>,
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct DepKindInfo {
    kind: DepKind,
    target: Option<Platform>,
    extern_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bin_name: Option<InternedString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compile_target: Option<InternedString>,
}

impl From<&Dependency> for DepKindInfo {
    fn from(dep: &Dependency) -> DepKindInfo {
        DepKindInfo {
            kind: dep.kind(),
            target: dep.platform().cloned(),
            extern_name: dep.name_in_toml().replace('-', "_").into(),
            bin_name: None,
            artifact: None,
            compile_target: None,
        }
    }
}

/// Builds the resolve graph as it will be displayed to the user.
fn build_resolve_graph(
    ws: &Workspace<'_>,
    metadata_opts: &OutputMetadataOptions,
) -> CargoResult<(Vec<SerializedPackage>, MetadataResolve)> {
    // TODO: Without --filter-platform, features are being resolved for `host` only.
    // How should this work?
    let requested_kinds =
        CompileKind::from_requested_targets(ws.config(), &metadata_opts.filter_platforms)?;
    let target_data = RustcTargetData::new(ws, &requested_kinds)?;
    // Resolve entire workspace.
    let specs = Packages::All.to_package_id_specs(ws)?;
    let force_all = if metadata_opts.filter_platforms.is_empty() {
        crate::core::resolver::features::ForceAllTargets::Yes
    } else {
        crate::core::resolver::features::ForceAllTargets::No
    };

    // Note that even with --filter-platform we end up downloading host dependencies as well,
    // as that is the behavior of download_accessible.
    let ws_resolve = ops::resolve_ws_with_opts(
        ws,
        &target_data,
        &requested_kinds,
        &metadata_opts.cli_features,
        &specs,
        HasDevUnits::Yes,
        force_all,
    )?;

    let package_map: BTreeMap<PackageId, Package> = ws_resolve
        .pkg_set
        .packages()
        // This is a little lazy, but serde doesn't handle Rc fields very well.
        .map(|pkg| (pkg.package_id(), Package::clone(pkg)))
        .collect();

    // Start from the workspace roots, and recurse through filling out the
    // map, filtering targets as necessary.
    let mut node_map = BTreeMap::new();
    for member_pkg in ws.members() {
        build_resolve_graph_r(
            &mut node_map,
            member_pkg.package_id(),
            &ws_resolve.targeted_resolve,
            &package_map,
            &target_data,
            &requested_kinds,
        );
    }
    // Get a Vec of Packages.
    let actual_packages = package_map
        .into_iter()
        .filter_map(|(pkg_id, pkg)| node_map.get(&pkg_id).map(|_| pkg))
        .map(|pkg| pkg.serialized())
        .collect();

    let mr = MetadataResolve {
        nodes: node_map.into_iter().map(|(_pkg_id, node)| node).collect(),
        root: ws.current_opt().map(|pkg| pkg.package_id()),
    };
    Ok((actual_packages, mr))
}

fn build_resolve_graph_r(
    node_map: &mut BTreeMap<PackageId, MetadataResolveNode>,
    pkg_id: PackageId,
    resolve: &Resolve,
    package_map: &BTreeMap<PackageId, Package>,
    target_data: &RustcTargetData<'_>,
    requested_kinds: &[CompileKind],
) {
    if node_map.contains_key(&pkg_id) {
        return;
    }
    // This normalizes the IDs so that they are consistent between the
    // `packages` array and the `resolve` map. This is a bit of a hack to
    // compensate for the fact that
    // SourceKind::Git(GitReference::Branch("master")) is the same as
    // SourceKind::Git(GitReference::DefaultBranch). We want IDs in the JSON
    // to be opaque, and compare with basic string equality, so this will
    // always prefer the style of ID in the Package instead of the resolver.
    // Cargo generally only exposes PackageIds from the Package struct, and
    // AFAIK this is the only place where the resolver variant is exposed.
    //
    // This diverges because the SourceIds created for Packages are built
    // based on the Dependency declaration, but the SourceIds in the resolver
    // are deserialized from Cargo.lock. Cargo.lock may have been generated by
    // an older (or newer!) version of Cargo which uses a different style.
    let normalize_id = |id| -> PackageId { *package_map.get_key_value(&id).unwrap().0 };
    let features = resolve.features(pkg_id).to_vec();

    let deps: Vec<Dep> = resolve
        .deps(pkg_id)
        .filter(|(_dep_id, deps)| {
            if requested_kinds == [CompileKind::Host] {
                true
            } else {
                requested_kinds.iter().any(|kind| {
                    deps.iter()
                        .any(|dep| target_data.dep_platform_activated(dep, *kind))
                })
            }
        })
        .filter_map(|(dep_id, deps)| {
            let dep_pkg = package_map.get(&dep_id);
            dep_pkg
                .and_then(
                    |dep_pkg| match dep_pkg.targets().iter().find(|t| t.is_lib()) {
                        Some(lib_target) => resolve
                            .extern_crate_name_and_dep_name(pkg_id, dep_id, lib_target)
                            .map(|(ext_crate_name, _)| ext_crate_name)
                            .ok(),
                        None => {
                            // No traditional library is present which excludes bin-only artifacts.
                            // If one is present, we emulate the naming that would happen in `extern_crate_name_…()`.
                            deps.iter().find_map(|d| {
                                d.artifact()
                                    .map(|_| d.name_in_toml().replace('-', "_").into())
                            })
                        }
                    },
                )
                .map(|name| {
                    let mut dep_kinds: Vec<_> = deps
                        .iter()
                        .flat_map(|dep| single_dep_kind_or_spread_artifact_kinds(dep_pkg, dep))
                        .collect();
                    dep_kinds.sort();
                    Dep {
                        name,
                        pkg: normalize_id(dep_id),
                        dep_kinds,
                    }
                })
        })
        .collect();
    let dumb_deps: Vec<PackageId> = deps.iter().map(|dep| normalize_id(dep.pkg)).collect();
    let to_visit = dumb_deps.clone();
    let node = MetadataResolveNode {
        id: normalize_id(pkg_id),
        dependencies: dumb_deps,
        deps,
        features,
    };
    node_map.insert(pkg_id, node);
    for dep_id in to_visit {
        build_resolve_graph_r(
            node_map,
            dep_id,
            resolve,
            package_map,
            target_data,
            requested_kinds,
        );
    }
}

fn single_dep_kind_or_spread_artifact_kinds(
    dep_pkg: Option<&Package>,
    dep: &Dependency,
) -> Vec<DepKindInfo> {
    fn fix_extern_name(dki: &mut DepKindInfo, bin_name: &str) {
        dki.extern_name = bin_name.replace('-', "_").into();
    }
    dep.artifact()
        .map(|artifact| {
            let mut has_all_binaries = false;
            let compile_target = artifact.target().map(|target| match target {
                ArtifactTarget::BuildDependencyAssumeTarget => "target".into(),
                ArtifactTarget::Force(target) => target.rustc_target().into(),
            });
            let mut dep_kinds: Vec<_> = artifact
                .kinds()
                .iter()
                .filter_map(|kind| {
                    let mut dki = DepKindInfo::from(dep);
                    dki.artifact = Some(
                        match kind {
                            ArtifactKind::Staticlib => "staticlib",
                            ArtifactKind::Cdylib => "cdylib",
                            ArtifactKind::AllBinaries => {
                                // handled in second pass
                                has_all_binaries = true;
                                return None;
                            }
                            ArtifactKind::SelectedBinary(name) => {
                                dki.bin_name = Some(*name);
                                fix_extern_name(&mut dki, name);
                                "bin"
                            }
                        }
                        .into(),
                    );
                    dki.compile_target = compile_target;
                    Some(dki)
                })
                .collect();
            if let Some(dep_pkg) = has_all_binaries.then(|| dep_pkg).flatten() {
                // Note that we silently ignore the binaries missed here if dep_pkg is None, which apparently can happen.
                // If some warnings should one day be printed for less surprising behaviour, also consider adding a warning to the
                // ignored error further above (see `….ok()`).
                dep_kinds.extend(dep_pkg.targets().iter().filter(|t| t.is_bin()).map(
                    |bin_target| {
                        let mut dki = DepKindInfo::from(dep);
                        dki.artifact = "bin".into();
                        dki.bin_name = Some(bin_target.name().into());
                        fix_extern_name(&mut dki, bin_target.name());
                        dki.compile_target = compile_target;
                        dki
                    },
                ));
            };
            if artifact.is_lib() {
                dep_kinds.push(DepKindInfo::from(dep))
            }
            dep_kinds
        })
        .unwrap_or_else(|| vec![DepKindInfo::from(dep)])
}
