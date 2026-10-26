use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use jni::{
    InitArgsBuilder, JavaVM, jni_sig, jni_str,
    objects::{Global, JObject, JString, JValue},
    signature::RuntimeFieldSignature,
    strings::JNIString,
};

pub use mcje_macros::*;

pub mod client;

/// Minecraft version the embedded jar was downloaded for.
pub const VERSION: &str = env!("MCJE_VERSION");
/// Assets directory (`<download>/assets`, containing `indexes/` and `objects/`).
pub const ASSETS_DIR: &str = env!("MCJE_ASSETS_DIR");
/// Asset index id matching [`ASSETS_DIR`].
pub const ASSET_INDEX: &str = env!("MCJE_ASSET_INDEX");
/// Extracted LWJGL/SDL native libraries.
pub const NATIVES_DIR: &str = env!("MCJE_NATIVES_DIR");

pub async fn init() -> JavaVM {
    let classpath = env!("MCJE_JVM_CLASSPATH");
    let natives_dir = env!("MCJE_NATIVES_DIR");

    let jvm_args = InitArgsBuilder::new()
        .option(format!("-Djava.class.path={classpath}"))
        .option(format!("-Djava.library.path={natives_dir}"))
        .option(format!("-Dorg.lwjgl.librarypath={natives_dir}"))
        .build()
        .unwrap();

    JavaVM::new(jvm_args).unwrap()
}

const SERVER_THREAD_NAME: &str = "Server thread";

/// Handle to an embedded vanilla dedicated server.
pub struct ServerHandle {
    shutdown: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    server_exited: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// True when the server thread died unexpectedly.
    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    /// Call right before sending `stop`, so the failure monitor stays quiet.
    pub fn mark_stopping(&self) {
        self.stopping.store(true, Ordering::SeqCst);
    }

    /// True once the dedicated server's `main` method has returned.
    pub fn server_exited(&self) -> bool {
        self.server_exited.load(Ordering::SeqCst)
    }

    /// Release the keep-alive thread and wait for it to detach.
    pub fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Run `net.minecraft.server.Main` on a new attached thread.
///
/// The thread stays attached (non-daemon, keeping the JVM alive) until
/// [`ServerHandle::shutdown`] is called, so the process exits only when we
/// decide it should.
pub fn spawn_dedicated_server(jvm: JavaVM, args: Vec<String>) -> ServerHandle {
    let shutdown = Arc::new(AtomicBool::new(false));
    let stopping = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let server_exited = Arc::new(AtomicBool::new(false));

    let thread = {
        let shutdown = shutdown.clone();
        let stopping = stopping.clone();
        let failed = failed.clone();
        let server_exited = server_exited.clone();
        std::thread::spawn(move || {
            let result = jvm.attach_current_thread(|env| {
                let result = run_server_main(env, &args);
                if result.is_err() {
                    failed.store(true, Ordering::SeqCst);
                }
                server_exited.store(true, Ordering::SeqCst);
                result?;
                monitor_server_thread(env, &shutdown, &stopping, &failed);
                Ok::<_, jni::errors::Error>(())
            });
            if let Err(err) = result {
                eprintln!("mcje: dedicated server thread failed: {err}");
                failed.store(true, Ordering::SeqCst);
            }
            server_exited.store(true, Ordering::SeqCst);
        })
    };

    ServerHandle {
        shutdown,
        stopping,
        failed,
        server_exited,
        thread: Some(thread),
    }
}

fn run_server_main(env: &mut jni::Env, args: &[String]) -> Result<(), jni::errors::Error> {
    let string_class = env.find_class(jni_str!("java/lang/String"))?;
    let array = env.new_object_array(args.len() as i32, &string_class, JObject::null())?;
    for (index, arg) in args.iter().enumerate() {
        let value = env.new_string(arg)?;
        array.set_element(env, index, &value)?;
    }
    let array = JObject::from(array);
    env.call_static_method(
        jni_str!("net/minecraft/server/Main"),
        jni_str!("main"),
        jni_sig!("([Ljava/lang/String;)V"),
        &[JValue::Object(&array)],
    )?;
    Ok(())
}

fn monitor_server_thread(
    env: &mut jni::Env,
    shutdown: &AtomicBool,
    stopping: &AtomicBool,
    failed: &AtomicBool,
) {
    let mut server_thread: Option<Global<JObject<'static>>> = None;
    let mut missing_since: Option<std::time::Instant> = None;
    let mut last_enumerate = std::time::Instant::now();

    while !shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(250));
        if stopping.load(Ordering::SeqCst) {
            continue;
        }

