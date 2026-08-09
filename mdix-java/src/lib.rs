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
    ArrayMergeStrategy, DixCompactor, DixConverter, DixData, DixFormatOptions, DixLoader,
    DixLoadOptions, DixValue, ExpectedValueType, HotReloadWatcher, MdixMergeInput, MdixMerger,
    MdixMergeStrategy, SchemaBuilder,
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

struct WatcherHandle {
    watcher: HotReloadWatcher,
}

fn box_watcher(watcher: HotReloadWatcher) -> jlong {
    Box::into_raw(Box::new(WatcherHandle { watcher })) as jlong
}

unsafe fn as_watcher<'a>(handle: jlong) -> Option<&'a HotReloadWatcher> {
    if handle == 0 { None } else { Some(&(*(handle as *const WatcherHandle)).watcher) }
}

unsafe fn as_watcher_mut<'a>(handle: jlong) -> Option<&'a mut HotReloadWatcher> {
    if handle == 0 { None } else { Some(&mut (*(handle as *mut WatcherHandle)).watcher) }
}

unsafe fn free_watcher(handle: jlong) {
    if handle != 0 { drop(Box::from_raw(handle as *mut WatcherHandle)); }
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
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_builderEntryCount<
    'local,
>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jint {
    match unsafe { as_builder(handle) } {
        Some(b) => b.entries.len() as jint,
        None => 0,
    }
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

// ════════════════════════════════════════════════════════════════════════════
// Query — LINQ-style querying over array data
// ════════════════════════════════════════════════════════════════════════════
//
// `query(path)` needs no new native surface at all: `Database.getJson(path)`
// on an array-shaped path already returns the full array as JSON (see
// `getJson` above), and `MdixQuery.java` parses that into a `List<MdixValue>`
// and applies filter/sort/group/aggregate entirely in managed code — the
// same shape MidManStudio.Mdix.Core's C# `MdixQuery.cs` uses (fetch the
// typed list via `GetArray<T>`, then plain C# LINQ) rather than trying to
// marshal a Java `Predicate`/`Comparator` across the JNI boundary as a
// per-call callback.
//
// `query_many(pattern)` is different: it gathers *sibling* paths sharing
// structure via `DixData::select_many`, whole-segment `*` glob syntax that
// managed code has no way to replicate without reimplementing the matcher.
// One native call, mirroring mdix-ffi's `mdix_select_many_as_json` byte for
// byte in output shape (a JSON array of the matched `DixValue`s).

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_selectManyAsJson<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    pattern: JString<'local>,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => {
            set_exception(&mut env, "selectManyAsJson: null handle");
            return std::ptr::null_mut();
        }
    };
    let pat = match jstr(&mut env, pattern) {
        Some(s) => s,
        None => {
            set_exception(&mut env, "selectManyAsJson: pattern is null");
            return std::ptr::null_mut();
        }
    };
    let items: Vec<DixValue> = h.data.select_many::<DixValue>(&pat);
    match serde_json::to_string(&items) {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => {
            set_exception(&mut env, &format!("selectManyAsJson('{}'): {}", pat, e));
            std::ptr::null_mut()
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Merge — AST-level weighted merge of multiple DixScript sources
// ════════════════════════════════════════════════════════════════════════════
//
// Reimplements mdix-ffi/src/merge.rs's `run_merge()` against this crate's
// own `ReadHandle` instead of mdix-ffi's `MdixHandle` — same reasoning that
// file's own doc comment gives for not reimplementing `MdixMerger` in
// managed code: weighted-priority conflict resolution, per-source conflict
// reporting, configurable array-merge strategy, and full `DixValue` type
// fidelity (Long / Float / Double / HexColor / Blob / Regex / Date /
// Timestamp / Enum) are only available from the real AST-level merger.
// See mdix-ffi/src/merge.rs for the reference implementation this mirrors —
// keep the two in sync if merge semantics ever change.
//
// Takes SOURCE STRINGS, not existing loaded handles or file paths:
// `MdixMerger` operates on freshly-parsed ASTs, and an already-loaded
// `ReadHandle` only retains the resolved `DixData`, not the AST it came
// from. To merge two already-loaded databases, round-trip each back to
// source text with `toMdix()` first (`Merge.java` does exactly this).

fn merge_strategy_from_i32(v: jint) -> MdixMergeStrategy {
    match v {
        1 => MdixMergeStrategy::PrimaryWins,
        2 => MdixMergeStrategy::SecondaryWins,
        3 => MdixMergeStrategy::ThrowOnConflict,
        _ => MdixMergeStrategy::WeightedPriority,
    }
}

fn array_merge_strategy_from_i32(v: jint) -> ArrayMergeStrategy {
    match v {
        1 => ArrayMergeStrategy::Concat,
        2 => ArrayMergeStrategy::ConcatDedup,
        _ => ArrayMergeStrategy::Replace,
    }
}

/// Reads every element of a Java `String[]` into an owned `Vec<String>`.
/// `None` on any JNI failure (bad element, non-UTF-8, etc.) — the caller is
/// responsible for raising the exception with call-site-specific context.
fn read_jstring_array(env: &mut JNIEnv, arr: &JObjectArray) -> Option<Vec<String>> {
    let len = env.get_array_length(arr).ok()?;
    let mut out = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len {
        let elem = env.get_object_array_element(arr, i).ok()?;
        let js: JString = elem.into();
        out.push(jstr(env, js)?);
    }
    Some(out)
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_mergeSources<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    sources: JObjectArray<'local>,
    weights: JObjectArray<'local>,
    strategy: jint,
    array_strategy: jint,
) -> JObjectArray<'local> {
    let null_arr = unsafe { JObjectArray::from_raw(std::ptr::null_mut()) };

    let source_strings = match read_jstring_array(&mut env, &sources) {
        Some(v) if !v.is_empty() => v,
        Some(_) => {
            set_exception(&mut env, "mergeSources: sources is empty");
            return null_arr;
        }
        None => {
            set_exception(&mut env, "mergeSources: invalid sources array");
            return null_arr;
        }
    };

    // `weights` is `null` from the Java side (JObject.null()-backed array)
    // when the caller wants auto-descending weights — mirrors mdix-ffi's C
    // ABI, where a null `weights` pointer means the same thing.
    let weights_vec: Option<Vec<f64>> = if weights.is_null() {
        None
    } else {
        match read_jstring_array(&mut env, &weights) {
            Some(strs) => {
                let mut parsed = Vec::with_capacity(strs.len());
                for s in strs {
                    match s.parse::<f64>() {
                        Ok(f) => parsed.push(f),
                        Err(_) => {
                            set_exception(&mut env, &format!("mergeSources: invalid weight '{}'", s));
                            return null_arr;
                        }
                    }
                }
                if parsed.len() != source_strings.len() {
                    set_exception(
                        &mut env,
                        &format!(
                            "mergeSources: weights.length ({}) must equal sources.length ({})",
                            parsed.len(),
                            source_strings.len()
                        ),
                    );
                    return null_arr;
                }
                Some(parsed)
            }
            None => {
                set_exception(&mut env, "mergeSources: invalid weights array");
                return null_arr;
            }
        }
    };

    let n = source_strings.len();
    let loader = DixLoader::new();
    let mut inputs = Vec::with_capacity(n);
    for (i, source) in source_strings.into_iter().enumerate() {
        let weight = match &weights_vec {
            Some(w) => w[i],
            None if n == 1 => 1.0,
            None => 1.0 - (i as f64 / (n - 1) as f64),
        };
        let label = format!("source[{}]", i);
        let ast = match loader.compile_to_resolved_ast_from_str(&source, &label) {
            Ok(a) => a,
            Err(e) => {
                set_exception(&mut env, &format!("mergeSources: {}: {}", label, e));
                return null_arr;
            }
        };
        inputs.push(MdixMergeInput::new(ast).with_weight(weight).with_label(label));
    }

    let result = MdixMerger::new()
        .with_strategy(merge_strategy_from_i32(strategy))
        .with_array_strategy(array_merge_strategy_from_i32(array_strategy))
        .merge_all(inputs);

    if !result.is_success {
        set_exception(&mut env, &format!("mergeSources: {}", result.errors.join("; ")));
        return null_arr;
    }

    let conflicts: Vec<serde_json::Value> = result
        .conflicts
        .iter()
        .map(|c| {
            serde_json::json!({
                "path": c.path,
                "winningSource": c.winning_source,
                "winningLabel": c.winning_label,
            })
        })
        .collect();
    let conflicts_json = match serde_json::to_string(&conflicts) {
        Ok(s) => s,
        Err(e) => {
            set_exception(&mut env, &format!("mergeSources: conflict report: {}", e));
            return null_arr;
        }
    };

    let data = DixData::from_ast(
        result.merged_ast,
        "1.0.0".to_string(),
        chrono::Utc::now(),
        false,
        false,
        vec![],
    );
    let handle = box_read(data);

    let string_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => {
            set_exception(&mut env, "mergeSources: internal: String class lookup failed");
            return null_arr;
        }
    };
    let out = match env.new_object_array(2, &string_class, JObject::null()) {
        Ok(a) => a,
        Err(_) => {
            set_exception(&mut env, "mergeSources: internal: result array alloc failed");
            return null_arr;
        }
    };
    if let Ok(js) = env.new_string(handle.to_string()) {
        let _ = env.set_object_array_element(&out, 0, js);
    }
    if let Ok(js) = env.new_string(&conflicts_json) {
        let _ = env.set_object_array_element(&out, 1, js);
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
// Schema — declarative field validation
// ════════════════════════════════════════════════════════════════════════════
//
// `SchemaBuilder::require_with()` / `optional_with()` take a Rust closure
// and can't cross the JNI boundary, so this native call only exposes the
// type/required-checked subset (`require_*` / `optional_*` /
// `with_description`) — everything a schema needs for the common case.
// `com.midmanstudio.dixscript.SchemaBuilder` layers its own pure-Java custom
// validators (`SchemaBuilder.Validator`) on top of this result, entirely in
// managed code, for anyone who needs more than a type check on a field —
// the same closure-can't-cross-JNI tradeoff `MdixQuery` documents above.

fn expected_type_from_str(s: &str) -> ExpectedValueType {
    match s {
        "String" => ExpectedValueType::String,
        "Int" => ExpectedValueType::Int,
        "Long" => ExpectedValueType::Long,
        "Float" => ExpectedValueType::Float,
        "Double" => ExpectedValueType::Double,
        "Bool" => ExpectedValueType::Bool,
        "Array" => ExpectedValueType::Array,
        "Object" => ExpectedValueType::Object,
        "Date" => ExpectedValueType::Date,
        "Timestamp" => ExpectedValueType::Timestamp,
        "HexColor" => ExpectedValueType::HexColor,
        "Blob" => ExpectedValueType::Blob,
        "Regex" => ExpectedValueType::Regex,
        "Enum" => ExpectedValueType::Enum,
        _ => ExpectedValueType::Any,
    }
}

/// `fields_json` shape (produced by `SchemaBuilder.java`):
/// `[{"path":"port","required":true,"type":"Int","description":"..."?}, ...]`
/// Returns the validation errors as JSON:
/// `[{"path":..,"expected":..,"actual":..,"kind":"Missing"|"WrongType"|"InvalidValue"}, ...]`
/// (an empty `[]` means the schema passed).
#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_schemaValidate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
    fields_json: JString<'local>,
) -> jstring {
    let h = match unsafe { as_read(handle) } {
        Some(h) => h,
        None => {
            set_exception(&mut env, "schemaValidate: null handle");
            return std::ptr::null_mut();
        }
    };
    let spec = match jstr(&mut env, fields_json) {
        Some(s) => s,
        None => {
            set_exception(&mut env, "schemaValidate: fields spec is null");
            return std::ptr::null_mut();
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&spec) {
        Ok(v) => v,
        Err(e) => {
            set_exception(&mut env, &format!("schemaValidate: invalid fields JSON: {}", e));
            return std::ptr::null_mut();
        }
    };
    let entries = match parsed.as_array() {
        Some(a) => a,
        None => {
            set_exception(&mut env, "schemaValidate: fields JSON must be an array");
            return std::ptr::null_mut();
        }
    };

    let mut builder = SchemaBuilder::new();
    for entry in entries {
        let path = match entry.get("path").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => {
                set_exception(&mut env, "schemaValidate: field entry missing 'path'");
                return std::ptr::null_mut();
            }
        };
        let required = entry.get("required").and_then(|v| v.as_bool()).unwrap_or(true);
        let ty = expected_type_from_str(
            entry.get("type").and_then(|v| v.as_str()).unwrap_or("Any"),
        );
        builder = if required { builder.require(path, ty) } else { builder.optional(path, ty) };
        if let Some(desc) = entry.get("description").and_then(|v| v.as_str()) {
            builder = builder.with_description(desc);
        }
    }

    let report = builder.validate(&h.data);
    let errors: Vec<serde_json::Value> = report
        .errors
        .iter()
        .map(|e| {
            serde_json::json!({
                "path": e.path,
                "expected": e.expected,
                "actual": e.actual,
                "kind": e.kind.to_string(),
            })
        })
        .collect();
    match serde_json::to_string(&errors) {
        Ok(s) => to_jstring(&mut env, &s),
        Err(e) => {
            set_exception(&mut env, &format!("schemaValidate: {}", e));
            std::ptr::null_mut()
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// HotReload — poll-based file watching
// ════════════════════════════════════════════════════════════════════════════
//
// `dixscript::Runtime::HotReloadWatcher` is deliberately a single-file,
// `std::fs::metadata` poll (see `hot_reload.rs`'s own doc comment for why:
// no notify/inotify/FSEvents/ReadDirectoryChangesW dependency, identical
// behavior on every target DixScript ships native bindings to, including
// targets with no FS-event backend at all) — this binds it directly rather
// than reimplementing anything watch-related with `java.nio.file.WatchService`.
// Call `checkAndReload()` from a game loop / scheduled task, same as the
// Rust doc example on `HotReloadWatcher` itself.
//
// NOTE: `HotReloadWatcher::force_reload()` always calls `DixLoader::load_text()`
// internally, never `load_encrypted()` — encrypted `.mdix` files are not
// supported by hot reload in the core Runtime yet, so this binding can't
// support them either. `Database.getEnumField`-style silent behavior
// differences are avoided here on purpose: `HotReload.java`'s doc comment
// says this plainly rather than letting it surface as a confusing runtime
// error only when someone tries it.

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherNew<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) -> jlong {
    let p = match jstr(&mut env, path) {
        Some(s) => s,
        None => {
            set_exception(&mut env, "watcherNew: path is null");
            return 0;
        }
    };
    box_watcher(HotReloadWatcher::new(p))
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherFree<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) {
    unsafe { free_watcher(handle) };
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherPath<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jstring {
    let w = match unsafe { as_watcher(handle) } {
        Some(w) => w,
        None => {
            set_exception(&mut env, "watcherPath: null handle");
            return std::ptr::null_mut();
        }
    };
    let p = w.path().to_string_lossy().into_owned();
    to_jstring(&mut env, &p)
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherHasLoaded<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jboolean {
    match unsafe { as_watcher(handle) } {
        Some(w) => if w.has_loaded() { JNI_TRUE } else { JNI_FALSE },
        None => {
            set_exception(&mut env, "watcherHasLoaded: null handle");
            JNI_FALSE
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherHasChanged<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jboolean {
    let w = match unsafe { as_watcher(handle) } {
        Some(w) => w,
        None => {
            set_exception(&mut env, "watcherHasChanged: null handle");
            return JNI_FALSE;
        }
    };
    match w.has_changed() {
        Ok(b) => if b { JNI_TRUE } else { JNI_FALSE },
        Err(e) => {
            set_exception(&mut env, &format!("watcherHasChanged: {}", e));
            JNI_FALSE
        }
    }
}

/// Returns a new read handle (jlong) when the file changed and reloaded
/// successfully, or `0` with *no* pending exception when the file is
/// unchanged. A reload failure also returns `0`, but *with* a pending
/// exception — callers must check for a pending exception before treating a
/// `0` return as "unchanged" (`HotReload.java` does this for you).
#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherCheckAndReload<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jlong {
    let w = match unsafe { as_watcher_mut(handle) } {
        Some(w) => w,
        None => {
            set_exception(&mut env, "watcherCheckAndReload: null handle");
            return 0;
        }
    };
    match w.check_and_reload() {
        Ok(Some(data)) => box_read(data),
        Ok(None) => 0,
        Err(e) => {
            set_exception(&mut env, &format!("watcherCheckAndReload: {}", e));
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_midmanstudio_dixscript_internal_MdixNative_watcherForceReload<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    handle: jlong,
) -> jlong {
    let w = match unsafe { as_watcher_mut(handle) } {
        Some(w) => w,
        None => {
            set_exception(&mut env, "watcherForceReload: null handle");
            return 0;
        }
    };
    match w.force_reload() {
        Ok(data) => box_read(data),
        Err(e) => {
            set_exception(&mut env, &format!("watcherForceReload: {}", e));
            0
        }
    }
}
