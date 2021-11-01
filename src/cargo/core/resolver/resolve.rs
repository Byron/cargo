use super::encode::Metadata;
use crate::core::dependency::DepKind;
use crate::core::{Dependency, PackageId, PackageIdSpec, Summary, Target};
use crate::util::errors::CargoResult;
use crate::util::interning::InternedString;
use crate::util::Graph;
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Represents a fully-resolved package dependency graph. Each node in the graph
/// is a package and edges represent dependencies between packages.
///
/// Each instance of `Resolve` also understands the full set of features used
/// for each package.
pub struct Resolve {
    /// A graph, whose vertices are packages and edges are dependency specifications
    /// from `Cargo.toml`. We need a `HashSet` here because the same package
    /// might be present in both `[dependencies]` and `[build-dependencies]`.
    graph: Graph<PackageId, HashSet<Dependency>>,
    /// Replacements from the `[replace]` table.
    replacements: HashMap<PackageId, PackageId>,
    /// Inverted version of `replacements`.
    reverse_replacements: HashMap<PackageId, PackageId>,
    /// Features enabled for a given package.
    features: HashMap<PackageId, Vec<InternedString>>,
    /// Checksum for each package. A SHA256 hash of the `.crate` file used to
    /// validate the correct crate file is used. This is `None` for sources
    /// that do not use `.crate` files, like path or git dependencies.
    checksums: HashMap<PackageId, Option<String>>,
    /// "Unknown" metadata. This is a collection of extra, unrecognized data
    /// found in the `[metadata]` section of `Cargo.lock`, preserved for
    /// forwards compatibility.
    metadata: Metadata,
    /// `[patch]` entries that did not match anything, preserved in
    /// `Cargo.lock` as the `[[patch.unused]]` table array. Tracking unused
    /// patches helps prevent Cargo from being forced to re-update the
    /// registry every time it runs, and keeps the resolve in a locked state
    /// so it doesn't re-resolve the unused entries.
    unused_patches: Vec<PackageId>,
    /// A map from packages to a set of their public dependencies
    public_dependencies: HashMap<PackageId, HashSet<PackageId>>,
    /// Version of the `Cargo.lock` format, see
    /// `cargo::core::resolver::encode` for more.
    version: ResolveVersion,
    summaries: HashMap<PackageId, Summary>,
}

/// A version to indicate how a `Cargo.lock` should be serialized. Currently
/// V2 is the default when creating a new lockfile. If a V1 lockfile already
/// exists, it will stay as V1.
///
/// It's theorized that we can add more here over time to track larger changes
/// to the `Cargo.lock` format, but we've yet to see how that strategy pans out.
#[derive(PartialEq, Eq, Clone, Copy, Debug, PartialOrd, Ord)]
pub enum ResolveVersion {
    /// Historical baseline for when this abstraction was added.
    V1,
    /// A more compact format, more amenable to avoiding source-control merge
    /// conflicts. The `dependencies` arrays are compressed and checksums are
    /// listed inline. Introduced in 2019 in version 1.38. New lockfiles use
    /// V2 by default starting in 1.41.
    V2,
    /// A format that explicitly lists a `version` at the top of the file as
    /// well as changing how git dependencies are encoded. Dependencies with
    /// `branch = "master"` are no longer encoded the same way as those without
    /// branch specifiers.
    V3,
}

impl Resolve {
    pub fn new(
        graph: Graph<PackageId, HashSet<Dependency>>,
        replacements: HashMap<PackageId, PackageId>,
        features: HashMap<PackageId, Vec<InternedString>>,
        checksums: HashMap<PackageId, Option<String>>,
        metadata: Metadata,
        unused_patches: Vec<PackageId>,
        version: ResolveVersion,
        summaries: HashMap<PackageId, Summary>,
    ) -> Resolve {
        let reverse_replacements = replacements.iter().map(|(&p, &r)| (r, p)).collect();
        let public_dependencies = graph
            .iter()
            .map(|p| {
                let public_deps = graph
                    .edges(p)
                    .filter(|(_, deps)| {
                        deps.iter()
                            .any(|d| d.kind() == DepKind::Normal && d.is_public())
                    })
                    .map(|(dep_package, _)| *dep_package)
                    .collect::<HashSet<PackageId>>();

                (*p, public_deps)
            })
            .collect();

        Resolve {
            graph,
            replacements,
            features,
            checksums,
            metadata,
            unused_patches,
            reverse_replacements,
            public_dependencies,
            version,
            summaries,
        }
    }