        if let Some(thread) = &server_thread {
            let alive = env
                .call_method(thread.as_obj(), jni_str!("isAlive"), jni_sig!("()Z"), &[])
                .map(|value| value.z().unwrap_or(false))
                .unwrap_or(false);
            if !alive {
                if !failed.load(Ordering::SeqCst) {
                    eprintln!("mcje: server thread exited unexpectedly");
                    failed.store(true, Ordering::SeqCst);
                }
                server_thread = None;
            }
            continue;
        }

        if last_enumerate.elapsed() >= Duration::from_secs(1) {
            last_enumerate = std::time::Instant::now();
            if let Some(found) = find_thread_by_name(env, SERVER_THREAD_NAME) {
                server_thread = Some(found);
                missing_since = None;
                continue;
            }
        }

        if missing_since.is_none() {
            missing_since = Some(std::time::Instant::now());
        } else if let Some(since) = missing_since
            && since.elapsed() >= Duration::from_secs(5)
            && !failed.load(Ordering::SeqCst)
        {
            eprintln!("mcje: server never started (no '{SERVER_THREAD_NAME}' after 5s)");
            failed.store(true, Ordering::SeqCst);
        }
    }
}

fn find_thread_by_name(env: &mut jni::Env, name: &str) -> Option<Global<JObject<'static>>> {
    let traces = env
        .call_static_method(
            jni_str!("java/lang/Thread"),
            jni_str!("getAllStackTraces"),
            jni_sig!("()Ljava/util/Map;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;
    let key_set = env
        .call_method(
            &traces,
            jni_str!("keySet"),
            jni_sig!("()Ljava/util/Set;"),
            &[],
        )
        .ok()?
        .l()
        .ok()?;

    let mut found = None;
    iterate(&key_set, env, |_, thread, env| {
        let Some(thread_name) = (|| {
            let value = env
                .call_method(
                    &thread,
                    jni_str!("getName"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .ok()?
                .l()
                .ok()?;
            let cast = env.as_cast::<JString>(&value).ok()?;
            cast.try_to_string(env).ok()
        })() else {
            return;
        };
        if thread_name == name {
            found = env.new_global_ref(thread).ok();
        }
    });
    found
}

pub fn bootstrap(env: &mut jni::Env) {
    let detected_version_built_in = env
        .get_static_field(
            jni_str!("net/minecraft/DetectedVersion"),
            jni_str!("BUILT_IN"),
            jni_sig!("Lnet/minecraft/WorldVersion;"),
        )
        .unwrap()
        .l()
        .unwrap();

    env.call_static_method(
        jni_str!("net/minecraft/SharedConstants"),
        jni_str!("setVersion"),
        jni_sig!("(Lnet/minecraft/WorldVersion;)V"),
        &[JValue::from(&detected_version_built_in)],
    )
    .unwrap();

    env.call_static_method(
        jni_str!("net/minecraft/server/Bootstrap"),
        jni_str!("bootStrap"),
        jni_sig!("()V"),
        &[],
    )
    .unwrap();
}

pub fn get_registry<'a>(env: &mut jni::Env<'a>, name: &str, jtype: &str) -> JObject<'a> {
    let built_in_registries = env
        .find_class(jni_str!("net/minecraft/core/registries/BuiltInRegistries"))
        .unwrap();

    let jni_name = JNIString::new(name);
    let runtime_sig =
        RuntimeFieldSignature::from_str(format!("Lnet/minecraft/core/{jtype};")).unwrap();
    let sig = runtime_sig.field_signature();

    env.get_static_field(built_in_registries, &jni_name, &sig)
        .unwrap()
        .l()
        .unwrap()
}

pub fn iterate<'a, 'env>(
    obj: &JObject<'a>,
    env: &'env mut jni::Env<'a>,
    mut cb: impl FnMut(usize, JObject<'a>, &mut jni::Env<'a>),
) {
    let iterator = env
        .call_method(
            obj,
            jni_str!("iterator"),
            jni_sig!("()Ljava/util/Iterator;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let mut i = 0;
    loop {
        let has_next = env
            .call_method(&iterator, jni_str!("hasNext"), jni_sig!("()Z"), &[])
            .unwrap()
            .z()
            .unwrap();
        if !has_next {
            break;
        }

        let element = env
            .call_method(
                &iterator,
                jni_str!("next"),
                jni_sig!("()Ljava/lang/Object;"),
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        cb(i, element, env);

        i += 1;
    }
}
