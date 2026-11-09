//! The grouping family fragment (dc-h9u): PERCENTOF, the share of a subset's sum in a whole's.
//!
//! Measured in Excel 16.0.20527 through COM on 2026-10-30: PERCENTOF(1,4) is 0.25,
//! PERCENTOF({1,2},{1,2,3,4}) 0.3, PERCENTOF("1",4) and PERCENTOF(TRUE,4) 0.25, PERCENTOF(,4) 0,
//! PERCENTOF(1,0) #DIV/0!, PERCENTOF(1,"a") #VALUE!, PERCENTOF(#N/A,4) #N/A; over a reference only
//! its numbers count, and a union is summed area by area; an error a reference or an array holds
//! is #NUM! (PERCENTOF(C1,4) over C1 = #N/A, PERCENTOF({1,#N/A},4)), a direct error the error.

use super::aggregate::{Held, direct_first, each_number};

use crate::call::{EvalServices, FunctionFault};
use crate::kernel::{Exit, Site};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};

use calc_catalogue::group_pivot::{CODE, ENTRIES as DECLARATIONS, PERCENTOF};

attach_family! {
    code: CODE; declarations: DECLARATIONS;
    PERCENTOF => Eager(percentof);
}

/// PERCENTOF (dc-h9u): the sum of data_subset divided by the sum of data_all, each summed as SUM
/// sums its one argument; #DIV/0! when the second sum is 0.
fn percentof(
    arguments: &[CalcValue],
    services: &mut EvalServices<'_>,
) -> Result<CalcValue, FunctionFault> {
    let site = Site::new(PERCENTOF, services);
    let mut sums = [0.0f64; 2];
    let mut read = Ok(());
    for (index, total) in sums.iter_mut().enumerate() {
        let one = std::slice::from_ref(&arguments[index]);
        let held = matches!(one[0].core_ref(), CoreRef::Reference(_) | CoreRef::Array(_));
        read = read
            .and_then(|()| direct_first(&site, one).map_err(|exit| reindex(exit, index)))
            .and_then(|()| {
                each_number(&site, one, services, Held::Numbers, &mut |n| {
                    *total = site.add(*total, n)?;
                    Ok(())
                })
                .map_err(|exit| match exit {
                    // An error a reference or an array holds is #NUM! (Excel 16.0.20527:
                    // PERCENTOF(C1,4) over C1 = #N/A, PERCENTOF({1,#N/A},4) are #NUM!).
                    Exit::Error(_) if held => Exit::Error(WorksheetErrorCode::Num),
                    other => reindex(other, index),
                })
            });
    }
    Site::finish(read.and_then(|()| site.number(site.divide(sums[0], sums[1])?)))
}

/// A refusal or fault naming argument 0 of a one-argument read, renamed to argument `index`.
fn reindex(exit: Exit, index: usize) -> Exit {
    match exit {
        Exit::Fault(FunctionFault::NotImplemented {
            id,
            argument: Some(0),
        }) => Exit::Fault(FunctionFault::NotImplemented {
            id,
            argument: Some(index),
        }),
        other => other,
    }
}
