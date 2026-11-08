//! Native-only: needs the Windows broker running (`broker.py start`) and Excel under it.
//! Run with `cargo test -p touchstone-client --test live -- --ignored`.
use touchstone_client::{Answer, Arg, Client};

#[test]
#[ignore = "needs a running touchstone broker with Excel"]
fn the_edge_input_of_erf_precise_is_answered_by_excel() {
    let client = Client::connect()
        .expect("a running broker (TOUCHSTONE_ENDPOINT or the default state folder)");
    let reply = client
        .ask(
            "ERF.PRECISE",
            vec![vec![Arg::number_bits(0x3e47bc15e9263272)]],
        )
        .expect("answered");
    assert_eq!(reply.rows, [Answer::Number(0x3e4ac820de8e10b3)]);
    let env = reply
        .env
        .expect("the environment entry arrives with the answer");
    assert_eq!(env["engine"], "excel");
    // 10,000 calls in one request, all numbers
    let rows: Vec<Vec<Arg>> = (0..10000u64)
        .map(|i| vec![Arg::number_bits(0x3fb0000000000000 + i * 977)])
        .collect();
    let big = client.ask("ERF.PRECISE", rows).expect("answered");
    assert_eq!(big.rows.len(), 10000);
    assert!(big.rows.iter().all(|a| matches!(a, Answer::Number(_))));
}
