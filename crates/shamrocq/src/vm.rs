use crate::arena::{Arena, ArenaError};
use crate::value::Value;

mod op {
    pub const IMM: u8 = 0x01;
    pub const TUPLE: u8 = 0x02;
    pub const LOAD: u8 = 0x03;
    pub const GLOBAL: u8 = 0x04;
    pub const CLOSURE: u8 = 0x05;
    pub const APPLY: u8 = 0x06;
    pub const TAIL_APPLY: u8 = 0x07;
    pub const RET: u8 = 0x08;
    pub const MATCH: u8 = 0x09;
    pub const JMP: u8 = 0x0A;
    pub const BIND: u8 = 0x0B;
    pub const DROP: u8 = 0x0C;
    pub const ERROR: u8 = 0x0D;
    pub const LETREC_FIX: u8 = 0x0E;
}

#[derive(Debug)]
pub enum VmError {
    Oom,
    MatchFailure,
    InvalidBytecode,
    NotAClosure,
    StackOverflow,
}

impl From<ArenaError> for VmError {
    fn from(_: ArenaError) -> Self {
        VmError::Oom
    }
}

pub struct Program<'a> {
    pub n_globals: u16,
    pub global_names: &'a [u8],
    pub code: &'a [u8],
}

impl<'a> Program<'a> {
    pub fn from_blob(blob: &'a [u8]) -> Result<Self, VmError> {
        if blob.len() < 2 {
            return Err(VmError::InvalidBytecode);
        }
        let n_globals = u16::from_le_bytes([blob[0], blob[1]]);
        let mut pos = 2usize;
        for _ in 0..n_globals {
            if pos >= blob.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = blob[pos] as usize;
            pos += 1 + name_len + 2;
        }
        Ok(Program {
            n_globals,
            global_names: &blob[2..pos],
            code: &blob[pos..],
        })
    }

    pub fn global_code_offset(&self, idx: u16) -> Result<u16, VmError> {
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            if pos >= self.global_names.len() {
                return Err(VmError::InvalidBytecode);
            }
            let name_len = self.global_names[pos] as usize;
            pos += 1 + name_len;
            if i == idx {
                return Ok(u16::from_le_bytes([
                    self.global_names[pos],
                    self.global_names[pos + 1],
                ]));
            }
            pos += 2;
        }
        Err(VmError::InvalidBytecode)
    }

    pub fn global_index(&self, name: &str) -> Option<u16> {
        let name_bytes = name.as_bytes();
        let mut pos = 0usize;
        for i in 0..self.n_globals {
            if pos >= self.global_names.len() {
                return None;
            }
            let name_len = self.global_names[pos] as usize;
            let entry_name = &self.global_names[pos + 1..pos + 1 + name_len];
            if entry_name == name_bytes {
                return Some(i);
            }
            pos += 1 + name_len + 2;
        }
        None
    }
}

struct CallFrame {
    return_pc: usize,
    frame_base: usize,
}

pub struct Vm<'buf> {
    pub arena: Arena<'buf>,
    globals: [Value; 64],
    n_globals: u16,
    code: &'buf [u8],
}

const MAX_CALL_DEPTH: usize = 256;

impl<'buf> Vm<'buf> {
    pub fn new(buf: &'buf mut [u8]) -> Self {
        Vm {
            arena: Arena::new(buf),
            globals: [Value::immediate(0); 64],
            n_globals: 0,
            code: &[],
        }
    }

    pub fn reset(&mut self) {
        self.arena.reset();
    }

    pub fn load_program(&mut self, prog: &Program<'buf>) -> Result<(), VmError> {
        self.n_globals = prog.n_globals;
        self.code = prog.code;
        for i in 0..prog.n_globals {
            let offset = prog.global_code_offset(i)?;
            let val = self.eval(offset as usize)?;
            self.globals[i as usize] = val;
        }
        Ok(())
    }

    pub fn call(&mut self, global_idx: u16, args: &[Value]) -> Result<Value, VmError> {
        let mut func = self.globals[global_idx as usize];
        for &arg in args {
            if !func.is_closure() {
                return Err(VmError::NotAClosure);
            }
            let code_addr = self.arena.closure_code(func);
            let n_cap = self.arena.closure_capture_count(func);
            let saved_depth = self.arena.stack_depth();
            for i in 0..n_cap {
                self.arena.stack_push(self.arena.closure_capture(func, i))?;
            }
            self.arena.stack_push(arg)?;
            func = self.eval_with_frame(code_addr as usize, saved_depth)?;
        }
        Ok(func)
    }

    pub fn tuple_field(&self, val: Value, idx: usize) -> Value {
        self.arena.tuple_field(val, idx)
    }

    pub fn nil(&self) -> Value {
        Value::immediate(crate::value::tags::NIL)
    }

    pub fn alloc_tuple(&mut self, tag: u8, fields: &[Value]) -> Result<Value, VmError> {
        Ok(self.arena.alloc_tuple(tag, fields)?)
    }

    fn eval(&mut self, start_pc: usize) -> Result<Value, VmError> {
        self.eval_with_frame(start_pc, self.arena.stack_depth())
    }

