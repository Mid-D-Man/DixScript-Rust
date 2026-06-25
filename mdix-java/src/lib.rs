// mdix-java/src/lib.rs
//
// JNI bridge between the JVM and the DixScript runtime.
//
// Naming convention (mandatory for JNI):
//   Java_<package_underscores>_<ClassName>_<methodName>
//
// Java class: com.midmanstudio.dixscript.internal.MdixNative
// → prefix:   Java_com_midmanstudio_dixscript_internal_MdixNative_
//
// Handles are passed as jlong (i64 on all platforms).
// Box::into_raw → pointer cast to i64 on the way out.
// i64 cast → pointer → Box::from_raw on the way in (free) or &* (read).
//
// All functions follow the same error reporting convention:
//   - success: return the value (0 / false / null for numeric/bool/object)
//   - failure: call set_exception() and return the zero value
//
// set_exception() calls env.throw_new(MdixException, message) which schedules
// a JVM exception — the JNI spec says we must return immediately after.

#![allow(non_snake_case)]

use jni::objects::{JClass, JString, JObject, JObjectArray};
use jni::sys::{jboolean, jdouble, jfloat, jint, jlong, jstring, JNI_TRUE, JNI_FALSE};
use jni::JNIEnv;

use dixscript::Runtime::{
    DixCompactor, DixConverter, DixFormatOptions, DixLoader, DixLoadOptions, DixValue,
};

// ── Handle types ──────────────────────────────────────────────────────────────

struct ReadHandle {
    data: dixscript::Runtime::DixData,
}

struct BuilderHandle {
    entries: std::collections::HashMap<String, DixValue>,
}

fn box_read(data: dixscript::Runtime::DixData) -> jlong {
    Box::into_raw(Box::new(ReadHandle { data })) as jlong
}

unsafe fn as_read<'a>(handle: jlong) -> Option<&'a ReadHandle> {
    if handle == 0 { None } else { Some(&*(handle as *const ReadHandle)) }
}

unsafe fn free_read(handle: jlong) {
    if handle != 0 { drop(Box::from_raw(handle as *mut ReadHandle)); }
}

fn box_builder() -> jlong {
    Box::into_raw(Box::new(BuilderHandle {
        entries: std::collections::HashMap::new(),
    })) as jlong
}

unsafe fn as_builder<'a>(handle: jlong) -> Option<&'a BuilderHandle> {
    if handle == 0 { None } else { Some(&*(handle as *const BuilderHandle)) }
}

unsafe fn as_builder_mut<'a>(handle: jlong) -> Option<&'a mut BuilderHandle> {
    if handle == 0 { None } else { Some(&mut *(handle as *mut BuilderHandle)) }
}

unsafe fn free_builder(handle: jlong) {
    if handle != 0 { drop(Box::from_raw(handle as *mut BuilderHandle)); }
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn set_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("com/midmanstudio/dixscript/MdixException", msg);
}

fn jstr(env: &mut JNIEnv, s: JString) -> Option<String> {
    env.get_string(&s).ok().map(|v| v.into())
}

fn to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s)
        .map(|js| js.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

// ── Metadata ──────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_version<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    to_jstring(&mut env, "1.0.0")
}