    /// Resolves one of the paths from the given dependent package up to
    /// the root.
    pub fn path_to_top<'a>(
        &'a self,
        pkg: &'a PackageId,
    ) -> Vec<(&'a PackageId, Option<&'a HashSet<Dependency>>)> {
        self.graph.path_to_top(pkg)
    }

    pub fn register_used_patches(&mut self, patches: &[Summary]) {
        for summary in patches {
            if !self.graph.contains(&summary.package_id()) {
                self.unused_patches.push(summary.package_id())
            };
        }
    }

    pub fn merge_from(&mut self, previous: &Resolve) -> CargoResult<()> {
        // Given a previous instance of resolve, it should be forbidden to ever
        // have a checksums which *differ*. If the same package ID has differing
        // checksums, then something has gone wrong such as:
        //
        // * Something got seriously corrupted
        // * A "mirror" isn't actually a mirror as some changes were made
        // * A replacement source wasn't actually a replacement, some changes
        //   were made
        //
        // In all of these cases, we want to report an error to indicate that
        // something is awry. Normal execution (esp just using crates.io) should
        // never run into this.
        for (id, cksum) in previous.checksums.iter() {
            if let Some(mine) = self.checksums.get(id) {
                if mine == cksum {
                    continue;
                }

                // If the previous checksum wasn't calculated, the current
                // checksum is `Some`. This may indicate that a source was
                // erroneously replaced or was replaced with something that
                // desires stronger checksum guarantees than can be afforded
                // elsewhere.
                if cksum.is_none() {
                    anyhow::bail!(
                        "\
checksum for `{}` was not previously calculated, but a checksum could now \
be calculated

this could be indicative of a few possible situations:

    * the source `{}` did not previously support checksums,
      but was replaced with one that does
    * newer Cargo implementations know how to checksum this source, but this
      older implementation does not
    * the lock file is corrupt
",
                        id,
                        id.source_id()
                    )

                // If our checksum hasn't been calculated, then it could mean
                // that future Cargo figured out how to checksum something or
                // more realistically we were overridden with a source that does
                // not have checksums.
                } else if mine.is_none() {
                    anyhow::bail!(
                        "\
checksum for `{}` could not be calculated, but a checksum is listed in \
the existing lock file

this could be indicative of a few possible situations:

    * the source `{}` supports checksums,
      but was replaced with one that doesn't
    * the lock file is corrupt

unable to verify that `{0}` is the same as when the lockfile was generated
",
                        id,
                        id.source_id()
                    )

                // If the checksums aren't equal, and neither is None, then they
                // must both be Some, in which case the checksum now differs.
                // That's quite bad!
                } else {
                    anyhow::bail!(
                        "\
checksum for `{}` changed between lock files

this could be indicative of a few possible errors:

    * the lock file is corrupt
    * a replacement source in use (e.g., a mirror) returned a different checksum
    * the source itself may be corrupt in one way or another

unable to verify that `{0}` is the same as when the lockfile was generated
",
                        id
                    );
                }
            }
        }

        // Be sure to just copy over any unknown metadata.
        self.metadata = previous.metadata.clone();

        // Preserve the lockfile encoding where possible to avoid lockfile churn
        self.version = previous.version;

        Ok(())
    }

    pub fn contains<Q: ?Sized>(&self, k: &Q) -> bool
    where
        PackageId: Borrow<Q>,
        Q: Ord + Eq,
    {
        self.graph.contains(k)
    }

    pub fn sort(&self) -> Vec<PackageId> {
        self.graph.sort()
    }

    pub fn iter(&self) -> impl Iterator<Item = PackageId> + '_ {
        self.graph.iter().cloned()
    }

    pub fn deps(&self, pkg: PackageId) -> impl Iterator<Item = (PackageId, &HashSet<Dependency>)> {
        self.deps_not_replaced(pkg)
            .map(move |(id, deps)| (self.replacement(id).unwrap_or(id), deps))
    }

    pub fn deps_not_replaced(
        &self,
        pkg: PackageId,
    ) -> impl Iterator<Item = (PackageId, &HashSet<Dependency>)> {
        self.graph.edges(&pkg).map(|(id, deps)| (*id, deps))
    }

    pub fn replacement(&self, pkg: PackageId) -> Option<PackageId> {
        self.replacements.get(&pkg).cloned()
    }

    pub fn replacements(&self) -> &HashMap<PackageId, PackageId> {
        &self.replacements
    }

    pub fn features(&self, pkg: PackageId) -> &[InternedString] {
        self.features.get(&pkg).map(|v| &**v).unwrap_or(&[])
    }

    /// This is only here for legacy support, it will be removed when
    /// switching to the new feature resolver.
    pub fn features_clone(&self) -> HashMap<PackageId, Vec<InternedString>> {
        self.features.clone()
    }

    pub fn is_public_dep(&self, pkg: PackageId, dep: PackageId) -> bool {
        self.public_dependencies
            .get(&pkg)
            .map(|public_deps| public_deps.contains(&dep))
            .unwrap_or_else(|| panic!("Unknown dependency {:?} for package {:?}", dep, pkg))
    }

    pub fn query(&self, spec: &str) -> CargoResult<PackageId> {
        PackageIdSpec::query_str(spec, self.iter())
    }

    pub fn specs_to_ids(&self, specs: &[PackageIdSpec]) -> CargoResult<Vec<PackageId>> {
        specs.iter().map(|s| s.query(self.iter())).collect()
    }

    pub fn unused_patches(&self) -> &[PackageId] {
        &self.unused_patches
    }

    pub fn checksums(&self) -> &HashMap<PackageId, Option<String>> {
        &self.checksums
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn extern_crate_name_and_dep_name(
        &self,
        from: PackageId,
        to: PackageId,
        to_target: &Target,
    ) -> CargoResult<(String, Option<InternedString>)> {
        let empty_set: HashSet<Dependency> = HashSet::new();
        let deps = if from == to {
            &empty_set
        } else {
            self.dependencies_listed(from, to)
        };

        let crate_name = to_target.crate_name();
        let mut names = deps.iter().map(|d| {
            d.explicit_name_in_toml()
                .map(|s| s.as_str().replace("-", "_"))
                .unwrap_or_else(|| crate_name.clone())
        });
        let name = names.next().unwrap_or_else(|| crate_name.clone());
        for n in names {
            anyhow::ensure!(
                n == name,
                "the crate `{}` depends on crate `{}` multiple times with different names",
                from,
                to,
            );
        }
        let dep_name = deps.iter().filter_map(|d| d.explicit_name_in_toml()).next();
        Ok((name, dep_name))
    }

    fn dependencies_listed(&self, from: PackageId, to: PackageId) -> &HashSet<Dependency> {
        // We've got a dependency on `from` to `to`, but this dependency edge
        // may be affected by [replace]. If the `to` package is listed as the
        // target of a replacement (aka the key of a reverse replacement map)
        // then we try to find our dependency edge through that. If that fails
        // then we go down below assuming it's not replaced.
        //
        // Note that we don't treat `from` as if it's been replaced because
        // that's where the dependency originates from, and we only replace
        // targets of dependencies not the originator.
        if let Some(replace) = self.reverse_replacements.get(&to) {
            if let Some(deps) = self.graph.edge(&from, replace) {
                return deps;
            }
        }
        match self.graph.edge(&from, &to) {
            Some(ret) => ret,
            None => panic!("no Dependency listed for `{}` => `{}`", from, to),
        }
    }

    /// Returns the version of the encoding that's being used for this lock
    /// file.
    pub fn version(&self) -> ResolveVersion {
        self.version
    }

    pub fn set_version(&mut self, version: ResolveVersion) {
        self.version = version;
    }

    pub fn summary(&self, pkg_id: PackageId) -> &Summary {
        &self.summaries[&pkg_id]
    }
}

