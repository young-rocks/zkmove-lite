use crate::frame::{ExitStatus, Frame, Locals};
use crate::stack::{CallStack, EvalStack};
use crate::value::Value;
use bellman::pairing::Engine;
use bellman::{ConstraintSystem, SynthesisError};
use error::{RuntimeError, StatusCode, VmResult};
use logger::prelude::*;
use move_vm_runtime::loader::Function;
use movelang::argument::{convert_from, ScriptArguments};
use movelang::loader::MoveLoader;
use movelang::value::MoveValueType;
use std::convert::TryInto;
use std::sync::Arc;
use crate::circuit::InstructionsChip;
use halo2::{
    arithmetic::FieldExt,
    circuit::Layouter,
};
use crate::instructions::Instructions;

pub struct Interpreter<F: FieldExt> {
    pub stack: EvalStack<F>,
    pub frames: CallStack<F>,
    pub step: u64,
}

impl<F: FieldExt> Interpreter<F>
{
    pub fn new() -> Self {
        Self {
            stack: EvalStack::new(),
            frames: CallStack::new(),
            step: 0,
        }
    }

    pub fn stack(&self) -> &EvalStack<F> {
        &self.stack
    }

    pub fn frames(&mut self) -> &mut CallStack<F> {
        &mut self.frames
    }

    pub fn current_frame(&mut self) -> Option<&mut Frame<F>> {
        self.frames.top()
    }

    fn process_arguments(
        &mut self,
        locals: &mut Locals<F>,
        args: Option<ScriptArguments>,
        arg_types: Vec<MoveValueType>,
        instructions_chip: &InstructionsChip<F>,
        mut layouter: impl Layouter<F>,
    ) -> VmResult<()>
    {
        let arg_type_pairs: Vec<_> = match args {
            Some(values) => values
                .as_inner()
                .iter()
                .map(|v| Some(v.clone()))
                .zip(arg_types)
                .collect(),
            None => std::iter::repeat(None).zip(arg_types).collect(),
        };

        for (i, (arg, ty)) in arg_type_pairs.into_iter().enumerate() {
            let val = match arg {
                Some(a) => {
                    let value: F = convert_from(a)?;
                    Some(value)
                }
                None => None,
            };
            let cell = instructions_chip.load_private(layouter.namespace(|| format!("load argument #{}", i)), val)
            .map_err(|e| {
                debug!("Process arguments error: {:?}", e);
                RuntimeError::new(StatusCode::SynthesisError)
            })?;

            locals.store(i, Value::new_variable(cell.value, cell.cell, ty)?)?;
        }

        Ok(())
    }

    fn make_frame(&mut self, func: Arc<Function>) -> VmResult<Frame<F>> {
        let mut locals = Locals::new(func.local_count());
        let arg_count = func.arg_count();
        for i in 0..arg_count {
            locals.store(arg_count - i - 1, self.stack.pop()?)?;
        }
        Ok(Frame::new(func, locals))
    }

    pub fn run_script(
        &mut self,
        instructions_chip: &InstructionsChip<F>,
        mut layouter: impl Layouter<F>,
        entry: Arc<Function>,
        args: Option<ScriptArguments>,
        arg_types: Vec<MoveValueType>,
        loader: &MoveLoader,
    ) -> VmResult<()>
    {
        let mut locals = Locals::new(entry.local_count());
        // cs.enforce(
        //     || "constraint",
        //     |zero| zero + CS::one(),
        //     |zero| zero + CS::one(),
        //     |zero| zero + CS::one(),
        // );

        self.process_arguments(&mut locals, args, arg_types, instructions_chip, layouter.namespace(|| format!("process arguments in step#{}", self.step)))?;

        let mut frame = Frame::new(entry, locals);
        frame.print_frame();
        loop {
            let status = frame.execute(instructions_chip, layouter.namespace(|| format!("into frame in step#{}", self.step)), self)?;
            match status {
                ExitStatus::Return => {
                    if let Some(caller_frame) = self.frames.pop() {
                        frame = caller_frame;
                        frame.add_pc();
                    } else {
                        return Ok(());
                    }
                }
                ExitStatus::Call(index) => {
                    let func = loader.function_from_handle(frame.func(), index);
                    debug!("Call into function: {:?}", func.name());
                    let callee_frame = self.make_frame(func)?;
                    callee_frame.print_frame();
                    self.frames.push(frame)?;
                    frame = callee_frame;
                }
            }
        }
    }

    // pub fn binary_op<CS, Fn>(&mut self, cs: &mut CS, op: Fn) -> VmResult<()>
    // where
    //     CS: ConstraintSystem<F>,
    //     Fn: FnOnce(&mut CS, Value<F>, Value<F>) -> VmResult<Value<F>>,
    // {
    //     let right = self.stack.pop()?;
    //     let left = self.stack.pop()?;
    //
    //     let result = op(cs, left, right)?;
    //     self.stack.push(result)
    // }
    //
    // pub fn unary_op<CS, Fn>(&mut self, cs: &mut CS, op: Fn) -> VmResult<()>
    // where
    //     CS: ConstraintSystem<F>,
    //     Fn: FnOnce(&mut CS, Value<F>) -> VmResult<Value<F>>,
    // {
    //     let operand = self.stack.pop()?;
    //
    //     let result = op(cs, operand)?;
    //     self.stack.push(result)
    // }
}

impl<F: FieldExt> Default for Interpreter<F> {
    fn default() -> Self {
        Self::new()
    }
}
