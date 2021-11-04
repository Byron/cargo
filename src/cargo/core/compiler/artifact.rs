use crate::core::compiler::unit_graph::UnitDep;
use crate::core::compiler::{Context, CrateType, FileFlavor, Unit};
use crate::core::TargetKind;
use crate::CargoResult;
use cargo_util::ProcessBuilder;

pub fn set_env(
    cx: &Context<'_, '_>,
    dependencies: &[UnitDep],
    cmd: &mut ProcessBuilder,
) -> CargoResult<()> {
    for unit_dep in dependencies.iter().filter(|d| d.unit.artifact) {
        for artifact_path in cx
            .outputs(&unit_dep.unit)?
            .iter()
            .filter_map(|f| (f.flavor == FileFlavor::Normal).then(|| &f.path))
        {
            let artifact_type_upper = unit_artifact_type_name_upper(&unit_dep.unit);
            let dep_name = unit_dep.dep_name.unwrap_or(unit_dep.unit.pkg.name());
            let dep_name_upper = dep_name.to_uppercase().replace("-", "_");
            cmd.env(
                &format!("CARGO_{}_DIR_{}", artifact_type_upper, dep_name_upper),
                artifact_path.parent().expect("parent dir for artifacts"),
            );
            cmd.env(
                &format!(
                    "CARGO_{}_FILE_{}_{}",
                    artifact_type_upper,
                    dep_name_upper,
                    unit_dep.unit.target.name()
                ),
                artifact_path,
            );
            if unit_dep.unit.target.name() == dep_name.as_str() {
                cmd.env(
                    &format!("CARGO_{}_FILE_{}", artifact_type_upper, dep_name_upper,),
                    artifact_path,
                );
            }
        }
    }
    Ok(())
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
