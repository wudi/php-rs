use crate::core::value::{Symbol, Val};

pub const ATTRIBUTE_TARGET_CLASS: u32 = 1 << 0;
pub const ATTRIBUTE_TARGET_FUNCTION: u32 = 1 << 1;
pub const ATTRIBUTE_TARGET_METHOD: u32 = 1 << 2;
pub const ATTRIBUTE_TARGET_PROPERTY: u32 = 1 << 3;
pub const ATTRIBUTE_TARGET_CLASS_CONST: u32 = 1 << 4;
pub const ATTRIBUTE_TARGET_PARAMETER: u32 = 1 << 5;
pub const ATTRIBUTE_TARGET_CONST: u32 = 1 << 6;
pub const ATTRIBUTE_TARGET_ALL: u32 = (1 << 7) - 1;
pub const ATTRIBUTE_IS_REPEATABLE: u32 = 1 << 7;

#[derive(Debug, Clone)]
pub struct AttributeArg {
    pub name: Option<Symbol>,
    pub value: Val,
}

#[derive(Debug, Clone)]
pub struct AttributeInstance {
    pub name: Symbol,
    pub lc_name: Symbol,
    pub args: Vec<AttributeArg>,
    pub target: u32,
}

#[derive(Debug, Clone)]
pub struct AttributeClassInfo {
    pub targets: u32,
    pub is_repeatable: bool,
}
