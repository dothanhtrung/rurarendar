//! JNI helpers for calls that start on a Rust thread, not inside a JNI upcall.
//!
//! `android-activity` stores the `Application` in `ndk_context` and only installs the app
//! class loader on the `android_main` thread. Sync runs on a `std::thread`, so `FindClass`
//! (what `call_static_method` does for a string class name) searches the boot loader and
//! cannot see `SecretBridge`. `Context.getClassLoader()` is the app loader and can see it.

use jni::{
    JavaVM,
    jni_sig,
    jni_str,
    objects::{
        JClass,
        JClassLoader,
        JObject,
        LoaderContext,
    },
    strings::JNIString,
};

/// Attach this thread and run `body` with the process `Application`.
pub fn with_env<T, F>(body: F) -> jni::errors::Result<T>
where
    F: for<'local> FnOnce(&mut jni::Env<'local>, &JObject<'local>) -> jni::errors::Result<T>,
{
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) };
    vm.attach_current_thread(|env| {
        let application = unsafe { JObject::from_raw(env, context.context().cast()) };
        body(env, &application)
    })
}

/// Load an app class by binary name (`net.onlyesterday.rurarendar.SecretBridge`).
pub fn load_app_class<'local>(
    env: &mut jni::Env<'local>,
    application: &JObject<'_>,
    binary_name: &str,
) -> jni::errors::Result<JClass<'local>> {
    let loader = env
        .call_method(
            application,
            jni_str!("getClassLoader"),
            jni_sig!(() -> "java.lang.ClassLoader"),
            &[],
        )?
        .l()?;
    let loader = env.as_cast::<JClassLoader>(&loader)?;
    let name = JNIString::from(binary_name);
    LoaderContext::Loader(loader.as_ref()).load_class(env, &name, true)
}