    fn eval_with_frame(&mut self, start_pc: usize, frame_base: usize) -> Result<Value, VmError> {
        let code = self.code;
        let mut pc = start_pc;
        let mut call_stack: [CallFrame; MAX_CALL_DEPTH] = core::array::from_fn(|_| CallFrame {
            return_pc: 0,
            frame_base: 0,
        });
        let mut call_depth: usize = 0;
        let mut frame_base = frame_base;

        loop {
            if pc >= code.len() {
                return Err(VmError::InvalidBytecode);
            }

            let opcode = code[pc];
            pc += 1;

            match opcode {
                op::IMM => {
                    let tag = code[pc];
                    pc += 1;
                    self.arena.stack_push(Value::immediate(tag))?;
                }

                op::TUPLE => {
                    let tag = code[pc];
                    let arity = code[pc + 1] as usize;
                    pc += 2;
                    let mut fields = [Value::immediate(0); 8];
                    for i in (0..arity).rev() {
                        fields[i] = self.arena.stack_pop();
                    }
                    let val = self.arena.alloc_tuple(tag, &fields[..arity])?;
                    self.arena.stack_push(val)?;
                }

                op::LOAD => {
                    let idx = code[pc] as usize;
                    pc += 1;
                    let depth_from_top = self.arena.stack_depth() - frame_base - 1 - idx;
                    let val = self.arena.stack_peek(depth_from_top);
                    self.arena.stack_push(val)?;
                }

                op::GLOBAL => {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    pc += 2;
                    self.arena.stack_push(self.globals[idx])?;
                }

                op::CLOSURE => {
                    let code_addr = u16::from_le_bytes([code[pc], code[pc + 1]]);
                    let n_cap = code[pc + 2] as usize;
                    pc += 3;
                    let mut caps = [Value::immediate(0); 16];
                    for i in (0..n_cap).rev() {
                        caps[i] = self.arena.stack_pop();
                    }
                    let val = self.arena.alloc_closure(code_addr, &caps[..n_cap])?;
                    self.arena.stack_push(val)?;
                }

                op::APPLY => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();
                    if !func.is_closure() {
                        return Err(VmError::NotAClosure);
                    }
                    if call_depth >= MAX_CALL_DEPTH {
                        return Err(VmError::StackOverflow);
                    }
                    call_stack[call_depth] = CallFrame {
                        return_pc: pc,
                        frame_base,
                    };
                    call_depth += 1;

                    let ca = self.arena.closure_code(func);
                    let n_cap = self.arena.closure_capture_count(func);
                    frame_base = self.arena.stack_depth();
                    for i in 0..n_cap {
                        self.arena.stack_push(self.arena.closure_capture(func, i))?;
                    }
                    self.arena.stack_push(arg)?;
                    pc = ca as usize;
                }

                op::TAIL_APPLY => {
                    let arg = self.arena.stack_pop();
                    let func = self.arena.stack_pop();
                    if !func.is_closure() {
                        return Err(VmError::NotAClosure);
                    }
                    self.arena.stack_truncate(frame_base);

                    let ca = self.arena.closure_code(func);
                    let n_cap = self.arena.closure_capture_count(func);
                    frame_base = self.arena.stack_depth();
                    for i in 0..n_cap {
                        self.arena.stack_push(self.arena.closure_capture(func, i))?;
                    }
                    self.arena.stack_push(arg)?;
                    pc = ca as usize;
                }

                op::RET => {
                    let result = self.arena.stack_pop();
                    self.arena.stack_truncate(frame_base);
                    self.arena.stack_push(result)?;

                    if call_depth == 0 {
                        return Ok(result);
                    }
                    call_depth -= 1;
                    pc = call_stack[call_depth].return_pc;
                    frame_base = call_stack[call_depth].frame_base;
                }

                op::MATCH => {
                    let n_cases = code[pc] as usize;
                    pc += 1;
                    let table_start = pc;
                    pc += n_cases * 4;

                    let scrutinee = self.arena.stack_pop();
                    let scrutinee_tag = scrutinee.tag();

                    let mut matched = false;
                    for i in 0..n_cases {
                        let entry = table_start + i * 4;
                        let case_tag = code[entry];
                        let case_arity = code[entry + 1] as usize;
                        let case_offset =
                            u16::from_le_bytes([code[entry + 2], code[entry + 3]]) as usize;

                        if case_tag == scrutinee_tag {
                            // If the case has bindings, push the scrutinee back
                            // so the subsequent BIND can extract fields.
                            if case_arity > 0 {
                                self.arena.stack_push(scrutinee)?;
                            }
                            pc = case_offset;
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        return Err(VmError::MatchFailure);
                    }
                }

                op::JMP => {
                    pc = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                }

                op::BIND => {
                    let n = code[pc] as usize;
                    pc += 1;
                    // Pop the scrutinee (pushed back by MATCH), push its fields.
                    let scrutinee = self.arena.stack_pop();
                    for i in 0..n {
                        let field = self.arena.tuple_field(scrutinee, i);
                        self.arena.stack_push(field)?;
                    }
                }

                op::DROP => {
                    let n = code[pc] as usize;
                    pc += 1;
                    let depth = self.arena.stack_depth();
                    self.arena.stack_truncate(depth - n);
                }

                op::ERROR => {
                    return Err(VmError::MatchFailure);
                }

                op::LETREC_FIX => {
                    let cap_idx = code[pc] as usize;
                    pc += 1;
                    // TOS = the closure (letrec val).
                    // Position 1 from TOS = the dummy placeholder.
                    let closure = self.arena.stack_peek(0);
                    if cap_idx != 0xFF {
                        self.arena.closure_set_capture(closure, cap_idx, closure);
                    }
                    // Overwrite the dummy with the closure.
                    self.arena.stack_set(1, closure);
                    // Pop the extra copy.
                    self.arena.stack_pop();
                }

                _ => return Err(VmError::InvalidBytecode),
            }
        }
    }
}
