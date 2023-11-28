// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod absint;
pub mod ast;
mod borrows;
pub mod cfg;
mod liveness;
mod locals;
mod remove_no_ops;
pub(crate) mod translate;
pub mod visitor;

mod optimize;

use crate::{
    expansion::ast::{AbilitySet, ModuleIdent},
    hlir::ast::{FunctionSignature, Label, SingleType, Var},
    parser::ast::StructName,
    shared::{unique_map::UniqueMap, CompilationEnv, Name},
};
use cfg::*;
use optimize::optimize;
use std::collections::BTreeSet;

pub struct CFGContext<'a> {
    pub module: Option<ModuleIdent>,
    pub member: MemberName,
    pub struct_declared_abilities: &'a UniqueMap<ModuleIdent, UniqueMap<StructName, AbilitySet>>,
    pub signature: &'a FunctionSignature,
    pub locals: &'a UniqueMap<Var, SingleType>,
    pub infinite_loop_starts: &'a BTreeSet<Label>,
}

pub enum MemberName {
    Constant(Name),
    Function(Name),
}

pub fn refine_inference_and_verify(
    env: &mut CompilationEnv,
    context: &CFGContext,
    cfg: &mut MutForwardCFG,
) {
    liveness::last_usage(env, context, cfg);
    let locals_states = locals::verify(env, context, cfg);

    liveness::release_dead_refs(context, &locals_states, cfg);
    borrows::verify(env, context, cfg);
}

impl MemberName {
    pub fn name(&self) -> Name {
        match self {
            MemberName::Constant(n) | MemberName::Function(n) => *n,
        }
    }
}