// ── Load / Free ───────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_load<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) -> jlong {
    let path_str = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "load: path is null"); return 0; }
    };
    let loader = DixLoader::new();
    match loader.load_text(&path_str, &DixLoadOptions::new()) {
        Ok(data) => box_read(data),
        Err(e) => { set_exception(&mut env, &format!("load: {}", e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_loadStr<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    source: JString<'local>,
) -> jlong {
    let src = match jstr(&mut env, source) {
        Some(s) => s,
        None => { set_exception(&mut env, "loadStr: source is null"); return 0; }
    };
    let loader = DixLoader::new();
    match loader.load_from_str(&src, &DixLoadOptions::new()) {
        Ok(data) => box_read(data),
        Err(e) => { set_exception(&mut env, &format!("loadStr: {}", e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_loadEncrypted<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    enc_path: JString<'local>,
    key_path: JString<'local>,
) -> jlong {
    let enc = match jstr(&mut env, enc_path) {
        Some(s) => s,
        None => { set_exception(&mut env, "loadEncrypted: encPath is null"); return 0; }
    };
    let mut opts = DixLoadOptions::new();
    if let Ok(kp) = env.get_string(&key_path) {
        let kp_str: String = kp.into();
        if !kp_str.is_empty() {
            opts.key_file_path = Some(kp_str);
        }
    }
    let loader = DixLoader::new();
    match loader.load_encrypted(&enc, &opts) {
        Ok(data) => box_read(data),
        Err(e) => { set_exception(&mut env, &format!("loadEncrypted: {}", e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_loadEncryptedPassword<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    enc_path: JString<'local>,
    password: JString<'local>,
) -> jlong {
    let enc = match jstr(&mut env, enc_path) {
        Some(s) => s,
        None => { set_exception(&mut env, "loadEncryptedPassword: encPath is null"); return 0; }
    };
    let pw = match jstr(&mut env, password) {
        Some(s) => s,
        None => { set_exception(&mut env, "loadEncryptedPassword: password is null"); return 0; }
    };
    let opts = DixLoadOptions::with_password(&pw);
    let loader = DixLoader::new();
    match loader.load_encrypted(&enc, &opts) {
        Ok(data) => box_read(data),
        Err(e) => { set_exception(&mut env, &format!("loadEncryptedPassword: {}", e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_free<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    unsafe { free_read(handle); }
}

// ── Validity / metadata ───────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_isValid<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jboolean {
    (handle != 0) as jboolean
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_entryCount<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jint {
    match unsafe { as_read(handle) } {
        Some(h) => h.data.entry_count() as jint,
        None => -1,
    }
}

// ── Type inspection ───────────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getType<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jint {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => return -1,
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => return -1,
    };
    match h.data.get_value(&p) {
        None                         => -1,
        Some(DixValue::Null)         => 0,
        Some(DixValue::Bool(_))      => 1,
        Some(DixValue::Int(_))       => 2,
        Some(DixValue::Long(_))      => 3,
        Some(DixValue::Float(_))     => 4,
        Some(DixValue::Double(_))    => 5,
        Some(DixValue::String(_))    => 6,
        Some(DixValue::Date(_))      => 7,
        Some(DixValue::Timestamp(_)) => 8,
        Some(DixValue::HexColor(_))  => 9,
        Some(DixValue::Blob(_))      => 10,
        Some(DixValue::Regex(_))     => 11,
        Some(DixValue::Array(_))     => 12,
        Some(DixValue::Object(_))    => 13,
        Some(DixValue::Tuple(_))     => 14,
        Some(DixValue::Enum { .. })  => 15,
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getArrayLength<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jint {
    let h = match unsafe { as_read(handle) } { Some(h) => h, None => return -1 };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return -1 };
    match h.data.get_value(&p) {
        Some(DixValue::Array(arr)) => arr.len() as jint,
        _ => -1,
    }
}

// ── Typed getters ─────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getString<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getString: null handle"); return std::ptr::null_mut(); }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getString: null path"); return std::ptr::null_mut(); }
    };
    match h.data.get::<String>(&p) {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => { set_exception(&mut env, &format!("getString('{}'): {}", p, e)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getInt<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jint {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getInt: null handle"); return 0; }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getInt: null path"); return 0; }
    };
    match h.data.get::<i32>(&p) {
        Ok(v) => v as jint,
        Err(e) => { set_exception(&mut env, &format!("getInt('{}'): {}", p, e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getLong<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jlong {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getLong: null handle"); return 0; }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getLong: null path"); return 0; }
    };
    match h.data.get::<i64>(&p) {
        Ok(v) => v as jlong,
        Err(e) => { set_exception(&mut env, &format!("getLong('{}'): {}", p, e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getFloat<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jfloat {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getFloat: null handle"); return 0.0; }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getFloat: null path"); return 0.0; }
    };
    match h.data.get::<f64>(&p) {
        Ok(v) => v as jfloat,
        Err(e) => { set_exception(&mut env, &format!("getFloat('{}'): {}", p, e)); 0.0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getDouble<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jdouble {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getDouble: null handle"); return 0.0; }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getDouble: null path"); return 0.0; }
    };
    match h.data.get::<f64>(&p) {
        Ok(v) => v as jdouble,
        Err(e) => { set_exception(&mut env, &format!("getDouble('{}'): {}", p, e)); 0.0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getBool<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jboolean {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getBool: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getBool: null path"); return JNI_FALSE; }
    };
    match h.data.get::<bool>(&p) {
        Ok(v) => if v { JNI_TRUE } else { JNI_FALSE },
        Err(e) => { set_exception(&mut env, &format!("getBool('{}'): {}", p, e)); JNI_FALSE }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getEnumName<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getEnumName: null handle"); return std::ptr::null_mut(); }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getEnumName: null path"); return std::ptr::null_mut(); }
    };
    match h.data.get_value(&p) {
        Some(DixValue::Enum { enum_name, .. }) => to_jstring(&mut env, enum_name),
        _ => { set_exception(&mut env, &format!("getEnumName('{}'): not an enum", p)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getEnumField<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getEnumField: null handle"); return std::ptr::null_mut(); }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getEnumField: null path"); return std::ptr::null_mut(); }
    };
    match h.data.get_value(&p) {
        Some(DixValue::Enum { field_name, .. }) => to_jstring(&mut env, field_name),
        _ => { set_exception(&mut env, &format!("getEnumField('{}'): not an enum", p)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getJson: null handle"); return std::ptr::null_mut(); }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "getJson: null path"); return std::ptr::null_mut(); }
    };
    match h.data.get_value(&p) {
        None => { set_exception(&mut env, &format!("getJson('{}'): path not found", p)); std::ptr::null_mut() }
        Some(v) => match serde_json::to_string(v) {
            Ok(s) => to_jstring(&mut env, &s),
            Err(e) => { set_exception(&mut env, &format!("getJson('{}'): {}", p, e)); std::ptr::null_mut() }
        }
    }
}

// ── Key existence / enumeration ───────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_exists<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jboolean {
    let h = match unsafe { as_read(handle) } { Some(h) => h, None => return JNI_FALSE };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    if h.data.exists(&p) { JNI_TRUE } else { JNI_FALSE }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_getKeys<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    prefix: JString<'local>,
) -> JObjectArray<'local> {
    let null_arr = unsafe { JObjectArray::from_raw(std::ptr::null_mut()) };

    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "getKeys: null handle"); return null_arr; }
    };
    let pfx = jstr(&mut env, prefix).unwrap_or_default();
    let keys = h.data.get_keys(&pfx);

    let string_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return null_arr,
    };
    let arr = match env.new_object_array(keys.len() as i32, &string_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => return null_arr,
    };
    for (i, key) in keys.iter().enumerate() {
        if let Ok(js) = env.new_string(key) {
            let _ = env.set_object_array_element(&arr, i as i32, js);
        }
    }
    arr
}

// ── Conversion — export ───────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_toJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    indented: jboolean,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "toJson: null handle"); return std::ptr::null_mut(); }
    };
    let entries = h.data.to_hashmap();
    let converter = DixConverter::new();
    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("toJson: {}", e)); return std::ptr::null_mut(); }
    };
    let map = converter.to_hashmap(&ast);
    let result = if indented == JNI_TRUE {
        serde_json::to_string_pretty(&map)
    } else {
        serde_json::to_string(&map)
    };
    match result {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => { set_exception(&mut env, &format!("toJson: {}", e)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_toMdix<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    mode: jint,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "toMdix: null handle"); return std::ptr::null_mut(); }
    };
    let entries = h.data.to_hashmap();
    let converter = DixConverter::new();
    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("toMdix: {}", e)); return std::ptr::null_mut(); }
    };
    let opts = match mode {
        1 => DixFormatOptions::pretty(),
        2 => DixFormatOptions::compact(),
        3 => DixFormatOptions::minified(),
        _ => DixFormatOptions::new(),
    };
    match converter.to_mdix(&ast, Some(&opts)) {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => { set_exception(&mut env, &format!("toMdix: {}", e)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_toToml<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => { set_exception(&mut env, "toToml: null handle"); return std::ptr::null_mut(); }
    };
    let entries = h.data.to_hashmap();
    let converter = DixConverter::new();
    let ast = match converter.from_hashmap(entries) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("toToml: {}", e)); return std::ptr::null_mut(); }
    };
    match converter.to_toml(&ast) {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => { set_exception(&mut env, &format!("toToml: {}", e)); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_fromJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> jlong {
    let src = match jstr(&mut env, json) {
        Some(s) => s,
        None => { set_exception(&mut env, "fromJson: null source"); return 0; }
    };
    let converter = DixConverter::new();
    let ast = match converter.from_json(&src) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("fromJson: {}", e)); return 0; }
    };
    let mdix_src = match converter.to_mdix(&ast, None) {
        Ok(s) => s,
        Err(e) => { set_exception(&mut env, &format!("fromJson: re-serialize failed: {}", e)); return 0; }
    };
    let loader = DixLoader::new();
    match loader.load_from_str(&mdix_src, &DixLoadOptions::new()) {
        Ok(data) => box_read(data),
        Err(e) => { set_exception(&mut env, &format!("fromJson: load failed: {}", e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_fromToml<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    toml: JString<'local>,
) -> jlong {
    let src = match jstr(&mut env, toml) {
        Some(s) => s,
        None => { set_exception(&mut env, "fromToml: null source"); return 0; }
    };
    let converter = DixConverter::new();
    let ast = match converter.from_toml(&src) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("fromToml: {}", e)); return 0; }
    };
    let mdix_src = match converter.to_mdix(&ast, None) {
        Ok(s) => s,
        Err(e) => { set_exception(&mut env, &format!("fromToml: re-serialize: {}", e)); return 0; }
    };
    let loader = DixLoader::new();
    match loader.load_from_str(&mdix_src, &DixLoadOptions::new()) {
        Ok(data) => box_read(data),
        Err(e) => { set_exception(&mut env, &format!("fromToml: load: {}", e)); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_formatSource<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    source: JString<'local>,
    mode: jint,
) -> jstring {
    let src = match jstr(&mut env, source) {
        Some(s) => s,
        None => { set_exception(&mut env, "formatSource: null source"); return std::ptr::null_mut(); }
    };
    let result = if mode == 3 {
        DixCompactor::minify(&src)
    } else {
        DixCompactor::compact(&src)
    };
    to_jstring(&mut env, &result)
}

// ── Builder ───────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderNew<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jlong {
    box_builder()
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderFree<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    unsafe { free_builder(handle); }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSetString<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
    value: JString<'local>,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSetString: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    let v = match jstr(&mut env, value) { Some(s) => s, None => return JNI_FALSE };
    b.entries.insert(p, DixValue::String(v));
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSetInt<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
    value: jint,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSetInt: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    b.entries.insert(p, DixValue::Int(value));
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSetLong<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
    value: jlong,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSetLong: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    b.entries.insert(p, DixValue::Long(value));
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSetFloat<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
    value: jfloat,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSetFloat: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    b.entries.insert(p, DixValue::Float(value));
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSetDouble<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
    value: jdouble,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSetDouble: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    b.entries.insert(p, DixValue::Double(value));
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSetBool<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
    value: jboolean,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSetBool: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    b.entries.insert(p, DixValue::Bool(value == JNI_TRUE));
    JNI_TRUE
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderRemove<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jboolean {
    let b = match unsafe { as_builder_mut(handle) } {
        Some(b) => b,
        None => return JNI_FALSE,
    };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    if b.entries.remove(&p).is_some() { JNI_TRUE } else { JNI_FALSE }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderClear<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    if let Some(b) = unsafe { as_builder_mut(handle) } {
        b.entries.clear();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderHasKey<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jboolean {
    let b = match unsafe { as_builder(handle) } { Some(b) => b, None => return JNI_FALSE };
    let p = match jstr(&mut env, path) { Some(s) => s, None => return JNI_FALSE };
    if b.entries.contains_key(&p) { JNI_TRUE } else { JNI_FALSE }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderSave<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    path: JString<'local>,
) -> jboolean {
    let b = match unsafe { as_builder(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderSave: null handle"); return JNI_FALSE; }
    };
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => { set_exception(&mut env, "builderSave: null path"); return JNI_FALSE; }
    };
    let converter = DixConverter::new();
    let ast = match converter.from_hashmap(b.entries.clone()) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("builderSave: {}", e)); return JNI_FALSE; }
    };
    let content = match converter.to_mdix(&ast, None) {
        Ok(s) => s,
        Err(e) => { set_exception(&mut env, &format!("builderSave: {}", e)); return JNI_FALSE; }
    };
    if let Some(parent) = std::path::Path::new(&p).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&p, content) {
        Ok(()) => JNI_TRUE,
        Err(e) => { set_exception(&mut env, &format!("builderSave: write failed: {}", e)); JNI_FALSE }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderToString<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jstring {
    let b = match unsafe { as_builder(handle) } {
        Some(b) => b,
        None => { set_exception(&mut env, "builderToString: null handle"); return std::ptr::null_mut(); }
    };
    let converter = DixConverter::new();
    let ast = match converter.from_hashmap(b.entries.clone()) {
        Ok(a) => a,
        Err(e) => { set_exception(&mut env, &format!("builderToString: {}", e)); return std::ptr::null_mut(); }
    };
    match converter.to_mdix(&ast, Some(&DixFormatOptions::pretty())) {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => { set_exception(&mut env, &format!("builderToString: {}", e)); std::ptr::null_mut() }
    }
    }
