// src/Builtins/Static/mod.rs
//! Static - Static objects (Math, DateTime, Array, Dix, etc.)

pub mod static_object_base;
pub mod dix_object;
pub mod datetime_object;
pub mod array_object;
pub mod random_object;
pub mod enum_object;
pub mod guid_object;
pub mod ip_address_object;
pub mod static_object_registry;
mod math_object;
mod datetime_object;
mod array_object;
mod random_object;
mod enum_object;
mod guid_object;

// Re-export the trait and base
pub use static_object_base::{IStaticObject, StaticObjectBase};

// Re-export static objects
pub use dix_object::DixObject;
pub use math_object::MathObject;
pub use datetime_object::DateTimeObject;
pub use array_object::ArrayObject;
pub use random_object::RandomObject;
pub use enum_object::EnumObject;
pub use guid_object::GuidObject;
pub use ip_address_object::IpAddressObject;

// Re-export registry functions
pub use static_object_registry::{
    call_static_method, get_static_object, has_static_method, has_static_object,
    initialize_static_registry, ValidationResult,
};