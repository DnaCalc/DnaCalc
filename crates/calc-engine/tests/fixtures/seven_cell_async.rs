//! The deterministic async provider shared by the seven-cell engine, host and worker tests.
use calc_fml::value::CalcValue;
use calc_fml::{
    UserFunction, UserFunctionContext, UserFunctionInvocation, UserFunctionStep,
    UserInvocationFrame,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Debug, Default)]
pub struct Counts {
    pub prefix: usize,
    pub suffix: usize,
    pub resumes: usize,
}
#[derive(Debug)]
pub struct Echo(pub Rc<RefCell<Counts>>);
#[derive(Debug)]
struct EchoCall {
    counts: Rc<RefCell<Counts>>,
    entered: bool,
    before_consumption: bool,
    returned: Option<CalcValue>,
}
impl UserFunction for Echo {
    fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
        panic!("the cooperative entry owns this call") // ubs:ignore -- cfg(test) fixture must fail if its owning activation is bypassed.
    }
    fn begin_call(
        &self,
        arguments: &[CalcValue],
        _: UserInvocationFrame,
        context: &mut dyn UserFunctionContext,
    ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
        for argument in arguments {
            if let Some(reference) = argument.as_reference() {
                context.argument_values_of(reference)?;
            }
        }
        Ok(Some(Box::new(EchoCall {
            counts: self.0.clone(),
            entered: false,
            before_consumption: false,
            returned: None,
        })))
    }
}
impl UserFunctionInvocation for EchoCall {
    fn resume(
        &mut self,
        context: &mut dyn UserFunctionContext,
        _: u64,
    ) -> Result<UserFunctionStep, String> {
        self.counts.borrow_mut().resumes += 1;
        if !self.entered {
            self.entered = true;
            self.counts.borrow_mut().prefix += 1;
            return Ok(UserFunctionStep::Awaiting { request: 7 });
        }
        if !self.before_consumption {
            self.before_consumption = true;
            return Ok(UserFunctionStep::Suspended);
        }
        if let Some(value) = self.returned.take() {
            return Ok(UserFunctionStep::Complete(value));
        }
        let value = context
            .take_async_result(7)
            .expect("only a delivered return resumes the suffix");
        assert!(context.take_async_result(7).is_none());
        self.counts.borrow_mut().suffix += 1;
        self.returned = Some(value);
        Ok(UserFunctionStep::Suspended)
    }
}