impl PartialEq for Resolve {
    fn eq(&self, other: &Resolve) -> bool {
        macro_rules! compare {
            ($($fields:ident)* | $($ignored:ident)*) => {
                let Resolve { $($fields,)* $($ignored: _,)* } = self;
                $($fields == &other.$fields)&&*
            }
        }
        compare! {
            // fields to compare
            graph replacements reverse_replacements features
            checksums metadata unused_patches public_dependencies summaries
            |
            // fields to ignore
            version
        }
    }
}

impl fmt::Debug for Resolve {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(fmt, "graph: {:?}", self.graph)?;
        writeln!(fmt, "\nfeatures: {{")?;
        for (pkg, features) in &self.features {
            writeln!(fmt, "  {}: {:?}", pkg, features)?;
        }
        write!(fmt, "}}")
    }
}

impl Default for ResolveVersion {
    /// The default way to encode new or updated `Cargo.lock` files.
    ///
    /// It's important that if a new version of `ResolveVersion` is added that
    /// this is not updated until *at least* the support for the version is in
    /// the stable release of Rust.
    ///
    /// This resolve version will be used for all new lock files, for example
    /// those generated by `cargo update` (update everything) or building after
    /// a `cargo new` (where no lock file previously existed). This is also used
    /// for *updated* lock files such as when a dependency is added or when a
    /// version requirement changes. In this situation Cargo's updating the lock
    /// file anyway so it takes the opportunity to bump the lock file version
    /// forward.
    fn default() -> ResolveVersion {
        ResolveVersion::V3
    }
}
