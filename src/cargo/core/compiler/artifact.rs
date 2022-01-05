use crate::core::compiler::unit_graph::UnitDep;
use crate::core::compiler::{Context, CrateType, FileFlavor, Metadata, Unit};
use crate::core::TargetKind;
use crate::CargoResult;
use cargo_util::ProcessBuilder;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Adjust `cmd` to contain artifact environment variables and return all set key/value pairs for later use.
pub fn set_env(
    cx: &Context<'_, '_>,
    dependencies: &[UnitDep],
    cmd: &mut ProcessBuilder,
) -> CargoResult<Option<HashMap<Metadata, HashSet<(String, PathBuf)>>>> {
    let mut ret = HashMap::new();
    for unit_dep in dependencies.iter().filter(|d| d.unit.artifact.is_true()) {
        let mut set = HashSet::new();
        for artifact_path in cx
            .outputs(&unit_dep.unit)?
            .iter()
            .filter_map(|f| (f.flavor == FileFlavor::Normal).then(|| &f.path))
        {
            let artifact_type_upper = unit_artifact_type_name_upper(&unit_dep.unit);
            let dep_name = unit_dep.dep_name.unwrap_or(unit_dep.unit.pkg.name());
            let dep_name_upper = dep_name.to_uppercase().replace("-", "_");

            let var = format!("CARGO_{}_DIR_{}", artifact_type_upper, dep_name_upper);
            let path = artifact_path.parent().expect("parent dir for artifacts");
            cmd.env(&var, path);
            set.insert((var, path.to_owned()));

            let var = format!(
                "CARGO_{}_FILE_{}_{}",
                artifact_type_upper,
                dep_name_upper,
                unit_dep.unit.target.name()
            );
            cmd.env(&var, artifact_path);
            set.insert((var, artifact_path.to_owned()));

            if unit_dep.unit.target.name() == dep_name.as_str() {
                let var = format!("CARGO_{}_FILE_{}", artifact_type_upper, dep_name_upper,);
                cmd.env(&var, artifact_path);
                set.insert((var, artifact_path.to_owned()));
            }
        }
        if !set.is_empty() {
            ret.insert(cx.files().metadata(&unit_dep.unit), set);
        }
    }
    Ok((!ret.is_empty()).then(|| ret))
}

fn unit_artifact_type_name_upper(unit: &Unit) -> &'static str {
    match unit.target.kind() {
        TargetKind::Lib(kinds) => match kinds.as_slice() {
            &[CrateType::Cdylib] => "CDYLIB",
            &[CrateType::Staticlib] => "STATICLIB",
            invalid => unreachable!("BUG: artifacts cannot be of type {:?}", invalid),
        },
        TargetKind::Bin => "BIN",
        invalid => unreachable!("BUG: artifacts cannot be of type {:?}", invalid),
    }
}
