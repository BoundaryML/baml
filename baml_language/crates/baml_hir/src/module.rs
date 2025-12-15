//! Module support for HIR.
//!
//! This module provides types for representing modules and their contents,
//! which will be used for path resolution in THIR.

use std::collections::HashMap;

use crate::Name;

/// A resolved module containing named items.
/// This is the "compiled" view of a module after name resolution.
#[derive(Debug, Clone, Default)]
pub struct Module {
    /// Enum types defined in this module
    pub enums: HashMap<Name, ModuleEnumId>,

    /// Class types defined in this module
    pub classes: HashMap<Name, ModuleClassId>,

    /// Functions defined in this module
    pub functions: HashMap<Name, ModuleFunctionId>,

    /// Sub-modules (for hierarchical structure)
    pub submodules: HashMap<Name, Module>,
}

impl Module {
    /// Create a new empty module.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up an enum by name.
    pub fn get_enum(&self, name: &Name) -> Option<ModuleEnumId> {
        self.enums.get(name).copied()
    }

    /// Look up a class by name.
    pub fn get_class(&self, name: &Name) -> Option<ModuleClassId> {
        self.classes.get(name).copied()
    }

    /// Look up a function by name.
    pub fn get_function(&self, name: &Name) -> Option<ModuleFunctionId> {
        self.functions.get(name).copied()
    }

    /// Look up a submodule by name.
    pub fn get_submodule(&self, name: &Name) -> Option<&Module> {
        self.submodules.get(name)
    }

    /// Insert an enum into the module.
    pub fn insert_enum(&mut self, name: Name, id: ModuleEnumId) {
        self.enums.insert(name, id);
    }

    /// Insert a class into the module.
    pub fn insert_class(&mut self, name: Name, id: ModuleClassId) {
        self.classes.insert(name, id);
    }

    /// Insert a function into the module.
    pub fn insert_function(&mut self, name: Name, id: ModuleFunctionId) {
        self.functions.insert(name, id);
    }

    /// Insert a submodule into the module.
    pub fn insert_submodule(&mut self, name: Name, module: Module) {
        self.submodules.insert(name, module);
    }
}

/// IDs for items within a module.
/// These will be replaced with proper database IDs once we have that infrastructure.
pub type ModuleEnumId = u32;
pub type ModuleClassId = u32;
pub type ModuleFunctionId = u32;
