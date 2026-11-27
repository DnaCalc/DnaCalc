//! Formula view state independent of the UI toolkit and the host carrier.

use calc_skin::Refusal;

#[derive(Clone, Debug, PartialEq)]
pub enum Live<T> {
    Idle,
    Loading,
    Ready(T),
    Outdated,
    Refused(Refusal),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RequestState<T> {
    latest: u64,
    live: Live<T>,
}

impl<T> Default for RequestState<T> {
    fn default() -> Self {
        Self {
            latest: 0,
            live: Live::Idle,
        }
    }
}

impl<T> RequestState<T> {
    pub fn live(&self) -> &Live<T> {
        &self.live
    }

    pub fn begin(&mut self) -> u64 {
        self.latest += 1;
        self.live = Live::Loading;
        self.latest
    }

    pub fn edited(&mut self) {
        self.latest += 1;
        if !matches!(self.live, Live::Idle) {
            self.live = Live::Outdated;
        }
    }

    pub fn finish(&mut self, request: u64, answer: Result<T, Refusal>) -> bool {
        if request == self.latest {
            self.live = match answer {
                Ok(answer) => Live::Ready(answer),
                Err(refusal) => Live::Refused(refusal),
            };
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_the_current_draft_can_report_a_current_result() {
        let mut state = RequestState::default();
        let first = state.begin();
        let second = state.begin();
        state.finish(first, Ok(4));
        assert_eq!(state.live(), &Live::Loading);
        state.edited();
        state.finish(second, Ok(5));
        assert_eq!(state.live(), &Live::Outdated);
        let third = state.begin();
        state.finish(third, Ok(6));
        assert_eq!(state.live(), &Live::Ready(6));
    }
}
