// dixscript/src/Builtins/Instance/mod.rs

pub mod number_methods;
pub mod string_methods;
pub mod array_methods;
pub mod tuple_methods;
pub mod universal_methods;
pub mod regex_methods;
pub mod blob_methods;
pub mod datetime_instance_methods;
pub mod object_methods;

pub use number_methods::{
    get_int_methods,
    get_long_methods,
    get_float_methods,
    get_double_methods,
};
pub use string_methods::get_methods    as get_string_methods;
pub use array_methods::get_methods     as get_array_methods;
pub use tuple_methods::get_methods     as get_tuple_methods;
pub use universal_methods::get_methods as get_universal_methods;
pub use regex_methods::get_methods     as get_regex_methods;
pub use blob_methods::get_methods      as get_blob_methods;
pub use datetime_instance_methods::{
    get_timestamp_methods,
    get_date_methods,
};
pub use object_methods::get_methods    as get_object_methods;
