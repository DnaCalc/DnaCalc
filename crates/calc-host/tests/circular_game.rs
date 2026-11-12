//! The generated Circular Reference workbook, played through the same host session as the app.
#![allow(non_snake_case)]
use calc_host::value::{CalcValue, CellKey, ExcelText, WorksheetErrorCode};
use calc_host::{
    HostOptions, HostSession, MemorySource, OpenPolicy, PackageRead, ProjectedValue,
    ProjectionRequest, SkinIntent, project,
};
use std::sync::Arc;

fn text(value: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(value))
}

// Independent position model: no spreadsheet formula is interpreted here.
struct Game {
    body: Vec<i32>,
    tick: i32,
    food: i32,
    eaten: i32,
    outcome: &'static str,
}
impl Game {
    fn new() -> Self {
        Self {
            body: vec![88, 87, 86],
            tick: 0,
            food: 89,
            eaten: 0,
            outcome: "Playing",
        }
    }
    fn food(&self) -> i32 {
        self.food
    }
    fn step(&mut self, direction: char) {
        let (dx, dy) = match direction {
            'L' => (-1, 0),
            'R' => (1, 0),
            'U' => (0, -1),
            'D' => (0, 1),
            _ => panic!("bad direction"),
        };
        let head = self.body[0];
        let col = (head - 1) % 16 + 1;
        let row = (head - 1) / 16 + 1;
        let next = head + dx + 16 * dy;
        let grow = next == self.food();
        self.tick += 1;
        self.outcome = if !(1..=16).contains(&(col + dx)) || !(1..=12).contains(&(row + dy)) {
            "GAME OVER: #REF!"
        } else if next == 75 {
            "GAME OVER: #DIV/0!"
        } else if self.body[..self.body.len() - usize::from(!grow)].contains(&next) {
            "GAME OVER: Circular reference"
        } else {
            "Playing"
        };
        if self.outcome == "Playing" {
            self.body.insert(0, next);
            if grow {
                self.eaten += 1;
                self.food = match self.eaten {
                    1 => 90,
                    2 => 91,
                    _ => {
                        let candidate = (self.tick * 37 + 37) % 192 + 1;
                        if candidate == 75 { 76 } else { candidate }
                    }
                };
            } else {
                self.body.pop();
            }
        }
    }
    fn score(&self) -> f64 {
        let mut score = 0.0;
        if self.eaten > 0 {
            score += 0.1;
        }
        if self.eaten > 1 {
            score += 0.2;
        }
        if self.eaten > 2 {
            score += std::f64::consts::PI;
        }
        score + f64::from((self.eaten - 3).max(0))
    }
    fn board(&self, pos: i32) -> CalcValue {
        if self.body.contains(&pos) {
            text("o")
        } else if pos == 75 {
            CalcValue::error(WorksheetErrorCode::Div0)
        } else if pos == self.food() {
            CalcValue::number(match self.eaten {
                0 => 0.1,
                1 => 0.2,
                2 => std::f64::consts::PI,
                _ => 1.0,
            })
        } else {
            text("")
        }
    }
}
fn enter(book: &mut HostSession, row: u32, col: u32, value: &str) {
    let at = CellKey::new(book.host().sheets()[0].id(), row, col).unwrap();
    let receipt = book.dispatch(SkinIntent::EnterCell {
        at,
        text: value.into(),
    });
    assert!(receipt.accepted());
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    for event in book.take_calculation_events() {
        if let calc_host::CalcEvent::Completed(completion) = event {
            assert_eq!(completion.coverage.refused, 0, "{completion:?}");
            println!("GAME-COVERAGE {:?}", completion.coverage);
        }
    }
}
fn assert_board(book: &HostSession, game: &Game, game_no: usize, step: usize) {
    let sheet = book.host().sheets()[0].id();
    let mut cells = vec![
        CellKey::new(sheet, 4, 2).unwrap(),
        CellKey::new(sheet, 5, 2).unwrap(),
        CellKey::new(sheet, 6, 2).unwrap(),
        CellKey::new(sheet, 5, 4).unwrap(),
        CellKey::new(sheet, 6, 6).unwrap(),
    ];
    for row in 7..=18 {
        for col in 2..=17 {
            cells.push(CellKey::new(sheet, row, col).unwrap());
        }
    }
    let page = project(book.host(), &ProjectionRequest { cells }).unwrap();
    let mut terms = Vec::new();
    if game.eaten > 0 {
        terms.push("0.1");
    }
    if game.eaten > 1 {
        terms.push("0.2");
    }
    if game.eaten > 2 {
        terms.push("PI()");
    }
    for _ in 3..game.eaten {
        terms.push("1");
    }
    let score = game.score();
    let numerator = (score.fract() * 72057594037927936.0) as u64;
    let exact = format!(
        "{} + {}{:08}/72057594037927936",
        score.trunc() as u64,
        numerator / 100000000,
        numerator % 100000000
    );
    let expected = [
        text(game.outcome),
        CalcValue::number(score),
        CalcValue::number(game.body.len() as f64),
        text(&format!("=SUM({})", terms.join(","))),
        text(&exact),
    ];
    for (idx, cell) in page.cells().iter().enumerate() {
        let wanted = if idx < 5 {
            expected[idx].clone()
        } else {
            game.board((idx - 5) as i32 + 1)
        };
        match cell.value() {
            ProjectedValue::Calculated { value, .. } => assert_eq!(
                *value,
                wanted,
                "game {game_no} step {step} cell {:?}",
                cell.at()
            ),
            other => panic!(
                "game {game_no} step {step} cell {:?} not calculated: {other:?}",
                cell.at()
            ),
        }
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_y2o__circular_reference_game_script() {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(
        include_bytes!("fixtures/circular-reference.xlsx").to_vec(),
    ));
    let mut book = HostSession::open(
        source,
        &OpenPolicy::default(),
        HostOptions {
            locale: calc_func::locale::FormatProfile::for_culture("en-US"),
            ..HostOptions::default()
        },
    )
    .unwrap();
    assert!(book.host().recalc_context().iteration);
    assert_eq!(book.host().recalc_context().max_iterations, 1);
    let mut total = 0;
    for (game_no, moves) in include_str!("fixtures/circular-reference-moves.txt")
        .lines()
        .enumerate()
    {
        if game_no > 0 {
            enter(&mut book, 2, 2, "");
            enter(&mut book, 3, 2, "1");
            enter(&mut book, 3, 2, "0");
        }
        let mut game = Game::new();
        for (step, direction) in moves.chars().enumerate() {
            game.step(direction);
            enter(&mut book, 2, 2, &direction.to_string());
            assert_board(&book, &game, game_no + 1, step + 1);
            let board = (1..=192)
                .map(|pos| {
                    if game.body.contains(&pos) {
                        "o".to_owned()
                    } else if pos == 75 {
                        "e2007".to_owned()
                    } else if pos == game.food() {
                        let n: f64 = match game.eaten {
                            0 => 0.1,
                            1 => 0.2,
                            2 => std::f64::consts::PI,
                            _ => 1.0,
                        };
                        format!("n{:016x}", n.to_bits())
                    } else {
                        ".".to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            let n = (game.score().fract() * 72057594037927936.0) as u64;
            let exact = format!(
                "{} + {}{:08}/72057594037927936",
                game.score().trunc() as u64,
                n / 100000000,
                n % 100000000
            );
            println!(
                "GAME|{}|{}|{}|{:016x}|{}|{}|{}",
                game_no + 1,
                step + 1,
                game.outcome,
                game.score().to_bits(),
                game.body.len(),
                exact,
                board
            );
            total += 1;
        }
        assert_eq!(
            game.outcome,
            [
                "GAME OVER: #DIV/0!",
                "GAME OVER: Circular reference",
                "GAME OVER: #REF!"
            ][game_no]
        );
    }
    assert!(total >= 40);
}
