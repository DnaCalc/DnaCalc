//! Compare independent workbook opens without equating their deliberately unique identities.
//! All value, cache, refusal, history and certificate fields remain in the comparison.

use calc_engine::contract::{CalcPassId, ComputationOrigin};
use calc_host::{Calculation, ProjectedValue};

fn pass(pass: &mut CalcPassId) {
    pass.command.domain_incarnation = 0;
}

fn calculation(calculation: &Calculation) -> String {
    let stamp = calculation.stamp().cloned().map(|mut stamp| {
        let workbook = stamp.target.workbook;
        assert_eq!(
            stamp.certification.command.domain_incarnation,
            workbook.document
        );
        assert_eq!(
            stamp.certification.pass.command,
            stamp.certification.command
        );
        if let ComputationOrigin::Evaluated { root } = &mut stamp.computed_by {
            assert_eq!(root.workbook, workbook);
            assert_eq!(root.pass.command.domain_incarnation, workbook.document);
            root.workbook.document = 0;
            pass(&mut root.pass);
        }
        stamp.target.workbook.document = 0;
        stamp.certification.command.domain_incarnation = 0;
        pass(&mut stamp.certification.pass);
        for read in &mut stamp.reads.dependencies {
            if let Some(version) = &mut read.versions.value {
                pass(version);
            }
        }
        stamp
    });
    format!(
        "r{:?} t{:?} mode{:?} target{:?} external{} circular{} stamp{:?}",
        calculation.revision(),
        calculation.tick(),
        calculation.mode(),
        calculation.target(),
        calculation.reads_external_links(),
        calculation.held_by_circular_rule(),
        stamp
    )
}

pub fn comparable(value: &ProjectedValue) -> String {
    match value {
        ProjectedValue::Calculated {
            value,
            calculation: provenance,
        } => format!("Calculated {value:?} {}", calculation(provenance)),
        ProjectedValue::NotModeled {
            refusal,
            calculation: provenance,
        } => format!("NotModeled {refusal:?} {}", calculation(provenance)),
        ProjectedValue::Stale(stale) => format!(
            "Stale {:?} {:?} {:?} {:?}",
            stale.last_value(),
            stale.last_calculation().map(calculation),
            stale.file_cache(),
            stale.cause()
        ),
        other => format!("{other:?}"),
    }
}
